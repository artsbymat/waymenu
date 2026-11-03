pub mod applications;
pub mod binary;
pub mod config;
pub mod history;
pub mod keybindings;
pub mod plugins;
pub mod search;
pub mod ui;
pub mod wayland;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub description: String,
    pub score: i64,
    pub icon: Option<String>,
    pub image: Option<String>,
    pub metadata: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    LaunchApplication(String),
    LaunchBinary(String),
    RunShell(String),
    CopyText(String),
    None,
}
