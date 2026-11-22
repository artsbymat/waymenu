use serde::{Deserialize, Serialize};
use std::{env, fs, io, path::PathBuf};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub appearance: Appearance,
    pub colors: Colors,
    pub plugins: Plugins,
    pub binary: Binary,
    pub history: History,
    pub keybindings: Keybindings,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct General {
    pub theme: String,
    pub width: u32,
    pub height: Option<u32>,
    pub max_results: usize,
    pub fuzzy_search: bool,
    pub remember_history: bool,
    pub monitor: Option<String>,
    pub terminal: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Appearance {
    pub font_family: String,
    pub font_size: f32,
    pub border_radius: f32,
    pub show_icons: bool,
    pub show_description: bool,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Colors {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(alias = "selection", skip_serializing_if = "Option::is_none")]
    pub selected: Option<String>,
    #[serde(alias = "text", skip_serializing_if = "Option::is_none")]
    pub primary: Option<String>,
    #[serde(
        alias = "subtext",
        alias = "description",
        skip_serializing_if = "Option::is_none"
    )]
    pub secondary: Option<String>,
    #[serde(
        alias = "match",
        alias = "accent",
        skip_serializing_if = "Option::is_none"
    )]
    pub highlight: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Plugins {
    pub shell_prefix: String,
    pub shell: Option<String>,
    pub dynamic: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Binary {
    pub prefix: String,
    pub show_path: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct History {
    pub enabled: bool,
    pub max_entries: usize,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Keybindings {
    pub execute: String,
    pub close: String,
    pub autocomplete: String,
    pub next: String,
    pub previous: String,
    pub clear: String,
    pub next_arrow: String,
    pub previous_arrow: String,
    pub left_arrow: String,
    pub right_arrow: String,
    pub backspace: String,
    pub delete_word: String,
    pub delete_word_back: String,
    pub delete_forward: String,
    pub delete_result: String,
    pub delete_word_forward: String,
    pub word_left: String,
    pub word_right: String,
    pub beginning: String,
    pub end: String,
    pub help: String,
}

impl Default for General {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            width: 640,
            height: None,
            max_results: 8,
            fuzzy_search: true,
            remember_history: true,
            monitor: None,
            terminal: None,
        }
    }
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            font_family: "sans-serif".into(),
            font_size: 14.0,
            border_radius: 12.0,
            show_icons: true,
            show_description: true,
        }
    }
}
impl Default for Plugins {
    fn default() -> Self {
        Self {
            shell_prefix: "!".into(),
            shell: None,
            dynamic: true,
        }
    }
}
impl Default for Binary {
    fn default() -> Self {
        Self {
            prefix: "$ ".into(),
            show_path: true,
        }
    }
}
impl Default for History {
    fn default() -> Self {
        Self {
            enabled: true,
            max_entries: 500,
        }
    }
}
impl Default for Keybindings {
    fn default() -> Self {
        Self {
            execute: "Return".into(),
            close: "Escape".into(),
            autocomplete: "Tab".into(),
            next: "Ctrl+n".into(),
            previous: "Ctrl+p".into(),
            clear: "Ctrl+u".into(),
            next_arrow: "Down".into(),
            previous_arrow: "Up".into(),
            left_arrow: "Left".into(),
            right_arrow: "Right".into(),
            backspace: "BackSpace".into(),
            delete_word: "Ctrl+w".into(),
            delete_word_back: "Ctrl+BackSpace".into(),
            delete_forward: "Delete".into(),
            delete_result: "Shift+Delete".into(),
            delete_word_forward: "Ctrl+Delete".into(),
            word_left: "Ctrl+Left".into(),
            word_right: "Ctrl+Right".into(),
            beginning: "Ctrl+a".into(),
            end: "Ctrl+e".into(),
            help: "F1".into(),
        }
    }
}

