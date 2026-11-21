#[path = "../../history-shared/mod.rs"]
mod history_plugin;

use std::{ffi::CStr, path::PathBuf};

use history_plugin::{absolute_env_dir, config_dir};

const PLUGIN_ID: &CStr = c"braveorigin-history";
const PLUGIN_NAME: &CStr = c"Brave Origin History";
const PLUGIN_DESCRIPTION: &CStr = c"Search page history across Brave Origin profiles.";
const BROWSER_NAME: &str = "Brave Origin";
const BROWSER_EXECUTABLE: &str = "brave-origin";
const BROWSER_ICON: &str = "brave-origin";

fn data_dir() -> Option<PathBuf> {
    absolute_env_dir("WAYMENU_BRAVE_ORIGIN_USER_DATA_DIR")
        .or_else(|| config_dir().map(|config| config.join("BraveSoftware/Brave-Origin")))
}
