use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    env, fs,
    path::{Path, PathBuf},
};

use ab_glyph::{point, Font, FontArc, PxScale, ScaleFont};
use tiny_skia::{Color, Paint, PathBuilder, PixmapMut, Rect, Transform};
use ttf_parser::{Face, RasterImageFormat};

use crate::{
    applications::DesktopApplication,
    binary::Binary,
    config::{parse_color, Config},
    history::History,
    plugins::PluginManager,
    search, Action, SearchResult,
};

pub struct UiModel {
    pub input: String,
    pub results: Vec<SearchResult>,
    pub selected: usize,
    pub exit: bool,
    pub prompt: String,
    selection_moved_down: bool,
    help_mode: bool,
    password: bool,
    cursor: usize,
    config: Config,
    applications: Vec<DesktopApplication>,
    application_results: Vec<SearchResult>,
    binaries: Option<Vec<Binary>>,
    binary_results: Option<Vec<SearchResult>>,
    history: History,
    plugins: Option<PluginManager>,
    dmenu: Option<DmenuState>,
}

struct DmenuState {
    items: Vec<String>,
    prompt: String,
    print_query: bool,
    fuzzy: bool,
    case_sensitive: bool,
}

pub struct DmenuModelOptions {
    pub prompt: String,
    pub placeholder: String,
    pub initial_query: String,
    pub print_query: bool,
    pub fuzzy: bool,
    pub case_sensitive: bool,
    pub password: bool,
}

impl UiModel {
    fn build_app_results(
        applications: &[DesktopApplication],
        history: &History,
        config: &Config,
    ) -> Vec<SearchResult> {
        let use_history = config.general.remember_history && config.history.enabled;
        let now = if use_history {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        } else {
            0
        };
        applications
            .iter()
            .map(|app| {
                app.as_result(if use_history {
                    history.score_with_now(&app.id, now)
                } else {
                    0
                })
            })
            .collect()
    }

    pub fn new(config: Config) -> Self {
        let history = History::load_with_limit(config.history.max_entries);
        let plugins = PluginManager::from_config(&config);
        let applications = crate::applications::discover();
        let application_results = Self::build_app_results(&applications, &history, &config);
        let mut model = Self {
            input: String::new(),
            results: Vec::new(),
            selected: 0,
            exit: false,
            prompt: "Search applications…".into(),
            selection_moved_down: false,
            help_mode: false,
            password: false,
            cursor: 0,
            config,
            applications,
            application_results,
            binaries: None,
            binary_results: None,
            history,
            plugins: Some(plugins),
            dmenu: None,
        };
        model.refresh();
        model
    }

    pub fn dmenu(config: Config, items: Vec<String>, options: DmenuModelOptions) -> Self {
        let history = History::load_with_limit(0);
        let DmenuModelOptions {
            prompt,
            placeholder,
            initial_query,
            print_query,
            fuzzy,
            case_sensitive,
            password,
        } = options;
        let cursor = initial_query.len();
        let mut model = Self {
            input: initial_query,
            results: Vec::new(),
            selected: 0,
            exit: false,
            prompt: placeholder,
            selection_moved_down: false,
            help_mode: false,
            password,
            cursor,
            config,
            applications: Vec::new(),
            application_results: Vec::new(),
            binaries: None,
            binary_results: None,
            history,
            plugins: None,
            dmenu: Some(DmenuState {
                items,
                prompt,
                print_query,
                fuzzy,
                case_sensitive,
            }),
        };
        model.refresh();
        model
    }

