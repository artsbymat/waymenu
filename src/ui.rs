use std::{
    collections::{HashMap, VecDeque},
    env, fs,
    path::{Path, PathBuf},
};

use ab_glyph::{point, Font, FontArc, PxScale, ScaleFont};
use tiny_skia::{Color, Paint, PathBuilder, PixmapMut, Rect, Transform};

use crate::{
    applications::DesktopApplication, binary::Binary, config::Config, history::History,
    plugins::PluginManager, search, Action, SearchResult,
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
    binaries: Option<Vec<Binary>>,
    history: History,
    plugins: Option<PluginManager>,
    dmenu: Option<DmenuState>,
}

struct DmenuState {
    items: Vec<SearchResult>,
    print_query: bool,
    fuzzy: bool,
    case_sensitive: bool,
}

pub struct DmenuModelOptions {
    pub prompt: String,
    pub initial_query: String,
    pub print_query: bool,
    pub fuzzy: bool,
    pub case_sensitive: bool,
    pub password: bool,
}

impl UiModel {
    pub fn new(config: Config) -> Self {
        let history = History::load_with_limit(config.history.max_entries);
        let plugins = PluginManager::from_config(&config);
        let applications = crate::applications::discover();
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
            binaries: None,
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
            initial_query,
            print_query,
            fuzzy,
            case_sensitive,
            password,
        } = options;
        let cursor = initial_query.len();
        let entries = items
            .into_iter()
            .enumerate()
            .map(|(index, title)| SearchResult {
                id: index.to_string(),
                title,
                description: String::new(),
                score: 0,
                icon: None,
                image: None,
                metadata: None,
            })
            .collect();
        let mut model = Self {
            input: initial_query,
            results: Vec::new(),
            selected: 0,
            exit: false,
            prompt,
            selection_moved_down: false,
            help_mode: false,
            password,
            cursor,
            config,
            applications: Vec::new(),
            binaries: None,
            history,
            plugins: None,
            dmenu: Some(DmenuState {
                items: entries,
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
    pub fn autocomplete(&mut self) {
        if self.help_mode {
            return;
        }
        if let Some(item) = self.results.get(self.selected) {
            self.input = if self.dmenu.is_some() {
                item.title.clone()
            } else if self.input.starts_with(&self.config.binary.prefix) {
                format!("{}{}", self.config.binary.prefix, item.title)
            } else if let Some((plugin, _)) = self
                .plugins
                .as_ref()
                .and_then(|plugins| plugins.select(&self.input))
            {
                format!("{}{}", plugin.prefix(), item.title)
            } else {
                item.title.clone()
            };
            self.cursor = self.input.len();
            self.refresh();
        }
    }

    pub fn delete_word(&mut self) {
        while self.cursor > 0 && self.input[..self.cursor].ends_with(char::is_whitespace) {
            self.backspace();
        }
        while self.cursor > 0 && !self.input[..self.cursor].ends_with(char::is_whitespace) {
            self.backspace();
        }
    }
    pub fn move_to_beginning(&mut self) {
        self.cursor = 0;
    }
    pub fn move_to_end(&mut self) {
        self.cursor = self.input.len();
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

    fn displayed_input(&self) -> std::borrow::Cow<'_, str> {
        if self.help_mode {
            std::borrow::Cow::Borrowed("Help · Keybindings and plugins")
        } else if self.input.is_empty() {
            std::borrow::Cow::Borrowed(&self.prompt)
        } else if self.password {
            std::borrow::Cow::Owned("•".repeat(self.input.chars().count()))
        } else {
            std::borrow::Cow::Borrowed(&self.input)
        }
    }

    fn highlight_query(&self) -> &str {
        if self.help_mode {
            return "";
        }
        if self.dmenu.is_some() {
            return &self.input;
        }
        self.input
            .strip_prefix(&self.config.binary.prefix)
            .or_else(|| self.input.strip_prefix(&self.config.plugins.shell_prefix))
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

    pub fn record_launch(&mut self, id: &str) {
        if self.config.history.enabled {
            let _ = self.history.record(id);
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

    pub fn set_binaries(&mut self, binaries: Vec<Binary>) {
        self.binaries = Some(binaries);
        self.refresh();
    }

    fn refresh(&mut self) {
        self.selected = 0;
        self.selection_moved_down = false;
        if self.help_mode {
            self.results = self.help_results();
            return;
        }
        if let Some(dmenu) = &self.dmenu {
            if self.input.is_empty() {
                self.results = dmenu.items.clone();
            } else if dmenu.fuzzy && !dmenu.case_sensitive {
                self.results = search::filter(&dmenu.items, &self.input, dmenu.items.len());
            } else {
                let query = if dmenu.case_sensitive {
                    self.input.clone()
                } else {
                    self.input.to_lowercase()
                };
                self.results = dmenu
                    .items
                    .iter()
                    .filter(|item| {
                        let title = if dmenu.case_sensitive {
                            item.title.clone()
                        } else {
                            item.title.to_lowercase()
                        };
                        if dmenu.fuzzy {
                            ordered_match(&title, &query)
                        } else {
                            title.contains(&query)
                        }
                    })
                    .cloned()
                    .collect();
            }
            return;
        }
        if let Some((plugin, query)) = self
            .plugins
            .as_ref()
            .and_then(|plugins| plugins.select(&self.input))
        {
            self.results = plugin.search(query);
            return;
        }
        if let Some(query) = self.input.strip_prefix(&self.config.binary.prefix) {
            let Some(binaries) = &self.binaries else {
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
            let candidates: Vec<_> = binaries.iter().map(Binary::as_result).collect();
            self.results = search::filter(&candidates, query, usize::MAX);
            return;
        }
        let candidates: Vec<_> = self
            .applications
            .iter()
            .map(|app| {
                app.as_result(
                    if self.config.general.remember_history && self.config.history.enabled {
                        self.history.score(&app.id)
                    } else {
                        0
                    },
                )
            })
            .collect();
        self.results = search::filter(&candidates, &self.input, usize::MAX);
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
                &format!("{} · Binary search", self.config.binary.prefix),
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
                &format!("{} · {}", plugin.prefix, plugin.name),
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

pub struct Renderer {
    font: Option<FontArc>,
    icons: HashMap<String, Option<IconBitmap>>,
    icon_order: VecDeque<String>,
    theme: Theme,
    show_icons: bool,
    show_description: bool,
    font_size: f32,
    border_radius: f32,
    first_visible: usize,
}

struct IconBitmap {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

struct Theme {
    background: [u8; 4],
    input: [u8; 4],
    selected: [u8; 4],
    primary: [u8; 4],
    secondary: [u8; 4],
}

impl Renderer {
    pub fn new(config: &Config) -> std::io::Result<Self> {
        let theme = if config.general.theme.eq_ignore_ascii_case("light") {
            Theme {
                background: [242, 243, 247, 255],
                input: [255, 255, 255, 255],
                selected: [216, 224, 242, 255],
                primary: [27, 30, 38, 255],
                secondary: [91, 97, 112, 255],
            }
        } else {
            Theme {
                background: [25, 27, 34, 255],
                input: [43, 46, 57, 255],
                selected: [63, 69, 87, 255],
                primary: [245, 245, 248, 255],
                secondary: [166, 171, 185, 255],
            }
        };
        let font = load_font(&config.appearance.font_family).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no usable TrueType/OpenType font was found",
            )
        })?;
        Ok(Self {
            font: Some(font),
            icons: HashMap::new(),
            icon_order: VecDeque::new(),
            theme,
            show_icons: config.appearance.show_icons,
            show_description: config.appearance.show_description,
            font_size: config.appearance.font_size,
            border_radius: config.appearance.border_radius,
            first_visible: 0,
        })
    }
    pub fn draw(&mut self, canvas: &mut [u8], width: u32, height: u32, model: &UiModel) {
        let Some(mut pixmap) = PixmapMut::from_bytes(canvas, width, height) else {
            return;
        };
        pixmap.fill(Color::from_rgba8(
            self.theme.background[0],
            self.theme.background[1],
            self.theme.background[2],
            255,
        ));
        fill_round_rect(
            &mut pixmap,
            12.0,
            12.0,
            width as f32 - 24.0,
            54.0,
            self.border_radius.min(27.0),
            self.theme.input,
        );
        self.text(
            &mut pixmap,
            &model.displayed_input(),
            32.0,
            45.0,
            self.font_size + 5.0,
            self.theme.primary,
        );
        let visible = ((height.saturating_sub(110)) / 58).max(1) as usize;
        self.first_visible = result_window_start(
            self.first_visible,
            model.selected,
            model.results.len(),
            visible,
            model.selection_moved_down,
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
            let visual = result.image.as_ref().or(result.icon.as_ref());
            let text_x = if self.show_icons && visual.is_some() {
                74.0
            } else {
                28.0
            };
            if let Some(visual) = visual.filter(|_| self.show_icons) {
                self.draw_image(&mut pixmap, visual, 26, (y - 1.0) as i32, 36);
            }
            self.text_highlight(
                &mut pixmap,
                &result.title,
                (text_x, y + 15.0),
                (self.font_size + 2.0, self.theme.primary),
                model.highlight_query(),
            );
            if self.show_description {
                self.text(
                    &mut pixmap,
                    &result.description,
                    text_x,
                    y + 35.0,
                    self.font_size.max(10.0) - 2.0,
                    self.theme.secondary,
                );
            }
            if index == model.selected {
                if let Some(preview) = result.image.as_ref() {
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
    }

    fn draw_image(&mut self, pixmap: &mut PixmapMut<'_>, name: &str, x: i32, y: i32, size: u32) {
        let cache_key = format!("{size}:{name}");
        if !self.icons.contains_key(&cache_key) {
            let bitmap = resolve_icon(name).and_then(|path| decode_image(&path, size));
            if self.icons.len() >= 32 {
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
        for row in 0..icon.height {
            for column in 0..icon.width {
                let offset = ((row * icon.width + column) * 4) as usize;
                blend(
                    pixmap,
                    x + column as i32,
                    y + row as i32,
                    [
                        icon.pixels[offset],
                        icon.pixels[offset + 1],
                        icon.pixels[offset + 2],
                        icon.pixels[offset + 3],
                    ],
                    1.0,
                );
            }
        }
    }

    fn text(
        &self,
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
        &self,
        pixmap: &mut PixmapMut<'_>,
        text: &str,
        position: (f32, f32),
        style: (f32, [u8; 4]),
        query: &str,
    ) {
        let (mut x, baseline) = position;
        let (size, color) = style;
        let Some(font) = &self.font else { return };
        let scaled = font.as_scaled(PxScale::from(size));
        let mut previous = None;
        let query = query.to_lowercase();
        let mut needles = query.chars().peekable();
        for ch in text.chars().take(100) {
            let id = scaled.glyph_id(ch);
            if let Some(last) = previous {
                x += scaled.kern(last, id);
            }
            let glyph = id.with_scale_and_position(size, point(x, baseline));
            if let Some(outline) = font.outline_glyph(glyph) {
                let bounds = outline.px_bounds();
                let matched = needles
                    .peek()
                    .is_some_and(|needle| ch.to_lowercase().next() == Some(*needle));
                if matched {
                    needles.next();
                }
                let glyph_color = if matched { [105, 170, 255, 255] } else { color };
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
        }
    }
}

fn ordered_match(candidate: &str, query: &str) -> bool {
    let mut candidate = candidate.chars();
    query
        .chars()
        .all(|needle| candidate.any(|value| value == needle))
}

fn result_window_start(
    current_first: usize,
    selected: usize,
    total: usize,
    visible: usize,
    moved_down: bool,
) -> usize {
    let visible = visible.max(1);
    let max_first = total.saturating_sub(visible);
    let mut first = current_first.min(max_first);
    if moved_down {
        if selected < first {
            first = 0;
        } else {
            let selected_row = visible.saturating_sub(2);
            if selected > first.saturating_add(selected_row) {
                first = selected.saturating_sub(selected_row).min(max_first);
            }
        }
    } else if selected < first {
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
    let mono = family.to_ascii_lowercase().contains("mono");
    let candidates: &[&str] = if mono {
        &[
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
        ]
    } else {
        &[
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
            "/usr/share/fonts/inter/Inter.ttc",
            "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/TTF/JetBrainsMonoNerdFont-Regular.ttf",
        ]
    };
    candidates
        .iter()
        .find_map(|path| read_font(Path::new(path)))
        .or_else(find_any_system_font)
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
    let Ok(index) = fs::read_to_string(theme_dir.join("index.theme")) else {
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
    for directory in directories {
        for filename in filenames {
            let candidate = theme_dir.join(&directory).join(filename);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    for inherited in inherits {
        if let Some(path) = find_icon_in_theme(icons_root, &inherited, filenames, visited) {
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

fn fill_round_rect(
    pixmap: &mut PixmapMut<'_>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    corner_radius: f32,
    rgba: [u8; 4],
) {
    let Some(rect) = Rect::from_xywh(x, y, width, height) else {
        return;
    };
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
    let Some(path) = builder.finish() else { return };
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

fn blend(pixmap: &mut PixmapMut<'_>, x: i32, y: i32, color: [u8; 4], coverage: f32) {
    if x < 0 || y < 0 || x >= pixmap.width() as i32 || y >= pixmap.height() as i32 {
        return;
    }
    let index = y as usize * pixmap.width() as usize + x as usize;
    let pixel = &mut pixmap.pixels_mut()[index];
    let alpha = coverage * color[3] as f32 / 255.0;
    let old = pixel.demultiply();
    *pixel = tiny_skia::PremultipliedColorU8::from_rgba(
        (color[0] as f32 * alpha + old.red() as f32 * (1.0 - alpha)) as u8,
        (color[1] as f32 * alpha + old.green() as f32 * (1.0 - alpha)) as u8,
        (color[2] as f32 * alpha + old.blue() as f32 * (1.0 - alpha)) as u8,
        255,
    )
    .unwrap();
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
                prompt: "Pick".into(),
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
                prompt: "Pick".into(),
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
    fn binary_autocomplete_preserves_provider_prefix() {
        let mut model = UiModel::new(Config::default());
        model.input = "$car".into();
        model.cursor = model.input.len();
        model.set_binaries(vec![Binary {
            name: "cargo".into(),
            path: "/usr/bin/cargo".into(),
        }]);
        model.autocomplete();
        assert_eq!(model.input, "$cargo");
        assert_eq!(
            model.selected_action(),
            Action::LaunchBinary("/usr/bin/cargo".into())
        );
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
                prompt: "Image".into(),
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
    fn result_window_keeps_one_row_below_selection_until_near_end() {
        let mut first = 0;
        for selected in 1..=7 {
            first = result_window_start(first, selected, 30, 8, true);
        }
        assert_eq!(first, 1);
        // Reversing direction moves the highlight up within the current viewport.
        first = result_window_start(first, 6, 30, 8, false);
        assert_eq!(first, 1);
        first = result_window_start(first, 5, 30, 8, false);
        assert_eq!(first, 1);
        first = result_window_start(first, 0, 30, 8, false);
        assert_eq!(first, 0);
        // At the end of the dataset, clamp the viewport so the final rows stay filled.
        assert_eq!(result_window_start(22, 29, 30, 8, true), 22);
        assert_eq!(result_window_start(0, 0, 4, 8, true), 0);
        assert_eq!(result_window_start(0, 4, 10, 1, true), 4);
        assert_eq!(result_window_start(22, 0, 30, 8, true), 0);
        assert_eq!(result_window_start(0, 29, 30, 8, false), 22);
    }

    #[test]
    fn dmenu_lines_limits_viewport_not_matching_dataset() {
        let mut config = Config::default();
        config.general.max_results = 2;
        let mut model = UiModel::dmenu(
            config,
            (0..8).map(|index| format!("entry-{index}")).collect(),
            DmenuModelOptions {
                prompt: "Pick".into(),
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
                prompt: "Password".into(),
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
}