impl Config {
    pub fn load_or_create() -> Result<Self, ConfigError> {
        Self::load_or_create_at(config_path())
    }
    pub fn load_or_create_at(path: PathBuf) -> Result<Self, ConfigError> {
        match fs::read_to_string(&path) {
            Ok(text) => {
                let config: Self = toml::from_str(&text)?;
                config.validate()?;
                Ok(config)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let config = Self::default();
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(
                    path,
                    toml::to_string_pretty(&config).map_err(ConfigError::Serialize)?,
                )?;
                Ok(config)
            }
            Err(error) => Err(error.into()),
        }
    }
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !(240..=4096).contains(&self.general.width) {
            return Err(ConfigError::Invalid(
                "general.width must be between 240 and 4096",
            ));
        }
        if self
            .general
            .height
            .is_some_and(|height| !(120..=2160).contains(&height))
        {
            return Err(ConfigError::Invalid(
                "general.height must be between 120 and 2160",
            ));
        }
        if self
            .general
            .height
            .is_some_and(|height| u64::from(height) * u64::from(self.general.width) > 4_000_000)
        {
            return Err(ConfigError::Invalid(
                "configured width × height exceeds the shared-memory safety limit",
            ));
        }
        if self.general.max_results == 0 || self.general.max_results > 100 {
            return Err(ConfigError::Invalid(
                "general.max_results must be between 1 and 100",
            ));
        }
        if !matches!(
            self.general.theme.to_ascii_lowercase().as_str(),
            "dark" | "light" | "custom"
        ) {
            return Err(ConfigError::Invalid(
                "general.theme must be 'dark', 'light', or 'custom'",
            ));
        }
        let validate_color =
            |name: &'static str, val: &Option<String>| -> Result<(), ConfigError> {
                if let Some(color_str) = val {
                    if parse_color(color_str).is_none() {
                        return Err(ConfigError::InvalidColor(format!(
                            "invalid color {color_str:?} for 'colors.{name}'"
                        )));
                    }
                }
                Ok(())
            };

        validate_color("background", &self.colors.background)?;
        validate_color("input", &self.colors.input)?;
        validate_color("selected", &self.colors.selected)?;
        validate_color("primary", &self.colors.primary)?;
        validate_color("secondary", &self.colors.secondary)?;
        validate_color("highlight", &self.colors.highlight)?;
        validate_color("border", &self.colors.border)?;
        if !(8.0..=48.0).contains(&self.appearance.font_size) {
            return Err(ConfigError::Invalid(
                "appearance.font_size must be between 8 and 48",
            ));
        }
        if !(0.0..=64.0).contains(&self.appearance.border_radius) {
            return Err(ConfigError::Invalid(
                "appearance.border_radius must be between 0 and 64",
            ));
        }
        if self.plugins.shell_prefix.is_empty() || self.binary.prefix.is_empty() {
            return Err(ConfigError::Invalid("plugin prefixes cannot be empty"));
        }
        if self.plugins.shell_prefix == self.binary.prefix {
            return Err(ConfigError::Invalid(
                "shell and binary prefixes must be unique",
            ));
        }
        let mut unique = std::collections::HashSet::new();
        for binding in self.keybindings.all() {
            let parsed =
                crate::keybindings::Binding::parse(binding).map_err(ConfigError::InvalidBinding)?;
            if !unique.insert(parsed) {
                return Err(ConfigError::InvalidBinding(format!(
                    "conflicting binding {binding:?}"
                )));
            }
        }
        Ok(())
    }
}

impl Keybindings {
    pub fn all(&self) -> [&str; 21] {
        [
            &self.execute,
            &self.close,
            &self.autocomplete,
            &self.next,
            &self.previous,
            &self.clear,
            &self.next_arrow,
            &self.previous_arrow,
            &self.left_arrow,
            &self.right_arrow,
            &self.backspace,
            &self.delete_word,
            &self.delete_word_back,
            &self.delete_forward,
            &self.delete_result,
            &self.delete_word_forward,
            &self.word_left,
            &self.word_right,
            &self.beginning,
            &self.end,
            &self.help,
        ]
    }
}

fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn config_dir() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("waymenu")
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot access configuration: {0}")]
    Io(#[from] io::Error),
    #[error("invalid configuration syntax: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("cannot serialize default configuration: {0}")]
    Serialize(toml::ser::Error),
    #[error("invalid configuration: {0}")]
    Invalid(&'static str),
    #[error("invalid keybinding: {0}")]
    InvalidBinding(String),
    #[error("invalid color: {0}")]
    InvalidColor(String),
}

pub fn parse_hex_color(hex: &str) -> Option<[u8; 4]> {
    let hex = hex.trim().trim_start_matches('#');
    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            Some([r, g, b, 255])
        }
        4 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            let a = u8::from_str_radix(&hex[3..4], 16).ok()? * 17;
            Some([r, g, b, a])
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some([r, g, b, 255])
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some([r, g, b, a])
        }
        _ => None,
    }
}