    pub fn insert(&mut self, text: &str) {
        self.input.insert_str(self.cursor, text);
        self.cursor += text.len();
        self.refresh();
    }
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let previous = self.input[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(index, _)| index)
                .unwrap_or(0);
            self.input.drain(previous..self.cursor);
            self.cursor = previous;
        }
        self.refresh();
    }
    pub fn clear(&mut self) {
        self.input.clear();
        self.cursor = 0;
        self.refresh();
    }
    pub fn next(&mut self) {
        if !self.results.is_empty() {
            self.selected = (self.selected + 1) % self.results.len();
            self.selection_moved_down = true;
        }
    }
    pub fn previous(&mut self) {
        if !self.results.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or(self.results.len() - 1);
            self.selection_moved_down = false;
        }
    }
    pub fn select(&mut self, index: usize) {
        if index < self.results.len() {
            self.selection_moved_down = index >= self.selected;
            self.selected = index;
        }
    }
    pub fn scroll_down(&mut self) {
        self.scroll_down_by(1);
    }
    pub fn scroll_up(&mut self) {
        self.scroll_up_by(1);
    }
    pub fn scroll_down_by(&mut self, steps: usize) {
        if !self.results.is_empty() {
            let max_index = self.results.len() - 1;
            self.selected = (self.selected + steps).min(max_index);
            self.selection_moved_down = true;
        }
    }
    pub fn scroll_up_by(&mut self, steps: usize) {
        if !self.results.is_empty() {
            self.selected = self.selected.saturating_sub(steps);
            self.selection_moved_down = false;
        }
    }
    pub fn autocomplete(&mut self) {
        if self.help_mode {
            return;
        }
        if let Some(item) = self.results.get(self.selected) {
            if item.is_pending() {
                return;
            }
            self.input = if self.dmenu.is_some() {
                item.title.clone()
            } else if self.input.starts_with(&self.config.binary.prefix) {
                let separator = if self.config.binary.prefix.ends_with(' ') {
                    ""
                } else {
                    " "
                };
                format!("{}{separator}{}", self.config.binary.prefix, item.title)
            } else if let Some((plugin, _)) = self
                .plugins
                .as_ref()
                .and_then(|plugins| plugins.select(&self.input))
            {
                let separator = if plugin
                    .prefix()
                    .chars()
                    .last()
                    .is_some_and(char::is_alphanumeric)
                {
                    " "
                } else {
                    ""
                };
                let completion = item
                    .metadata
                    .as_deref()
                    .and_then(|value| {
                        value.strip_prefix(waymenu_plugin_api::AUTOCOMPLETE_METADATA_PREFIX)
                    })
                    .unwrap_or(&item.title);
                format!("{}{separator}{completion}", plugin.prefix())
            } else {
                item.title.clone()
            };
            self.cursor = self.input.len();
            self.refresh();
        }
    }

    pub fn delete_word_backward(&mut self) {
        let target = self.word_left_index();
        if target < self.cursor {
            self.input.drain(target..self.cursor);
            self.cursor = target;
            self.refresh();
        }
    }

    pub fn delete_word(&mut self) {
        self.delete_word_backward();
    }

    pub fn delete_forward(&mut self) {
        if self.cursor < self.input.len() {
            let next = self.input[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(index, _)| self.cursor + index)
                .unwrap_or(self.input.len());
            self.input.drain(self.cursor..next);
            self.refresh();
        }
    }

    pub fn delete_word_forward(&mut self) {
        let target = self.word_right_index();
        if target > self.cursor {
            self.input.drain(self.cursor..target);
            self.refresh();
        }
    }

    pub fn move_to_beginning(&mut self) {
        self.cursor = 0;
    }
    pub fn move_to_end(&mut self) {
        self.cursor = self.input.len();
    }
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            let previous = self.input[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(index, _)| index)
                .unwrap_or(0);
            self.cursor = previous;
        }
    }
    pub fn move_right(&mut self) {
        if self.cursor < self.input.len() {
            let next = self.input[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(index, _)| self.cursor + index)
                .unwrap_or(self.input.len());
            self.cursor = next;
        }
    }
    pub fn move_word_left(&mut self) {
        self.cursor = self.word_left_index();
    }
    pub fn move_word_right(&mut self) {
        self.cursor = self.word_right_index();
    }

    fn word_left_index(&self) -> usize {
        if self.cursor == 0 {
            return 0;
        }
        let prefix = &self.input[..self.cursor];
        let mut chars = prefix.char_indices().rev().peekable();

        while let Some(&(_, c)) = chars.peek() {
            if char_category(c) == CharCategory::Whitespace {
                chars.next();
            } else {
                break;
            }
        }

        let Some(&(_, first_char)) = chars.peek() else {
            return 0;
        };
        let target_category = char_category(first_char);

        let mut last_index = 0;
        while let Some(&(index, c)) = chars.peek() {
            if char_category(c) == target_category {
                last_index = index;
                chars.next();
            } else {
                break;
            }
        }

        last_index
    }

    fn word_right_index(&self) -> usize {
        if self.cursor >= self.input.len() {
            return self.input.len();
        }
        let suffix = &self.input[self.cursor..];
        let mut chars = suffix.char_indices().peekable();

        while let Some(&(_, c)) = chars.peek() {
            if char_category(c) == CharCategory::Whitespace {
                chars.next();
            } else {
                break;
            }
        }

        let Some(&(_, first_char)) = chars.peek() else {
            return self.input.len();
        };
        let target_category = char_category(first_char);

        while let Some(&(_, c)) = chars.peek() {
            if char_category(c) == target_category {
                chars.next();
            } else {
                break;
            }
        }

        chars
            .peek()
            .map(|&(index, _)| self.cursor + index)
            .unwrap_or(self.input.len())
    }
    pub fn cursor_prefix(&self) -> Cow<'_, str> {
        if self.help_mode {
            Cow::Borrowed("")
        } else if self.password {
            let count = self.input[..self.cursor.min(self.input.len())]
                .chars()
                .count();
            Cow::Owned("•".repeat(count))
        } else {
            let end = self.cursor.min(self.input.len());
            Cow::Borrowed(&self.input[..end])
        }
    }

    pub fn toggle_help(&mut self) {
        if self.dmenu.is_some() {
            return;
        }
        self.help_mode = !self.help_mode;
        self.refresh();
    }

    pub fn is_help_mode(&self) -> bool {
        self.help_mode
    }

    pub fn help_binding(&self) -> &str {
        &self.config.keybindings.help
    }

    fn displayed_input(&self) -> Cow<'_, str> {
        if self.help_mode {
            Cow::Borrowed("Help · Keybindings and plugins")
        } else if self.input.is_empty() {
            Cow::Borrowed(&self.prompt)
        } else if self.password {
            Cow::Owned("•".repeat(self.input.chars().count()))
        } else {
            Cow::Borrowed(&self.input)
        }
    }

    fn highlight_query(&self) -> &str {
        if self.help_mode {
            return "";
        }
        if self.dmenu.is_some() {
            return &self.input;
        }
        if let Some((_, query)) = self
            .plugins
            .as_ref()
            .and_then(|plugins| plugins.select(&self.input))
        {
            return query;
        }
        self.input
            .strip_prefix(&self.config.binary.prefix)
            .map(str::trim_start)
            .or_else(|| {
                self.input
                    .strip_prefix(&self.config.plugins.shell_prefix)
                    .map(str::trim_start)
            })
            .unwrap_or(&self.input)
    }

    pub fn selected_action(&self) -> Action {
        if self.help_mode {
            return Action::None;
        }
        if let Some(dmenu) = &self.dmenu {
            if self.password {
                return Action::None;
            }
            return self
                .results
                .get(self.selected)
                .map(|item| Action::CopyText(item.title.clone()))
                .or_else(|| {
                    dmenu
                        .print_query
                        .then(|| Action::CopyText(self.input.clone()))
                })
                .unwrap_or(Action::None);
        }
        let Some(result) = self.results.get(self.selected) else {
            return Action::None;
        };
        if let Some((plugin, _)) = self
            .plugins
            .as_ref()
            .and_then(|plugins| plugins.select(&self.input))
        {
            return plugin.action(result);
        }
        if self.input.starts_with(&self.config.binary.prefix) {
            Action::LaunchBinary(result.id.clone())
        } else {
            Action::LaunchApplication(result.id.clone())
        }
    }

    pub(crate) fn delete_selected_result(&mut self) {
        if self.help_mode || self.dmenu.is_some() {
            return;
        }
        let Some(result) = self.results.get(self.selected) else {
            return;
        };
        let Some((plugin, _)) = self
            .plugins
            .as_ref()
            .and_then(|plugins| plugins.select(&self.input))
        else {
            return;
        };
        if !plugin.delete_result(result) {
            return;
        }
        let selected = self.selected;
        self.refresh();
        self.selected = selected.min(self.results.len().saturating_sub(1));
    }

    pub fn record_launch(&mut self, id: &str) {
        if self.config.history.enabled {
            let _ = self.history.record(id);
            self.application_results =
                Self::build_app_results(&self.applications, &self.history, &self.config);
        }
    }

    pub fn show_shell_running(&mut self, command: &str) {
        self.results = vec![SearchResult {
            id: command.into(),
            title: command.into(),
            description: "Running…".into(),
            score: 0,
            icon: None,
            image: None,
            metadata: Some("shell-running".into()),
        }];
        self.selected = 0;
        self.selection_moved_down = false;
    }

    pub fn show_shell_output(&mut self, command: &str, output: &str, success: bool) {
        let mut lines: Vec<_> = output
            .lines()
            .filter(|line| !line.is_empty())
            .take(self.config.general.max_results)
            .enumerate()
            .map(|(index, line)| SearchResult {
                id: command.into(),
                title: if index == 0 {
                    command.into()
                } else {
                    format!("Output {}", index + 1)
                },
                description: line.chars().take(240).collect(),
                score: 0,
                icon: None,
                image: None,
                metadata: Some(
                    if success {
                        "shell-output"
                    } else {
                        "shell-error"
                    }
                    .into(),
                ),
            })
            .collect();
        if lines.is_empty() {
            lines.push(SearchResult {
                id: command.into(),
                title: command.into(),
                description: if success {
                    "Command completed successfully"
                } else {
                    "Command failed without output"
                }
                .into(),
                score: 0,
                icon: None,
                image: None,
                metadata: Some(
                    if success {
                        "shell-output"
                    } else {
                        "shell-error"
                    }
                    .into(),
                ),
            });
        }
        self.results = lines;
        self.selected = 0;
        self.selection_moved_down = false;
    }

    pub fn needs_binary_scan(&self) -> bool {
        self.dmenu.is_none()
            && self.input.starts_with(&self.config.binary.prefix)
            && self.binaries.is_none()
    }

    pub fn is_shell_input(&self) -> bool {
        self.input.starts_with(&self.config.plugins.shell_prefix)
    }

    pub fn set_binaries(&mut self, binaries: Vec<Binary>) {
        self.binary_results = Some(binaries.iter().map(Binary::as_result).collect());
        self.binaries = Some(binaries);
        self.refresh();
    }

    pub(crate) fn refresh_pending_results(&mut self) -> bool {
        if self.results.iter().any(SearchResult::is_pending) {
            self.refresh();
            return true;
        }
        false
    }

    fn refresh(&mut self) {
        self.selected = 0;
        self.selection_moved_down = false;
        if self.help_mode {
            self.results = self.help_results();
            return;
        }
        let max_search_limit = (self.config.general.max_results * 15).max(200);
        if let Some(dmenu) = &self.dmenu {
            if self.input.is_empty() {
                self.results = dmenu
                    .items
                    .iter()
                    .take(max_search_limit)
                    .enumerate()
                    .map(|(index, title)| SearchResult {
                        id: index.to_string(),
                        title: title.clone(),
                        description: String::new(),
                        score: 0,
                        icon: None,
                        image: None,
                        metadata: None,
                    })
                    .collect();
            } else if dmenu.fuzzy && !dmenu.case_sensitive {
                self.results = search::filter_strings(&dmenu.items, &self.input, max_search_limit);
            } else {
                let query = if dmenu.case_sensitive {
                    self.input.clone()
                } else {
                    self.input.to_lowercase()
                };
                self.results = dmenu
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, title)| {
                        if dmenu.case_sensitive {
                            if dmenu.fuzzy {
                                ordered_match(title, &query)
                            } else {
                                title.contains(&query)
                            }
                        } else {
                            if dmenu.fuzzy {
                                ordered_match_ignore_case(title, &query)
                            } else {
                                contains_ignore_case(title, &query)
                            }
                        }
                    })
                    .take(max_search_limit)
                    .map(|(index, title)| SearchResult {
                        id: index.to_string(),
                        title: title.clone(),
                        description: String::new(),
                        score: 0,
                        icon: None,
                        image: None,
                        metadata: None,
                    })
                    .collect();
            }
            return;
        }
        if let Some((plugin, query)) = self
            .plugins
            .as_ref()
            .and_then(|plugins| plugins.select(&self.input))
        {
            let id = plugin.id().to_owned();
            self.results = plugin.search(query);
            if let Some(plugins) = &self.plugins {
                plugins.unload_unused(Some(&id));
            }
            return;
        }
        if let Some(query) = self.input.strip_prefix(&self.config.binary.prefix) {
            let query = query.trim_start();
            let Some(binaries) = &self.binary_results else {
                self.results = vec![SearchResult {
                    id: String::new(),
                    title: "Scanning PATH…".into(),
                    description: "Executable discovery runs in the background".into(),
                    score: 0,
                    icon: None,
                    image: None,
                    metadata: Some("binary-loading".into()),
                }];
                return;
            };
            self.results = search::filter(binaries, query, max_search_limit);
            return;
        }
        if self.application_results.len() != self.applications.len() {
            self.application_results =
                Self::build_app_results(&self.applications, &self.history, &self.config);
        }
        self.results = search::filter(&self.application_results, &self.input, max_search_limit);
    }

    fn help_results(&self) -> Vec<SearchResult> {
        let keybindings = &self.config.keybindings;
        let mut rows = vec![
            help_row(
                "provider-applications",
                "Applications · default mode",
                "Search installed desktop applications without a prefix.",
            ),
            help_row(
                "provider-binaries",
                &format!("{} · Binary search", self.config.binary.prefix.trim_end()),
                "Search executable files available through PATH.",
            ),
        ];
        let plugins = self
            .plugins
            .as_ref()
            .map(PluginManager::metadata)
            .unwrap_or_default();
        let dynamic_count = plugins.iter().filter(|plugin| plugin.id != "shell").count();
        rows.extend(plugins.into_iter().map(|plugin| {
            help_row(
                &format!("plugin-{}", plugin.id),
                &format!("{} · {}", plugin.prefix.trim_end(), plugin.name),
                &format!("{}  (id: {})", plugin.description, plugin.id),
            )
        }));
        if dynamic_count == 0 {
            rows.push(help_row(
                "plugins-none",
                "No dynamic plugins installed",
                &format!(
                    "Put .so plugins in {}",
                    crate::config::config_dir().join("plugins").display()
                ),
            ));
        }
        rows.extend(
            [
                (
                    "execute",
                    "Execute selected result",
                    keybindings.execute.as_str(),
                ),
                ("close", "Close launcher", keybindings.close.as_str()),
                (
                    "autocomplete",
                    "Complete selected result",
                    keybindings.autocomplete.as_str(),
                ),
                (
                    "delete-result",
                    "Delete selected result in supporting plugins",
                    keybindings.delete_result.as_str(),
                ),
                ("next", "Next result", keybindings.next.as_str()),
                ("previous", "Previous result", keybindings.previous.as_str()),
                ("next-arrow", "Next result", keybindings.next_arrow.as_str()),
                (
                    "previous-arrow",
                    "Previous result",
                    keybindings.previous_arrow.as_str(),
                ),
                ("clear", "Clear input", keybindings.clear.as_str()),
                (
                    "backspace",
                    "Delete previous character",
                    keybindings.backspace.as_str(),
                ),
                (
                    "delete-word",
                    "Delete previous word",
                    keybindings.delete_word.as_str(),
                ),
                (
                    "delete-word-back",
                    "Delete previous word",
                    keybindings.delete_word_back.as_str(),
                ),
                (
                    "delete-forward",
                    "Delete next character",
                    keybindings.delete_forward.as_str(),
                ),
                (
                    "delete-word-forward",
                    "Delete next word",
                    keybindings.delete_word_forward.as_str(),
                ),
                (
                    "word-left",
                    "Move cursor word left",
                    keybindings.word_left.as_str(),
                ),
                (
                    "word-right",
                    "Move cursor word right",
                    keybindings.word_right.as_str(),
                ),
                (
                    "left-arrow",
                    "Move cursor left",
                    keybindings.left_arrow.as_str(),
                ),
                (
                    "right-arrow",
                    "Move cursor right",
                    keybindings.right_arrow.as_str(),
                ),
                (
                    "beginning",
                    "Move cursor to beginning",
                    keybindings.beginning.as_str(),
                ),
                ("end", "Move cursor to end", keybindings.end.as_str()),
                ("help", "Toggle this help", keybindings.help.as_str()),
            ]
            .into_iter()
            .map(|(id, action, key)| {
                help_row(
                    &format!("key-{id}"),
                    &format!("{key} · {action}"),
                    "Keyboard shortcut",
                )
            }),
        );
        rows
    }
}

