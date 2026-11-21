#[path = "../../history-shared/mod.rs"]
mod history_plugin;

use std::{ffi::CStr, path::PathBuf};

use history_plugin::{absolute_env_dir, config_dir};

const PLUGIN_ID: &CStr = c"chrome-history";
const PLUGIN_NAME: &CStr = c"Chrome History";
const PLUGIN_DESCRIPTION: &CStr = c"Search page history across Google Chrome profiles.";
const BROWSER_NAME: &str = "Chrome";
const BROWSER_EXECUTABLE: &str = "google-chrome-stable";
const BROWSER_ICON: &str = "google-chrome";

fn data_dir() -> Option<PathBuf> {
    absolute_env_dir("WAYMENU_CHROME_USER_DATA_DIR")
        .or_else(|| absolute_env_dir("CHROME_USER_DATA_DIR"))
        .or_else(|| {
            absolute_env_dir("CHROME_CONFIG_HOME")
                .or_else(config_dir)
                .map(|config| config.join("google-chrome"))
        })
}