pub fn parse_color(input: &str) -> Option<[u8; 4]> {
    let trimmed = input.trim();
    if trimmed.starts_with('#')
        || (!trimmed.contains('(') && trimmed.chars().all(|c| c.is_ascii_hexdigit()))
    {
        return parse_hex_color(trimmed);
    }
    if let Some(inner) = trimmed
        .strip_prefix("rgba(")
        .or_else(|| trimmed.strip_prefix("rgb("))
    {
        let inner = inner.strip_suffix(')')?;
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        if parts.len() == 3 {
            let r = parts[0].parse::<u8>().ok()?;
            let g = parts[1].parse::<u8>().ok()?;
            let b = parts[2].parse::<u8>().ok()?;
            return Some([r, g, b, 255]);
        } else if parts.len() == 4 {
            let r = parts[0].parse::<u8>().ok()?;
            let g = parts[1].parse::<u8>().ok()?;
            let b = parts[2].parse::<u8>().ok()?;
            let a = if let Ok(val) = parts[3].parse::<f32>() {
                if (0.0..=1.0).contains(&val) {
                    (val * 255.0).round() as u8
                } else if (0.0..=255.0).contains(&val) {
                    val.round() as u8
                } else {
                    return None;
                }
            } else {
                return None;
            };
            return Some([r, g, b, a]);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_conflicting_prefixes() {
        let mut config = Config::default();
        config.binary.prefix = "!".into();
        assert!(config.validate().is_err());
    }

    #[test]
    fn default_binary_prefix_has_trailing_space() {
        let config = Config::default();
        assert_eq!(config.binary.prefix, "$ ");
    }

    #[test]
    fn creates_a_valid_default_configuration() {
        let path = std::env::temp_dir().join(format!(
            "waymenu-config-{}-{}.toml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let config = Config::load_or_create_at(path.clone()).unwrap();
        assert_eq!(config.plugins.shell_prefix, "!");
        assert!(fs::read_to_string(&path).unwrap().contains("[keybindings]"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_excessive_shared_memory_dimensions() {
        let mut config = Config::default();
        config.general.width = 4096;
        config.general.height = Some(2160);
        assert!(config.validate().is_err());
    }

    #[test]
    fn accepts_unknown_fields_but_rejects_binding_conflicts() {
        let config: Config = toml::from_str("[general]\nfuture_option = true\n").unwrap();
        assert!(config.validate().is_ok());
        let mut config = Config::default();
        config.keybindings.close = config.keybindings.execute.clone();
        assert!(config.validate().is_err());
    }

    #[test]
    fn parses_supported_color_formats() {
        assert_eq!(parse_color("#fff"), Some([255, 255, 255, 255]));
        assert_eq!(parse_color("#1234"), Some([17, 34, 51, 68]));
        assert_eq!(parse_color("#191b22"), Some([25, 27, 34, 255]));
        assert_eq!(parse_color("#191b2280"), Some([25, 27, 34, 128]));
        assert_eq!(parse_color("191b22"), Some([25, 27, 34, 255]));
        assert_eq!(parse_color("rgb(25, 27, 34)"), Some([25, 27, 34, 255]));
        assert_eq!(
            parse_color("rgba(25, 27, 34, 0.5)"),
            Some([25, 27, 34, 128])
        );
        assert_eq!(
            parse_color("rgba(25, 27, 34, 255)"),
            Some([25, 27, 34, 255])
        );

        assert_eq!(parse_color("not-a-color"), None);
        assert_eq!(parse_color("#xyz"), None);
        assert_eq!(parse_color("#12"), None);
        assert_eq!(parse_color("rgb(1, 2)"), None);
    }

    #[test]
    fn validates_custom_colors_in_config() {
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
        assert!(config.validate().is_ok());
        assert_eq!(config.colors.background.as_deref(), Some("#282a36"));
        assert_eq!(config.colors.highlight.as_deref(), Some("#ff79c6"));
        assert_eq!(config.colors.border.as_deref(), Some("#bd93f9"));
    }

    #[test]
    fn deserializes_color_aliases() {
        let toml_str = r##"
            [colors]
            selection = "#6272a4"
            text = "#f8f8f2"
            description = "#bfbfbf"
            accent = "#ff79c6"
        "##;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.validate().is_ok());
        assert_eq!(config.colors.selected.as_deref(), Some("#6272a4"));
        assert_eq!(config.colors.primary.as_deref(), Some("#f8f8f2"));
        assert_eq!(config.colors.secondary.as_deref(), Some("#bfbfbf"));
        assert_eq!(config.colors.highlight.as_deref(), Some("#ff79c6"));
    }

    #[test]
    fn rejects_invalid_colors() {
        let toml_str = r#"
            [colors]
            background = "invalid-hex"
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.validate().is_err());
    }
}