fn help_row(id: &str, title: &str, description: &str) -> SearchResult {
    SearchResult {
        id: id.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        score: 0,
        icon: None,
        image: None,
        metadata: Some("help".into()),
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum CharCategory {
    Whitespace,
    Word,
    Punctuation,
}

fn char_category(c: char) -> CharCategory {
    if c.is_whitespace() {
        CharCategory::Whitespace
    } else if c.is_alphanumeric() || c == '_' {
        CharCategory::Word
    } else {
        CharCategory::Punctuation
    }
}

pub struct Renderer {
    font: Option<FontArc>,
    fallback_font: RefCell<Option<FontArc>>,
    fallback_attempted: Cell<bool>,
    emoji_font_data: RefCell<Option<Vec<u8>>>,
    emoji_attempted: Cell<bool>,
    emoji_cache: HashMap<(char, u32), Option<IconBitmap>>,
    emoji_order: VecDeque<(char, u32)>,
    icons: HashMap<String, Option<IconBitmap>>,
    icon_order: VecDeque<String>,
    theme: Theme,
    show_icons: bool,
    show_description: bool,
    font_size: f32,
    border_radius: f32,
    first_visible: usize,
    cursor_visible: bool,
}

struct IconBitmap {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub background: [u8; 4],
    pub input: [u8; 4],
    pub selected: [u8; 4],
    pub primary: [u8; 4],
    pub secondary: [u8; 4],
    pub highlight: [u8; 4],
    pub border: Option<[u8; 4]>,
}

impl Theme {
    pub const DARK: Self = Self {
        background: [25, 27, 34, 255],
        input: [43, 46, 57, 255],
        selected: [63, 69, 87, 255],
        primary: [245, 245, 248, 255],
        secondary: [166, 171, 185, 255],
        highlight: [105, 170, 255, 255],
        border: None,
    };

    pub const LIGHT: Self = Self {
        background: [242, 243, 247, 255],
        input: [255, 255, 255, 255],
        selected: [216, 224, 242, 255],
        primary: [27, 30, 38, 255],
        secondary: [91, 97, 112, 255],
        highlight: [0, 102, 204, 255],
        border: None,
    };

    pub fn from_config(config: &Config) -> Self {
        let mut theme = if config.general.theme.eq_ignore_ascii_case("light") {
            Self::LIGHT
        } else {
            Self::DARK
        };

        if let Some(bg) = config.colors.background.as_deref().and_then(parse_color) {
            theme.background = bg;
        }
        if let Some(inp) = config.colors.input.as_deref().and_then(parse_color) {
            theme.input = inp;
        }
        if let Some(sel) = config.colors.selected.as_deref().and_then(parse_color) {
            theme.selected = sel;
        }
        if let Some(pri) = config.colors.primary.as_deref().and_then(parse_color) {
            theme.primary = pri;
        }
        if let Some(sec) = config.colors.secondary.as_deref().and_then(parse_color) {
            theme.secondary = sec;
        }
        if let Some(hl) = config.colors.highlight.as_deref().and_then(parse_color) {
            theme.highlight = hl;
        }
        if let Some(border) = config.colors.border.as_deref().and_then(parse_color) {
            theme.border = Some(border);
        }

        theme
    }
}

impl Renderer {
    pub fn new(config: &Config) -> std::io::Result<Self> {
        let theme = Theme::from_config(config);
        let font = load_font(&config.appearance.font_family).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no usable TrueType/OpenType font was found",
            )
        })?;
        Ok(Self {
            font: Some(font),
            fallback_font: RefCell::new(None),
            fallback_attempted: Cell::new(false),
            emoji_font_data: RefCell::new(None),
            emoji_attempted: Cell::new(false),
            emoji_cache: HashMap::new(),
            emoji_order: VecDeque::new(),
            icons: HashMap::new(),
            icon_order: VecDeque::new(),
            theme,
            show_icons: config.appearance.show_icons,
            show_description: config.appearance.show_description,
            font_size: config.appearance.font_size,
            border_radius: config.appearance.border_radius,
            first_visible: 0,
            cursor_visible: true,
        })
    }

    fn ensure_fallback_font(&self) -> Option<FontArc> {
        if !self.fallback_attempted.get() {
            self.fallback_attempted.set(true);
            *self.fallback_font.borrow_mut() = load_fallback_font();
        }
        self.fallback_font.borrow().clone()
    }

    fn ensure_emoji_font_data(&self) -> bool {
        if !self.emoji_attempted.get() {
            self.emoji_attempted.set(true);
            *self.emoji_font_data.borrow_mut() = load_emoji_font_data();
        }
        self.emoji_font_data.borrow().is_some()
    }

    pub fn set_cursor_visible(&mut self, visible: bool) {
        self.cursor_visible = visible;
    }

    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    pub fn text_width(&self, text: &str, size: f32) -> f32 {
        let Some(font) = &self.font else { return 0.0 };
        let scaled = font.as_scaled(PxScale::from(size));
        let space_width = scaled.h_advance(scaled.glyph_id(' ')).max(size * 0.5);
        let tab_size = 8.0 * space_width;
        let mut x = 0.0;
        let mut previous = None;
        for ch in text.chars() {
            if ch == '\r'
                || ('\u{fe00}'..='\u{fe0f}').contains(&ch)
                || ('\u{e0100}'..='\u{e01ef}').contains(&ch)
            {
                continue;
            }
            if ch == '\n' {
                x += space_width;
                previous = None;
                continue;
            }
            if ch == '\t' {
                let next_tab = ((x / tab_size).floor() + 1.0) * tab_size;
                x = next_tab;
                previous = None;
                continue;
            }
            if ch < ' ' {
                continue;
            }
            if is_emoji(ch) {
                x += (size * 1.05).round() + 2.0;
                previous = None;
                continue;
            }
            let id = scaled.glyph_id(ch);
            if id.0 != 0 {
                if let Some(last) = previous {
                    x += scaled.kern(last, id);
                }
                x += scaled.h_advance(id);
                previous = Some(id);
            } else if let Some(fallback) = self.ensure_fallback_font() {
                let f_scaled = fallback.as_scaled(PxScale::from(size));
                let f_id = f_scaled.glyph_id(ch);
                if f_id.0 != 0 {
                    x += f_scaled.h_advance(f_id);
                    previous = None;
                } else {
                    x += space_width;
                    previous = None;
                }
            } else {
                x += space_width;
                previous = None;
            }
        }
        x
    }

    pub fn caret_x(&self, model: &UiModel) -> f32 {
        let prefix = model.cursor_prefix();
        32.0 + self.text_width(&prefix, self.font_size + 5.0)
    }

    fn fit_text<'a>(&self, text: &'a str, size: f32, width: f32) -> Cow<'a, str> {
        if self.text_width(text, size) <= width {
            return Cow::Borrowed(text);
        }

        let available = width - self.text_width("…", size);
        if available < 0.0 {
            return Cow::Borrowed("");
        }

        let (mut start, mut end) = (0, text.len());
        while start < end {
            let mid = start + (end - start).div_ceil(2);
            if self.text_width(&text[..text.floor_char_boundary(mid)], size) <= available {
                start = mid;
            } else {
                end = mid - 1;
            }
        }
        Cow::Owned(format!("{}…", &text[..text.floor_char_boundary(start)]))
    }

    pub fn first_visible(&self) -> usize {
        self.first_visible
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    pub fn item_at(
        &self,
        x: f64,
        y: f64,
        width: u32,
        height: u32,
        total_results: usize,
    ) -> Option<usize> {
        if x < 0.0 || x > width as f64 {
            return None;
        }
        if y < 72.0 {
            return None;
        }
        let visible = ((height.saturating_sub(110)) / 58).max(1) as usize;
        let row = ((y - 72.0) / 58.0) as usize;
        if row < visible {
            let index = self.first_visible + row;
            if index < total_results {
                return Some(index);
            }
        }
        None
    }

    pub fn draw(&mut self, canvas: &mut [u8], width: u32, height: u32, model: &UiModel) {
        let Some(mut pixmap) = PixmapMut::from_bytes(canvas, width, height) else {
            return;
        };
        pixmap.fill(Color::from_rgba8(
            self.theme.background[0],
            self.theme.background[1],
            self.theme.background[2],
            self.theme.background[3],
        ));
        let mut input_offset = 0.0;
        if let Some(dmenu) = &model.dmenu {
            let prompt = self.fit_text(dmenu.prompt.trim(), self.font_size, width as f32 / 3.0);
            if !prompt.is_empty() {
                input_offset = 20.0 + self.text_width(&prompt, self.font_size).ceil();
                self.text(
                    &mut pixmap,
                    &prompt,
                    20.0,
                    45.0,
                    self.font_size,
                    self.theme.primary,
                );
            }
        }
        fill_round_rect(
            &mut pixmap,
            12.0 + input_offset,
            12.0,
            width as f32 - 24.0 - input_offset,
            54.0,
            self.border_radius.min(27.0),
            self.theme.input,
        );
        let show_placeholder = model.input.is_empty() && !model.is_help_mode();
        let (input_x, input_color) = if show_placeholder {
            (36.0, self.theme.secondary)
        } else {
            (32.0, self.theme.primary)
        };
        let input_x = input_x + input_offset;
        let input_size = self.font_size + 5.0;
        let displayed_input = model.displayed_input();
        let input_text = if show_placeholder && model.dmenu.is_some() {
            self.fit_text(&displayed_input, input_size, width as f32 - input_x - 24.0)
        } else {
            Cow::Borrowed(displayed_input.as_ref())
        };
        self.text(
            &mut pixmap,
            &input_text,
            input_x,
            45.0,
            input_size,
            input_color,
        );
        if !model.is_help_mode() && self.cursor_visible {
            let caret_x = (self.caret_x(model) + input_offset).min(width as f32 - 32.0);
            fill_round_rect(
                &mut pixmap,
                caret_x,
                27.0,
                2.0,
                23.0,
                1.0,
                self.theme.primary,
            );
        }
        let visible = ((height.saturating_sub(110)) / 58).max(1) as usize;
        self.first_visible = result_window_start(
            self.first_visible,
            model.selected,
            model.results.len(),
            visible,
        );
        let first = self.first_visible;
        for (row, (index, result)) in model
            .results
            .iter()
            .enumerate()
            .skip(first)
            .take(visible)
            .enumerate()
        {
            let y = 78.0 + row as f32 * 58.0;
            if index == model.selected {
                fill_round_rect(
                    &mut pixmap,
                    12.0,
                    y - 6.0,
                    width as f32 - 24.0,
                    52.0,
                    self.border_radius.min(20.0),
                    self.theme.selected,
                );
            }
            let visual = result.icon.as_ref().or(result.image.as_ref());
            let text_x = if self.show_icons && visual.is_some() {
                74.0
            } else {
                28.0
            };
            if let Some(visual) = visual.filter(|_| self.show_icons) {
                self.draw_image(&mut pixmap, visual, 26, (y + 2.0).round() as i32, 36);
            }
            let (title_y, desc_y) = if result.description.is_empty() || !self.show_description {
                (y + 24.0, y + 24.0)
            } else {
                (y + 15.0, y + 35.0)
            };
            self.text_highlight(
                &mut pixmap,
                &result.title,
                (text_x, title_y),
                (self.font_size + 2.0, self.theme.primary),
                model.highlight_query(),
            );
            if self.show_description && !result.description.is_empty() {
                self.text(
                    &mut pixmap,
                    &result.description,
                    text_x,
                    desc_y,
                    self.font_size.max(10.0) - 2.0,
                    self.theme.secondary,
                );
            }
            if index == model.selected {
                if let Some(preview) = result.image.as_ref() {
                    if visual != Some(preview) {
                        self.draw_image(
                            &mut pixmap,
                            preview,
                            width.saturating_sub(118) as i32,
                            (y - 2.0) as i32,
                            48,
                        );
                    }
                }
            }
        }
        let footer = if model.is_help_mode() {
            format!(
                "Up/Down Scroll     {} Back     Esc Close Launcher",
                model.help_binding()
            )
        } else {
            "Up/Down Navigate     Tab Complete     Enter Open     F1 Help     Esc Close".into()
        };
        self.text(
            &mut pixmap,
            &footer,
            20.0,
            height as f32 - 16.0,
            11.0,
            self.theme.secondary,
        );
        if let Some(border_color) = self.theme.border {
            stroke_round_rect(
                &mut pixmap,
                (0.5, 0.5, width as f32 - 1.0, height as f32 - 1.0),
                self.border_radius,
                border_color,
                1.0,
            );
        }
    }

    fn draw_image(&mut self, pixmap: &mut PixmapMut<'_>, name: &str, x: i32, y: i32, size: u32) {
        if let Some(color_spec) = name.strip_prefix("color:") {
            if let Some(color) = parse_color(color_spec) {
                let radius = 6.0_f32.min(size as f32 / 4.0);
                if color[3] < 255 {
                    fill_round_rect(
                        pixmap,
                        x as f32,
                        y as f32,
                        size as f32,
                        size as f32,
                        radius,
                        [180, 180, 180, 255],
                    );
                }
                fill_round_rect(
                    pixmap,
                    x as f32,
                    y as f32,
                    size as f32,
                    size as f32,
                    radius,
                    [255, 255, 255, 40],
                );
                fill_round_rect(
                    pixmap,
                    (x + 1) as f32,
                    (y + 1) as f32,
                    (size.saturating_sub(2)) as f32,
                    (size.saturating_sub(2)) as f32,
                    (radius - 1.0).max(0.0),
                    color,
                );
                return;
            }
        }
        if let Some(ch) = name
            .strip_prefix("emoji:")
            .and_then(|s| s.chars().next())
            .or_else(|| {
                let mut chars = name.chars();
                let first = chars.next()?;
                if chars.next().is_none() && is_emoji(first) {
                    Some(first)
                } else {
                    None
                }
            })
        {
            if self.draw_emoji(pixmap, ch, x + 2, y + 2, size.saturating_sub(4)) {
                return;
            }
        }
        let cache_key = format!("{size}:{name}");
        if !self.icons.contains_key(&cache_key) {
            let bitmap = resolve_icon(name).and_then(|path| decode_image(&path, size));
            if self.icons.len() >= 256 {
                if let Some(oldest) = self.icon_order.pop_front() {
                    self.icons.remove(&oldest);
                }
            }
            self.icon_order.push_back(cache_key.clone());
            self.icons.insert(cache_key.clone(), bitmap);
        }
        let Some(Some(icon)) = self.icons.get(&cache_key) else {
            return;
        };
        let offset_x = (size.saturating_sub(icon.width) / 2) as i32;
        let offset_y = (size.saturating_sub(icon.height) / 2) as i32;
        for row in 0..icon.height {
            for column in 0..icon.width {
                let offset = ((row * icon.width + column) * 4) as usize;
                let alpha = icon.pixels[offset + 3];
                if alpha == 0 {
                    continue;
                }
                blend(
                    pixmap,
                    x + offset_x + column as i32,
                    y + offset_y + row as i32,
                    [
                        icon.pixels[offset],
                        icon.pixels[offset + 1],
                        icon.pixels[offset + 2],
                        alpha,
                    ],
                    1.0,
                );
            }
        }
    }

    fn draw_emoji(
        &mut self,
        pixmap: &mut PixmapMut<'_>,
        ch: char,
        x: i32,
        y: i32,
        size: u32,
    ) -> bool {
        let cache_key = (ch, size);
        if !self.emoji_cache.contains_key(&cache_key) {
            let bitmap = self.decode_emoji(ch, size);
            if self.emoji_cache.len() >= 64 {
                if let Some(oldest) = self.emoji_order.pop_front() {
                    self.emoji_cache.remove(&oldest);
                }
            }
            self.emoji_order.push_back(cache_key);
            self.emoji_cache.insert(cache_key, bitmap);
        }
        let Some(Some(icon)) = self.emoji_cache.get(&cache_key) else {
            return false;
        };
        for row in 0..icon.height {
            for column in 0..icon.width {
                let offset = ((row * icon.width + column) * 4) as usize;
                let alpha = icon.pixels[offset + 3];
                if alpha == 0 {
                    continue;
                }
                blend(
                    pixmap,
                    x + column as i32,
                    y + row as i32,
                    [
                        icon.pixels[offset],
                        icon.pixels[offset + 1],
                        icon.pixels[offset + 2],
                        alpha,
                    ],
                    alpha as f32 / 255.0,
                );
            }
        }
        true
    }

    fn decode_emoji(&self, ch: char, size: u32) -> Option<IconBitmap> {
        if !self.ensure_emoji_font_data() {
            return None;
        }
        let borrowed = self.emoji_font_data.borrow();
        let data = borrowed.as_deref()?;
        let face = Face::parse(data, 0).ok()?;
        let gid = face.glyph_index(ch)?;
        let raster = face.glyph_raster_image(gid, (size * 2) as u16)?;
        if raster.format != RasterImageFormat::PNG {
            return None;
        }
        let dynamic_img = image::load_from_memory(raster.data).ok()?;
        let resized = image::imageops::resize(
            &dynamic_img,
            size,
            size,
            image::imageops::FilterType::Triangle,
        );
        Some(IconBitmap {
            width: size,
            height: size,
            pixels: resized.into_raw(),
        })
    }

    fn text(
        &mut self,
        pixmap: &mut PixmapMut<'_>,
        text: &str,
        x: f32,
        baseline: f32,
        size: f32,
        color: [u8; 4],
    ) {
        self.text_highlight(pixmap, text, (x, baseline), (size, color), "");
    }

    fn text_highlight(
        &mut self,
        pixmap: &mut PixmapMut<'_>,
        text: &str,
        position: (f32, f32),
        style: (f32, [u8; 4]),
        query: &str,
    ) {
        let (start_x, baseline) = position;
        let mut x = start_x;
        let (size, color) = style;
        let Some(font) = self.font.clone() else {
            return;
        };
        let mut fallback_font: Option<FontArc> = None;
        let scaled = font.as_scaled(PxScale::from(size));
        let space_width = scaled.h_advance(scaled.glyph_id(' ')).max(size * 0.5);
        let tab_size = 8.0 * space_width;
        let mut previous = None;
        let query = query.to_lowercase();
        let mut needles = query.chars().peekable();
        let max_x = pixmap.width() as f32 - 10.0;
        let emoji_size = (size * 1.05).round() as u32;

        for ch in text.chars() {
            if x >= max_x {
                break;
            }
            if ch == '\r'
                || ('\u{fe00}'..='\u{fe0f}').contains(&ch)
                || ('\u{e0100}'..='\u{e01ef}').contains(&ch)
            {
                continue;
            }
            if ch == '\n' {
                x += space_width;
                previous = None;
                continue;
            }
            if ch == '\t' {
                let rel_x = x - start_x;
                let next_tab = ((rel_x / tab_size).floor() + 1.0) * tab_size;
                x = start_x + next_tab;
                previous = None;
                continue;
            }
            if ch < ' ' {
                continue;
            }
            let matched = needles
                .peek()
                .is_some_and(|needle| ch.to_lowercase().next() == Some(*needle));
            if matched {
                needles.next();
            }
            let glyph_color = if matched { self.theme.highlight } else { color };

            if is_emoji(ch) {
                let emoji_y = (baseline - size * 0.82).round() as i32;
                if self.draw_emoji(pixmap, ch, x.round() as i32, emoji_y, emoji_size) {
                    x += emoji_size as f32 + 2.0;
                    previous = None;
                    continue;
                }
            }

            let id = scaled.glyph_id(ch);
            if id.0 != 0 {
                if let Some(last) = previous {
                    x += scaled.kern(last, id);
                }
                let glyph = id.with_scale_and_position(size, point(x, baseline));
                if let Some(outline) = font.outline_glyph(glyph) {
                    let bounds = outline.px_bounds();
                    outline.draw(|gx, gy, coverage| {
                        blend(
                            pixmap,
                            bounds.min.x as i32 + gx as i32,
                            bounds.min.y as i32 + gy as i32,
                            glyph_color,
                            coverage,
                        );
                    });
                }
                x += scaled.h_advance(id);
                previous = Some(id);
            } else {
                let emoji_y = (baseline - size * 0.82).round() as i32;
                if self.draw_emoji(pixmap, ch, x.round() as i32, emoji_y, emoji_size) {
                    x += emoji_size as f32 + 2.0;
                    previous = None;
                    continue;
                }
                if fallback_font.is_none() && !self.fallback_attempted.get() {
                    fallback_font = self.ensure_fallback_font();
                }
                if let Some(fallback) = &fallback_font {
                    let f_scaled = fallback.as_scaled(PxScale::from(size));
                    let f_id = f_scaled.glyph_id(ch);
                    if f_id.0 != 0 {
                        let glyph = f_id.with_scale_and_position(size, point(x, baseline));
                        if let Some(outline) = fallback.outline_glyph(glyph) {
                            let bounds = outline.px_bounds();
                            outline.draw(|gx, gy, coverage| {
                                blend(
                                    pixmap,
                                    bounds.min.x as i32 + gx as i32,
                                    bounds.min.y as i32 + gy as i32,
                                    glyph_color,
                                    coverage,
                                );
                            });
                        }
                        x += f_scaled.h_advance(f_id);
                        previous = None;
                    } else {
                        x += space_width;
                        previous = None;
                    }
                } else {
                    x += space_width;
                    previous = None;
                }
            }
        }
    }
}

