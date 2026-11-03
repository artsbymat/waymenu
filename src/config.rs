use serde::{Deserialize, Serialize};
use std::{env, fs, io, path::PathBuf};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub appearance: Appearance,
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
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Plugins {
    pub shell_prefix: String,
    pub shell: Option<String>,
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
    pub backspace: String,
    pub delete_word: String,
    pub beginning: String,
    pub end: String,
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
        }
    }
}
impl Default for Binary {
    fn default() -> Self {
        Self {
            prefix: "$".into(),
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
            backspace: "BackSpace".into(),
            delete_word: "Ctrl+w".into(),
            beginning: "Ctrl+a".into(),
            end: "Ctrl+e".into(),
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
            "dark" | "light"
        ) {
            return Err(ConfigError::Invalid(
                "general.theme must be 'dark' or 'light'",
            ));
        }
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
    pub fn all(&self) -> [&str; 12] {
        [
            &self.execute,
            &self.close,
            &self.autocomplete,
            &self.next,
            &self.previous,
            &self.clear,
            &self.next_arrow,
            &self.previous_arrow,
            &self.backspace,
            &self.delete_word,
            &self.beginning,
            &self.end,
        ]
    }
}

fn config_path() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("waymenu/config.toml")
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
}