fn ordered_match(candidate: &str, query: &str) -> bool {
    let mut candidate = candidate.chars();
    query
        .chars()
        .all(|needle| candidate.any(|value| value == needle))
}

fn ordered_match_ignore_case(candidate: &str, query_lower: &str) -> bool {
    if candidate.is_ascii() && query_lower.is_ascii() {
        let mut cand_bytes = candidate.as_bytes().iter();
        return query_lower
            .as_bytes()
            .iter()
            .all(|&needle| cand_bytes.any(|b| b.to_ascii_lowercase() == needle));
    }
    let mut candidate = candidate.chars().flat_map(char::to_lowercase);
    query_lower
        .chars()
        .all(|needle| candidate.any(|value| value == needle))
}

fn contains_ignore_case(candidate: &str, query_lower: &str) -> bool {
    if candidate.is_ascii() && query_lower.is_ascii() {
        let q_bytes = query_lower.as_bytes();
        if q_bytes.is_empty() {
            return true;
        }
        if candidate.len() < q_bytes.len() {
            return false;
        }
        return candidate
            .as_bytes()
            .windows(q_bytes.len())
            .any(|w| w.eq_ignore_ascii_case(q_bytes));
    }
    candidate.to_lowercase().contains(query_lower)
}

fn result_window_start(
    current_first: usize,
    selected: usize,
    total: usize,
    visible: usize,
) -> usize {
    let visible = visible.max(1);
    let max_first = total.saturating_sub(visible);
    let mut first = current_first.min(max_first);
    if selected < first {
        first = selected;
    } else if selected >= first.saturating_add(visible) {
        first = selected
            .saturating_sub(visible.saturating_sub(1))
            .min(max_first);
    }
    first
}

fn load_font(family: &str) -> Option<FontArc> {
    let family = if family.eq_ignore_ascii_case("sans-serif") {
        "Inter"
    } else {
        family
    };
    let mono = family.to_ascii_lowercase().contains("mono");
    let candidates: &[&str] = if mono {
        &[
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
        ]
    } else {
        &[
            "/usr/share/fonts/inter/Inter.ttc",
            "/usr/share/fonts/inter/Inter-Regular.otf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
            "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/TTF/JetBrainsMonoNerdFont-Regular.ttf",
        ]
    };
    if family.eq_ignore_ascii_case("Inter") || family.eq_ignore_ascii_case("sans-serif") || mono {
        if let Some(font) = candidates
            .iter()
            .find_map(|path| read_font(Path::new(path)))
        {
            return Some(font);
        }
    }
    let matched = std::process::Command::new("fc-match")
        .args([
            "-f",
            "%{file}",
            &format!("{family}:charset=0041,00e9,1e9e,20ac"),
        ])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(PathBuf::from);
    if let Some(font) = matched.as_deref().and_then(read_font) {
        return Some(font);
    }
    candidates
        .iter()
        .find_map(|path| read_font(Path::new(path)))
        .or_else(find_any_system_font)
}

fn load_fallback_font() -> Option<FontArc> {
    for path in [
        "/usr/share/fonts/TTF/SymbolsNerdFont-Regular.ttf",
        "/usr/share/fonts/wps-fonts/seguiemj.ttf",
        "/usr/share/fonts/wps-fonts/seguisym.ttf",
        "/usr/share/fonts/truetype/noto/NotoEmoji-Regular.ttf",
        "/usr/share/fonts/noto/NotoEmoji-Regular.ttf",
        "/usr/share/fonts/truetype/ancient-scripts/Symbola.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
    ] {
        if let Some(font) = read_font(Path::new(path)) {
            return Some(font);
        }
    }
    let matched = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", "monospace:charset=2600,1f600"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(PathBuf::from);
    if let Some(font) = matched.as_deref().and_then(read_font) {
        return Some(font);
    }
    None
}

fn load_emoji_font_data() -> Option<Vec<u8>> {
    let matched = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", "emoji"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(PathBuf::from);
    if let Some(path) = matched.filter(|p| p.exists()) {
        if let Ok(bytes) = fs::read(&path) {
            return Some(bytes);
        }
    }
    for path in [
        "/usr/share/fonts/noto/NotoColorEmoji.ttf",
        "/usr/share/fonts/google-noto-color-emoji-fonts/NotoColorEmoji.ttf",
        "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
        "/usr/share/fonts/noto-color-emoji/NotoColorEmoji.ttf",
        "/usr/share/fonts/TTF/NotoColorEmoji.ttf",
    ] {
        if let Ok(bytes) = fs::read(path) {
            return Some(bytes);
        }
    }
    None
}

fn is_emoji(ch: char) -> bool {
    matches!(
        ch as u32,
        0x1F600..=0x1F64F
        | 0x1F300..=0x1F5FF
        | 0x1F680..=0x1F6FF
        | 0x1F700..=0x1F77F
        | 0x1F780..=0x1F7FF
        | 0x1F800..=0x1F8FF
        | 0x1F900..=0x1F9FF
        | 0x1FA00..=0x1FA6F
        | 0x1FA70..=0x1FAFF
        | 0x2600..=0x26FF
        | 0x2700..=0x27BF
        | 0x2300..=0x23FF
        | 0x2B50..=0x2B55
        | 0x1F1E6..=0x1F1FF
    )
}

fn read_font(path: &Path) -> Option<FontArc> {
    fs::read(path)
        .ok()
        .and_then(|bytes| FontArc::try_from_vec(bytes).ok())
}

fn find_any_system_font() -> Option<FontArc> {
    let mut pending = vec![PathBuf::from("/usr/share/fonts")];
    if let Some(home) = env::var_os("HOME") {
        pending.push(PathBuf::from(home).join(".local/share/fonts"));
    }
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    matches!(extension.to_ascii_lowercase().as_str(), "ttf" | "otf")
                })
            {
                if let Some(font) = read_font(&path) {
                    return Some(font);
                }
            }
        }
    }
    None
}

fn resolve_icon(name: &str) -> Option<PathBuf> {
    static RESOLVED_CACHE: std::sync::OnceLock<
        std::sync::RwLock<HashMap<String, Option<PathBuf>>>,
    > = std::sync::OnceLock::new();
    let cache = RESOLVED_CACHE.get_or_init(|| std::sync::RwLock::new(HashMap::new()));
    if let Ok(guard) = cache.read() {
        if let Some(cached) = guard.get(name) {
            return cached.clone();
        }
    }

    let resolved = resolve_icon_uncached(name);
    if let Ok(mut guard) = cache.write() {
        guard.insert(name.to_owned(), resolved.clone());
    }
    resolved
}

fn resolve_icon_uncached(name: &str) -> Option<PathBuf> {
    let direct = PathBuf::from(name);
    if direct.is_absolute() && direct.is_file() {
        return Some(direct);
    }
    let mut roots = Vec::new();
    if let Some(home) = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
    {
        roots.push(home);
    }
    let system =
        env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    roots.extend(env::split_paths(&system));
    let filenames = icon_filenames(name);
    let preferred_theme = preferred_icon_theme();
    for root in roots {
        for filename in &filenames {
            let candidate = root.join("pixmaps").join(filename);
            if candidate.is_file() {
                return Some(candidate);
            }
        }

        let icons_root = root.join("icons");
        let Ok(entries) = fs::read_dir(&icons_root) else {
            continue;
        };
        let mut themes: Vec<_> = entries
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        themes.sort_unstable();
        themes.sort_by_key(|theme| {
            if preferred_theme.as_deref() == Some(theme) {
                0
            } else if theme == "Adwaita" {
                1
            } else if theme == "hicolor" {
                2
            } else {
                3
            }
        });
        for theme in themes {
            let mut visited = std::collections::HashSet::new();
            if let Some(path) = find_icon_in_theme(&icons_root, &theme, &filenames, &mut visited) {
                return Some(path);
            }
        }
    }
    None
}

fn preferred_icon_theme() -> Option<String> {
    static THEME_CACHE: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    THEME_CACHE
        .get_or_init(preferred_icon_theme_uncached)
        .clone()
}

fn preferred_icon_theme_uncached() -> Option<String> {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    for settings_path in [
        config_home.join("gtk-4.0/settings.ini"),
        config_home.join("gtk-3.0/settings.ini"),
    ] {
        if let Ok(settings) = fs::read_to_string(settings_path) {
            if let Some(theme) = parse_gtk_icon_theme(&settings) {
                if !theme.is_empty() {
                    return Some(theme);
                }
            }
        }
    }
    None
}

fn parse_gtk_icon_theme(settings: &str) -> Option<String> {
    settings.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "gtk-icon-theme-name")
            .then(|| value.trim().trim_matches(['\'', '"']).to_owned())
    })
}

fn icon_filenames(name: &str) -> Vec<String> {
    let has_supported_extension = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["svg", "png", "webp", "jpg", "jpeg"]
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        });
    if has_supported_extension {
        vec![name.to_owned()]
    } else {
        ["svg", "png", "webp", "jpg", "jpeg"]
            .into_iter()
            .map(|extension| format!("{name}.{extension}"))
            .collect()
    }
}

#[derive(Clone, Default)]
struct ThemeIndexData {
    directories: Vec<String>,
    inherits: Vec<String>,
}

fn read_theme_index(theme_dir: &Path) -> Option<ThemeIndexData> {
    static THEME_INDEX_CACHE: std::sync::OnceLock<
        std::sync::RwLock<HashMap<PathBuf, Option<ThemeIndexData>>>,
    > = std::sync::OnceLock::new();
    let cache = THEME_INDEX_CACHE.get_or_init(|| std::sync::RwLock::new(HashMap::new()));
    if let Ok(guard) = cache.read() {
        if let Some(entry) = guard.get(theme_dir) {
            return entry.clone();
        }
    }
    let data = fs::read_to_string(theme_dir.join("index.theme"))
        .ok()
        .map(|index| {
            let mut directories = Vec::new();
            let mut inherits = Vec::new();
            for line in index.lines().map(str::trim) {
                if let Some(value) = line.strip_prefix("Directories=") {
                    directories.extend(
                        value
                            .split(',')
                            .map(str::trim)
                            .filter(|part| !part.is_empty())
                            .map(str::to_owned),
                    );
                } else if let Some(value) = line.strip_prefix("ScaledDirectories=") {
                    directories.extend(
                        value
                            .split(',')
                            .map(str::trim)
                            .filter(|part| !part.is_empty())
                            .map(str::to_owned),
                    );
                } else if let Some(value) = line.strip_prefix("Inherits=") {
                    inherits.extend(
                        value
                            .split(',')
                            .map(str::trim)
                            .filter(|part| !part.is_empty())
                            .map(str::to_owned),
                    );
                }
            }
            ThemeIndexData {
                directories,
                inherits,
            }
        });
    if let Ok(mut guard) = cache.write() {
        guard.insert(theme_dir.to_path_buf(), data.clone());
    }
    data
}

fn find_icon_in_theme(
    icons_root: &Path,
    theme: &str,
    filenames: &[String],
    visited: &mut std::collections::HashSet<String>,
) -> Option<PathBuf> {
    if !visited.insert(theme.to_owned()) {
        return None;
    }
    let theme_dir = icons_root.join(theme);
    let Some(index) = read_theme_index(&theme_dir) else {
        // Some user-installed hicolor directories only contain app assets and omit
        // index.theme. Search the standard app-icon buckets before giving up.
        for directory in [
            "scalable/apps",
            "512x512/apps",
            "256x256/apps",
            "128x128/apps",
            "64x64/apps",
            "48x48/apps",
            "32x32/apps",
            "24x24/apps",
            "22x22/apps",
            "16x16/apps",
        ] {
            for filename in filenames {
                let candidate = theme_dir.join(directory).join(filename);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        return None;
    };
    for directory in &index.directories {
        for filename in filenames {
            let candidate = theme_dir.join(directory).join(filename);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    for inherited in &index.inherits {
        if let Some(path) = find_icon_in_theme(icons_root, inherited, filenames, visited) {
            return Some(path);
        }
    }
    None
}

fn decode_image(path: &Path, size: u32) -> Option<IconBitmap> {
    let image = if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
    {
        let data = fs::read(path).ok()?;
        let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default()).ok()?;
        let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)?;
        let icon_size = tree.size();
        let scale = (size as f32 / icon_size.width()).min(size as f32 / icon_size.height());
        let x = (size as f32 - icon_size.width() * scale) / 2.0;
        let y = (size as f32 - icon_size.height() * scale) / 2.0;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale).post_translate(x, y),
            &mut pixmap.as_mut(),
        );
        let mut pixels = pixmap.take();
        for pixel in pixels.as_chunks_mut::<4>().0 {
            let alpha = pixel[3] as u16;
            for channel in &mut pixel[..3] {
                *channel = ((*channel as u16 * 255 + alpha / 2)
                    .checked_div(alpha)
                    .unwrap_or_default())
                .min(255) as u8;
            }
        }
        image::RgbaImage::from_raw(size, size, pixels)?
    } else {
        image::ImageReader::open(path)
            .ok()?
            .decode()
            .ok()?
            .thumbnail(size, size)
            .to_rgba8()
    };
    Some(IconBitmap {
        width: image.width(),
        height: image.height(),
        pixels: image.into_raw(),
    })
}

fn round_rect_path(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    corner_radius: f32,
) -> Option<tiny_skia::Path> {
    let rect = Rect::from_xywh(x, y, width, height)?;
    let mut builder = PathBuilder::new();
    let radius = corner_radius.max(0.0).min(width / 2.0).min(height / 2.0);
    if radius == 0.0 {
        builder.push_rect(rect);
    } else {
        let k = radius * 0.552_284_8;
        builder.move_to(x + radius, y);
        builder.line_to(x + width - radius, y);
        builder.cubic_to(
            x + width - radius + k,
            y,
            x + width,
            y + radius - k,
            x + width,
            y + radius,
        );
        builder.line_to(x + width, y + height - radius);
        builder.cubic_to(
            x + width,
            y + height - radius + k,
            x + width - radius + k,
            y + height,
            x + width - radius,
            y + height,
        );
        builder.line_to(x + radius, y + height);
        builder.cubic_to(
            x + radius - k,
            y + height,
            x,
            y + height - radius + k,
            x,
            y + height - radius,
        );
        builder.line_to(x, y + radius);
        builder.cubic_to(x, y + radius - k, x + radius - k, y, x + radius, y);
        builder.close();
    }
    builder.finish()
}

fn fill_round_rect(
    pixmap: &mut PixmapMut<'_>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    corner_radius: f32,
    rgba: [u8; 4],
) {
    let Some(path) = round_rect_path(x, y, width, height, corner_radius) else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(rgba[0], rgba[1], rgba[2], rgba[3]);
    pixmap.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
}

fn stroke_round_rect(
    pixmap: &mut PixmapMut<'_>,
    bounds: (f32, f32, f32, f32),
    corner_radius: f32,
    rgba: [u8; 4],
    stroke_width: f32,
) {
    let (x, y, width, height) = bounds;
    let Some(path) = round_rect_path(x, y, width, height, corner_radius) else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(rgba[0], rgba[1], rgba[2], rgba[3]);
    let stroke = tiny_skia::Stroke {
        width: stroke_width,
        ..Default::default()
    };
    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
}

fn blend(pixmap: &mut PixmapMut<'_>, x: i32, y: i32, color: [u8; 4], coverage: f32) {
    if coverage <= 0.001 || color[3] == 0 {
        return;
    }
    if x < 0 || y < 0 || x >= pixmap.width() as i32 || y >= pixmap.height() as i32 {
        return;
    }
    let alpha = (coverage * color[3] as f32).round() as u32;
    if alpha == 0 {
        return;
    }
    let index = y as usize * pixmap.width() as usize + x as usize;
    let pixel = &mut pixmap.pixels_mut()[index];
    if alpha >= 255 {
        *pixel =
            tiny_skia::PremultipliedColorU8::from_rgba(color[0], color[1], color[2], 255).unwrap();
        return;
    }
    let inv_alpha = 255 - alpha;
    let r = ((color[0] as u32 * alpha + pixel.red() as u32 * inv_alpha + 127) / 255) as u8;
    let g = ((color[1] as u32 * alpha + pixel.green() as u32 * inv_alpha + 127) / 255) as u8;
    let b = ((color[2] as u32 * alpha + pixel.blue() as u32 * inv_alpha + 127) / 255) as u8;
    *pixel = tiny_skia::PremultipliedColorU8::from_rgba(r, g, b, 255).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dmenu_filters_and_returns_selection() {
        let model = UiModel::dmenu(
            Config::default(),
            vec!["Apple".into(), "Banana".into(), "Orange".into()],
            DmenuModelOptions {
                prompt: String::new(),
                placeholder: "Pick".into(),
                initial_query: "ban".into(),
                print_query: false,
                fuzzy: true,
                case_sensitive: false,
                password: false,
            },
        );
        assert_eq!(model.results.len(), 1);
        assert_eq!(model.selected_action(), Action::CopyText("Banana".into()));
    }

    #[test]
    fn dmenu_can_return_unmatched_raw_input() {
        let model = UiModel::dmenu(
            Config::default(),
            vec!["Apple".into()],
            DmenuModelOptions {
                prompt: String::new(),
                placeholder: "Pick".into(),
                initial_query: "missing".into(),
                print_query: true,
                fuzzy: false,
                case_sensitive: false,
                password: false,
            },
        );
        assert_eq!(model.selected_action(), Action::CopyText("missing".into()));
    }

    #[test]
    fn dmenu_prompt_stays_left_of_input_and_placeholder_disappears_on_typing() {
        let width = 240;
        let height = 240;
        let prompt = "Pilih daftar crème brûlée yang panjang";
        let placeholder = "Ketik nama pilihan yang ingin Anda cari…";
        for theme in ["dark", "light"] {
            let mut config = Config::default();
            config.general.theme = theme.into();
            let mut renderer = Renderer::new(&config).unwrap();
            let label = renderer.fit_text(prompt, renderer.font_size, width as f32 / 3.0);
            assert!(label.ends_with('…'));
            let label_width = renderer.text_width(&label, renderer.font_size);
            assert!(label_width <= width as f32 / 3.0);
            let field_left = 32 + label_width.ceil() as usize;
            let mut model = UiModel::dmenu(
                config,
                Vec::new(),
                DmenuModelOptions {
                    prompt: prompt.into(),
                    placeholder: placeholder.into(),
                    initial_query: String::new(),
                    print_query: false,
                    fuzzy: true,
                    case_sensitive: false,
                    password: false,
                },
            );
            let mut canvas = vec![0; (width * height * 4) as usize];
            renderer.draw(&mut canvas, width, height, &model);

            assert_eq!(model.displayed_input(), placeholder);
            let label_pixels = |canvas: &[u8]| {
                canvas[..(width * 54 * 4) as usize]
                    .chunks_exact(width as usize * 4)
                    .flat_map(|row| row[..field_left * 4].iter().copied())
                    .collect::<Vec<_>>()
            };
            let before = label_pixels(&canvas);
            assert!(before
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| *pixel != renderer.theme.background));
            let caret = (35 * width as usize + field_left + 20) * 4;
            assert_eq!(&canvas[caret..caret + 4], &renderer.theme.primary);

            model.insert("Apple");
            renderer.draw(&mut canvas, width, height, &model);
            assert_eq!(model.displayed_input(), "Apple");
            assert_eq!(label_pixels(&canvas), before);
        }
    }

    #[test]
    fn renderer_draws_color_rounded_swatch() {
        let mut model = UiModel::dmenu(
            Config::default(),
            vec!["#3498db".into()],
            DmenuModelOptions {
                prompt: String::new(),
                placeholder: "Color".into(),
                initial_query: String::new(),
                print_query: false,
                fuzzy: true,
                case_sensitive: false,
                password: false,
            },
        );
        model.results[0].icon = Some("color:#3498dbff".into());
        let mut renderer = Renderer::new(&Config::default()).unwrap();
        let mut canvas = vec![0u8; 640 * 240 * 4];
        renderer.draw(&mut canvas, 640, 240, &model);
        let offset = ((98 * 640 + 44) * 4) as usize;
        assert_eq!(canvas[offset], 52);
        assert_eq!(canvas[offset + 1], 152);
        assert_eq!(canvas[offset + 2], 219);
    }

    #[test]
    fn renderer_centers_image_thumbnail_vertically() {
        let path =
            std::env::temp_dir().join(format!("waymenu-landscape-{}.png", std::process::id()));
        let mut pixels = vec![200u8; 80 * 40 * 4];
        for chunk in pixels.as_chunks_mut::<4>().0 {
            chunk[3] = 255;
        }
        image::save_buffer_with_format(
            &path,
            &pixels,
            80,
            40,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .unwrap();

        let mut model = UiModel::dmenu(
            Config::default(),
            vec!["Landscape".into()],
            DmenuModelOptions {
                prompt: String::new(),
                placeholder: "Image".into(),
                initial_query: String::new(),
                print_query: false,
                fuzzy: true,
                case_sensitive: false,
                password: false,
            },
        );
        model.results[0].icon = Some(path.display().to_string());
        let mut renderer = Renderer::new(&Config::default()).unwrap();
        let mut canvas = vec![0u8; 640 * 240 * 4];
        renderer.draw(&mut canvas, 640, 240, &model);

        let center_offset = ((98 * 640 + 44) * 4) as usize;
        assert_eq!(canvas[center_offset], 200);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn binary_autocomplete_preserves_provider_prefix() {
        let mut model = UiModel::new(Config::default());
        model.input = "$ car".into();
        model.cursor = model.input.len();
        model.set_binaries(vec![Binary {
            name: "cargo".into(),
            path: "/usr/bin/cargo".into(),
        }]);
        model.autocomplete();
        assert_eq!(model.input, "$ cargo");
        assert_eq!(
            model.selected_action(),
            Action::LaunchBinary("/usr/bin/cargo".into())
        );
    }

    #[test]
    fn plugin_autocomplete_can_supply_query_separate_from_title() {
        let mut model = UiModel::new(Config::default());
        model.insert("!partial");
        model.results[0].title = "Abigail".into();
        model.results[0].metadata = Some("autocomplete:sv Abigail".into());

        model.autocomplete();

        assert_eq!(model.input, "!sv Abigail");
        assert_eq!(model.cursor, model.input.len());
    }

    #[test]
    fn renderer_decodes_and_caches_lazy_image_preview() {
        let path = std::env::temp_dir().join(format!(
            "waymenu-preview-{}-{}.png",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let pixels = vec![255u8; 80 * 40 * 4];
        image::save_buffer_with_format(
            &path,
            &pixels,
            80,
            40,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .unwrap();
        let mut model = UiModel::dmenu(
            Config::default(),
            vec!["Preview".into()],
            DmenuModelOptions {
                prompt: String::new(),
                placeholder: "Image".into(),
                initial_query: String::new(),
                print_query: false,
                fuzzy: true,
                case_sensitive: false,
                password: false,
            },
        );
        model.results[0].image = Some(path.display().to_string());
        let mut renderer = Renderer::new(&Config::default()).unwrap();
        let mut canvas = vec![0u8; 640 * 240 * 4];
        renderer.draw(&mut canvas, 640, 240, &model);
        assert!(renderer
            .icons
            .keys()
            .any(|key| key.contains("waymenu-preview")));
        assert!(
            decode_image(&path, 36).is_some_and(|image| image.width <= 36 && image.height <= 36)
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn svg_icons_are_rasterized_at_thumbnail_size() {
        let path = std::env::temp_dir().join(format!("waymenu-icon-{}.svg", std::process::id()));
        fs::write(
            &path,
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="128" height="64"><rect width="128" height="64" fill="#e04020" fill-opacity="0.5"/></svg>"##,
        )
        .unwrap();
        let image = decode_image(&path, 36).unwrap();
        assert_eq!((image.width, image.height), (36, 36));
        let center = ((18 * image.width + 18) * 4) as usize;
        assert!((image.pixels[center] as i16 - 224).abs() < 4);
        assert!((image.pixels[center + 1] as i16 - 64).abs() < 4);
        assert!((image.pixels[center + 2] as i16 - 32).abs() < 4);
        assert!((image.pixels[center + 3] as i16 - 128).abs() < 4);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn icon_theme_lookup_uses_declared_sizes_and_inheritance() {
        let root = std::env::temp_dir().join(format!("waymenu-theme-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let icons = root.join("icons");
        let child = icons.join("Child");
        let parent = icons.join("Parent");
        fs::create_dir_all(child.join("32x32/apps")).unwrap();
        fs::create_dir_all(parent.join("scalable/apps")).unwrap();
        fs::write(
            child.join("index.theme"),
            "[Icon Theme]\nDirectories=32x32/apps\nInherits=Parent\n\n[32x32/apps]\nSize=32\nContext=Applications\n",
        )
        .unwrap();
        fs::write(
            parent.join("index.theme"),
            "[Icon Theme]\nDirectories=scalable/apps\n\n[scalable/apps]\nSize=48\nType=Scalable\nContext=Applications\n",
        )
        .unwrap();
        let icon = parent.join("scalable/apps/org.example.app.svg");
        fs::write(&icon, "<svg/>").unwrap();
        let found = find_icon_in_theme(
            &icons,
            "Child",
            &icon_filenames("org.example.app"),
            &mut std::collections::HashSet::new(),
        );
        assert_eq!(found.as_deref(), Some(icon.as_path()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unindexed_user_icon_directories_are_searched() {
        let root = std::env::temp_dir().join(format!("waymenu-user-icons-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let icons = root.join("icons");
        let icon = icons.join("hicolor/256x256/apps/org.custom.launcher.png");
        fs::create_dir_all(icon.parent().unwrap()).unwrap();
        fs::write(&icon, b"icon").unwrap();
        let found = find_icon_in_theme(
            &icons,
            "hicolor",
            &icon_filenames("org.custom.launcher"),
            &mut std::collections::HashSet::new(),
        );
        assert_eq!(found.as_deref(), Some(icon.as_path()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn gtk_icon_theme_setting_is_read_without_quotes_or_surrounding_space() {
        assert_eq!(
            parse_gtk_icon_theme("[Settings]\ngtk-icon-theme-name = 'Papirus-Dark'\n"),
            Some("Papirus-Dark".into())
        );
        assert_eq!(
            icon_filenames("org.example.desktop-app"),
            [
                "org.example.desktop-app.svg",
                "org.example.desktop-app.png",
                "org.example.desktop-app.webp",
                "org.example.desktop-app.jpg",
                "org.example.desktop-app.jpeg",
            ]
        );
    }

    #[test]
    fn application_results_scroll_past_visible_limit() {
        let mut config = Config::default();
        config.general.max_results = 2;
        config.general.remember_history = false;
        let mut model = UiModel::new(config);
        model.applications = (0..8)
            .map(|index| DesktopApplication {
                id: format!("{index}.desktop"),
                name: format!("App {index}"),
                description: String::new(),
                exec: "true".into(),
                icon: None,
                terminal: false,
                working_directory: None,
                desktop_file: PathBuf::new(),
            })
            .collect();
        model.application_results.clear();
        model.refresh();
        assert_eq!(model.results.len(), 8);
        for _ in 0..7 {
            model.next();
        }
        assert_eq!(model.selected, 7);
        assert_eq!(model.results[model.selected].title, "App 7");
    }

    #[test]
    fn f1_help_shows_configured_bindings_and_loaded_plugin_metadata() {
        let mut model = UiModel::new(Config::default());
        model.insert("fire");
        model.toggle_help();
        assert!(model.is_help_mode());
        assert_eq!(model.input, "fire");
        assert!(model
            .results
            .iter()
            .any(|row| row.title.contains("F1 · Toggle this help")));
        assert!(model
            .results
            .iter()
            .any(|row| row.title.contains("! · Shell")));
        assert!(model
            .results
            .iter()
            .any(|row| row.title.contains("$ · Binary search")));
        model.toggle_help();
        assert!(!model.is_help_mode());
        assert_eq!(model.input, "fire");
    }

    #[test]
    fn result_window_scrolls_only_at_edges_without_scrolloff() {
        let mut first = 0;
        for selected in 1..=7 {
            first = result_window_start(first, selected, 30, 8);
        }
        // At selected = 7 (the 8th item, row index 7), first is still 0 (no scrolloff).
        assert_eq!(first, 0);
        // At selected = 8, it scrolls down by 1 so item 8 is the bottom row.
        first = result_window_start(first, 8, 30, 8);
        assert_eq!(first, 1);
        // Reversing direction keeps the current viewport until reaching the top edge.
        first = result_window_start(first, 6, 30, 8);
        assert_eq!(first, 1);
        first = result_window_start(first, 1, 30, 8);
        assert_eq!(first, 1);
        first = result_window_start(first, 0, 30, 8);
        assert_eq!(first, 0);
        // At the end of the dataset, clamp the viewport so the final rows stay filled.
        assert_eq!(result_window_start(22, 29, 30, 8), 22);
        assert_eq!(result_window_start(0, 0, 4, 8), 0);
        assert_eq!(result_window_start(0, 4, 10, 1), 4);
        assert_eq!(result_window_start(22, 0, 30, 8), 0);
        assert_eq!(result_window_start(0, 29, 30, 8), 22);
    }

    #[test]
    fn dmenu_lines_limits_viewport_not_matching_dataset() {
        let mut config = Config::default();
        config.general.max_results = 2;
        let mut model = UiModel::dmenu(
            config,
            (0..8).map(|index| format!("entry-{index}")).collect(),
            DmenuModelOptions {
                prompt: String::new(),
                placeholder: "Pick".into(),
                initial_query: String::new(),
                print_query: false,
                fuzzy: false,
                case_sensitive: false,
                password: false,
            },
        );
        assert_eq!(model.results.len(), 8);
        for _ in 0..6 {
            model.next();
        }
        assert_eq!(model.selected_action(), Action::CopyText("entry-6".into()));
    }

    #[test]
    fn password_dmenu_never_returns_selected_or_raw_input() {
        let model = UiModel::dmenu(
            Config::default(),
            vec!["secret-entry".into()],
            DmenuModelOptions {
                prompt: String::new(),
                placeholder: "Password".into(),
                initial_query: "sensitive".into(),
                print_query: true,
                fuzzy: true,
                case_sensitive: false,
                password: true,
            },
        );
        assert_eq!(model.selected_action(), Action::None);
        assert_eq!(model.displayed_input(), "•••••••••");
    }

    #[test]
    fn mouse_click_item_at_coordinates_identifies_clicked_result() {
        let config = Config::default();
        let mut renderer = Renderer::new(&config).unwrap();
        let width = 640;
        let height = 400; // visible = (400 - 110) / 58 = 5 rows
        let total = 10;

        // Above item area (e.g. search bar)
        assert_eq!(renderer.item_at(50.0, 50.0, width, height, total), None);

        // Outside width bounds
        assert_eq!(renderer.item_at(-5.0, 80.0, width, height, total), None);
        assert_eq!(renderer.item_at(700.0, 80.0, width, height, total), None);

        // First visible row (row 0: 72..130)
        assert_eq!(renderer.item_at(50.0, 75.0, width, height, total), Some(0));
        assert_eq!(renderer.item_at(50.0, 120.0, width, height, total), Some(0));

        // Second visible row (row 1: 130..188)
        assert_eq!(renderer.item_at(50.0, 135.0, width, height, total), Some(1));

        // Last visible row (row 4: 72 + 4 * 58 = 304..362)
        assert_eq!(renderer.item_at(50.0, 310.0, width, height, total), Some(4));

        // Below visible rows / footer
        assert_eq!(renderer.item_at(50.0, 370.0, width, height, total), None);

        // When total results is less than visible rows
        assert_eq!(renderer.item_at(50.0, 135.0, width, height, 1), None);

        // When first_visible is scrolled
        renderer.first_visible = 3;
        assert_eq!(renderer.item_at(50.0, 75.0, width, height, total), Some(3));
        assert_eq!(renderer.item_at(50.0, 135.0, width, height, total), Some(4));
    }

    #[test]
    fn ui_model_select_and_scroll_clamp_without_wrapping() {
        let mut model = UiModel::dmenu(
            Config::default(),
            (0..5).map(|i| format!("item-{i}")).collect(),
            DmenuModelOptions {
                prompt: String::new(),
                placeholder: "Pick".into(),
                initial_query: String::new(),
                print_query: false,
                fuzzy: false,
                case_sensitive: false,
                password: false,
            },
        );

        assert_eq!(model.selected, 0);

        // Direct select
        model.select(3);
        assert_eq!(model.selected, 3);
        assert!(model.selection_moved_down);

        // Direct select out of bounds is ignored
        model.select(10);
        assert_eq!(model.selected, 3);

        // Scroll down
        model.scroll_down();
        assert_eq!(model.selected, 4);

        // Scroll down at end clamps, does not wrap
        model.scroll_down();
        assert_eq!(model.selected, 4);
        model.scroll_down_by(10);
        assert_eq!(model.selected, 4);

        // Scroll up
        model.scroll_up();
        assert_eq!(model.selected, 3);
        assert!(!model.selection_moved_down);

        model.scroll_up_by(2);
        assert_eq!(model.selected, 1);

        // Scroll up at beginning clamps, does not wrap
        model.scroll_up_by(5);
        assert_eq!(model.selected, 0);
    }

    #[test]
    fn caret_position_and_blinking_visibility() {
        let config = Config::default();
        let mut renderer = Renderer::new(&config).unwrap();
        let mut model = UiModel::new(config);

        // Initially empty input
        assert_eq!(model.cursor(), 0);
        assert_eq!(model.cursor_prefix(), "");
        assert_eq!(renderer.caret_x(&model), 32.0);

        // Typing characters advances caret
        model.insert("firefox");
        assert_eq!(model.cursor(), 7);
        assert_eq!(model.cursor_prefix(), "firefox");
        let full_width = renderer.caret_x(&model);
        assert!(full_width > 32.0);

        // Moving left moves caret backwards
        model.move_left();
        assert_eq!(model.cursor(), 6);
        assert_eq!(model.cursor_prefix(), "firefo");
        let left_width = renderer.caret_x(&model);
        assert!(left_width < full_width);

        // Moving to beginning
        model.move_to_beginning();
        assert_eq!(model.cursor(), 0);
        assert_eq!(renderer.caret_x(&model), 32.0);

        // Moving right moves caret forward
        model.move_right();
        assert_eq!(model.cursor(), 1);
        assert!(renderer.caret_x(&model) > 32.0);

        // Cursor visibility toggle
        assert!(renderer.cursor_visible());
        renderer.set_cursor_visible(false);
        assert!(!renderer.cursor_visible());
        renderer.set_cursor_visible(true);
        assert!(renderer.cursor_visible());
    }

    #[test]
    fn tab_column_alignment_and_emoji_rendering() {
        let config = Config::default();
        let mut renderer = Renderer::new(&config).unwrap();

        // 1. Tab expands to tab stop aligning columns
        let w1 = renderer.text_width("20\t", 16.0);
        let w2 = renderer.text_width("9\t", 16.0);
        assert!(w1 > 0.0);
        assert_eq!(
            w1, w2,
            "Both 20\\t and 9\\t must align to the exact same tab stop column"
        );

        // 2. Both columns followed by emoji or text align identically
        let w_emoji1 = renderer.text_width("20\t😀", 16.0);
        let w_emoji2 = renderer.text_width("9\t😀", 16.0);
        assert_eq!(w_emoji1, w_emoji2);
        assert!(w_emoji1 > w1);

        // 3. Render into canvas without panic
        let model = UiModel::dmenu(
            config,
            vec!["20\t😀".into(), "9\tHubungkan".into()],
            DmenuModelOptions {
                prompt: String::new(),
                placeholder: String::new(),
                initial_query: String::new(),
                print_query: false,
                fuzzy: true,
                case_sensitive: false,
                password: false,
            },
        );
        let mut canvas = vec![0u8; 640 * 400 * 4];
        renderer.draw(&mut canvas, 640, 400, &model);
        // Canvas has non-zero pixel data
        assert!(canvas.iter().any(|&b| b != 0));
    }

    #[test]
    fn text_editor_word_navigation_and_deletions() {
        let mut model = UiModel::new(Config::default());

        // 1. Word navigation with spaces and punctuation
        model.insert("cargo run --release");
        assert_eq!(model.input, "cargo run --release");
        assert_eq!(model.cursor(), 19);

        // Word left navigation
        model.move_word_left();
        assert_eq!(model.cursor(), 12); // before "release"
        assert_eq!(&model.input[..model.cursor()], "cargo run --");

        model.move_word_left();
        assert_eq!(model.cursor(), 10); // before "--"
        assert_eq!(&model.input[..model.cursor()], "cargo run ");

        model.move_word_left();
        assert_eq!(model.cursor(), 6); // before "run"
        assert_eq!(&model.input[..model.cursor()], "cargo ");

        model.move_word_left();
        assert_eq!(model.cursor(), 0); // before "cargo"
        assert_eq!(&model.input[..model.cursor()], "");

        // Moving left when at start stays at 0
        model.move_word_left();
        assert_eq!(model.cursor(), 0);

        // Word right navigation
        model.move_word_right();
        assert_eq!(model.cursor(), 5); // after "cargo"

        model.move_word_right();
        assert_eq!(model.cursor(), 9); // after "run"

        model.move_word_right();
        assert_eq!(model.cursor(), 12); // after "--"

        model.move_word_right();
        assert_eq!(model.cursor(), 19); // after "release"

        // Moving right when at end stays at end
        model.move_word_right();
        assert_eq!(model.cursor(), 19);

        // 2. Delete word backward (Ctrl+Backspace / Ctrl+W)
        model.delete_word_backward();
        assert_eq!(model.input, "cargo run --");
        assert_eq!(model.cursor(), 12);

        model.delete_word_backward();
        assert_eq!(model.input, "cargo run ");
        assert_eq!(model.cursor(), 10);

        model.delete_word();
        assert_eq!(model.input, "cargo ");
        assert_eq!(model.cursor(), 6);

        model.delete_word_backward();
        assert_eq!(model.input, "");
        assert_eq!(model.cursor(), 0);

        model.delete_word_backward();
        assert_eq!(model.input, "");
        assert_eq!(model.cursor(), 0);

        // 3. Delete forward (Del / Delete) and delete word forward (Ctrl+Delete)
        model.insert("apple banana cherry");
        model.move_to_beginning();
        assert_eq!(model.cursor(), 0);

        // Delete single character forward
        model.delete_forward();
        assert_eq!(model.input, "pple banana cherry");
        assert_eq!(model.cursor(), 0);

        // Delete word forward
        model.delete_word_forward();
        assert_eq!(model.input, " banana cherry");
        assert_eq!(model.cursor(), 0);

        // Delete word forward including leading space
        model.delete_word_forward();
        assert_eq!(model.input, " cherry");
        assert_eq!(model.cursor(), 0);

        model.delete_word_forward();
        assert_eq!(model.input, "");
        assert_eq!(model.cursor(), 0);

        // 4. Unicode / multibyte support
        model.insert("halo dunia 🌍");
        assert_eq!(model.cursor(), 15);
        model.move_word_left();
        assert_eq!(model.cursor(), 11); // before emoji (4 bytes)
        model.delete_forward();
        assert_eq!(model.input, "halo dunia ");
        assert_eq!(model.cursor(), 11);
        model.delete_word_backward();
        assert_eq!(model.input, "halo ");
        assert_eq!(model.cursor(), 5);
        model.delete_word_backward();
        assert_eq!(model.input, "");
        assert_eq!(model.cursor(), 0);
    }

    #[test]
    fn custom_theme_colors_override_and_render() {
        let toml_str = r##"
            [general]
            theme = "custom"

            [colors]
            background = "#282a36"
            input = "#44475a"
            selected = "#6272a4"
            primary = "#f8f8f2"
            secondary = "#bfbfbf"
            highlight = "#ff79c6"
            border = "#bd93f9"
        "##;
        let config: Config = toml::from_str(toml_str).unwrap();
        let mut renderer = Renderer::new(&config).unwrap();

        // 1. Theme colors match custom hex values
        assert_eq!(renderer.theme().background, [40, 42, 54, 255]);
        assert_eq!(renderer.theme().input, [68, 71, 90, 255]);
        assert_eq!(renderer.theme().selected, [98, 114, 164, 255]);
        assert_eq!(renderer.theme().primary, [248, 248, 242, 255]);
        assert_eq!(renderer.theme().secondary, [191, 191, 191, 255]);
        assert_eq!(renderer.theme().highlight, [255, 121, 198, 255]);
        assert_eq!(renderer.theme().border, Some([189, 147, 249, 255]));

        // 2. Draws with custom colors and border into canvas without panic
        let model = UiModel::new(config);
        let mut canvas = vec![0u8; 640 * 400 * 4];
        renderer.draw(&mut canvas, 640, 400, &model);
        assert!(canvas.iter().any(|&b| b != 0));
    }
}
