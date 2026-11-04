use std::{env, path::Path};
use waymenu_plugin_api::PluginAction;

/// Detect available terminal emulator, honoring user preferences first.
pub fn detect_terminal() -> Option<String> {
    if let Ok(term) = env::var("WAYMENU_PACMAN_TERMINAL") {
        let trimmed = term.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if let Ok(term) = env::var("TERMINAL") {
        let trimmed = term.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    const CANDIDATES: &[&str] = &[
        "foot",
        "alacritty",
        "kitty",
        "ghostty",
        "wezterm",
        "gnome-terminal",
        "konsole",
        "xfce4-terminal",
        "xterm",
    ];

    for candidate in CANDIDATES {
        if is_executable_in_path(candidate) {
            return Some((*candidate).to_string());
        }
    }

    None
}

/// Checks whether an executable exists and has execute permissions in PATH.
pub fn is_executable_in_path(name: &str) -> bool {
    let path = Path::new(name);
    if path.is_absolute() || name.contains('/') {
        return is_executable_file(path);
    }
    let Some(path_var) = env::var_os("PATH") else {
        return false;
    };
    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if is_executable_file(&candidate) {
            return true;
        }
    }
    false
}

fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = path.metadata() {
            return meta.permissions().mode() & 0o111 != 0;
        }
        false
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Wraps a command so it executes inside the specified terminal emulator.
pub fn format_terminal_command(term: &str, command: &str) -> String {
    let basename = Path::new(term)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(term);

    let hold_enabled = env::var("WAYMENU_PACMAN_NO_HOLD").as_deref() != Ok("1");
    let escaped = command.replace('\'', "'\\''");

    let script = if hold_enabled {
        format!("{escaped}; echo; read -n 1 -s -r -p 'Press any key to close...'")
    } else {
        escaped
    };

    match basename {
        "kitty" => format!("{term} sh -c '{script}'"),
        "wezterm" => format!("{term} start -- sh -c '{script}'"),
        "gnome-terminal" => format!("{term} -- sh -c '{script}'"),
        _ => format!("{term} -e sh -c '{script}'"),
    }
}

/// Build the final shell action to run a pacman command in a terminal.
pub fn run_in_terminal(command: &str) -> PluginAction {
    match detect_terminal() {
        Some(term) => {
            let full_command = format_terminal_command(&term, command);
            PluginAction::RunShell(full_command)
        }
        None => {
            // Safe fallback when no terminal emulator could be detected in PATH.
            PluginAction::CopyText(command.to_string())
        }
    }
}

/// Resolves the command prefix for installing packages.
pub fn install_command_prefix() -> String {
    if let Ok(wrapper) = env::var("WAYMENU_PACMAN_WRAPPER") {
        let trimmed = wrapper.trim();
        if !trimmed.is_empty() {
            return format!("{trimmed} -S");
        }
    }
    if let Ok(cmd) = env::var("WAYMENU_PACMAN_CMD") {
        let trimmed = cmd.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    "sudo pacman -S".to_string()
}

/// Resolves the command prefix for removing packages.
pub fn remove_command_prefix() -> String {
    if let Ok(wrapper) = env::var("WAYMENU_PACMAN_WRAPPER") {
        let trimmed = wrapper.trim();
        if !trimmed.is_empty() {
            return format!("{trimmed} -Rns");
        }
    }
    "sudo pacman -Rns".to_string()
}

/// Resolves the command for system upgrade.
pub fn upgrade_command() -> String {
    if let Ok(wrapper) = env::var("WAYMENU_PACMAN_WRAPPER") {
        let trimmed = wrapper.trim();
        if !trimmed.is_empty() {
            return format!("{trimmed} -Syu");
        }
    }
    "sudo pacman -Syu".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_terminal_command_with_wait_prompt() {
        let cmd = format_terminal_command("foot", "sudo pacman -S ripgrep");
        assert!(cmd.starts_with("foot -e sh -c 'sudo pacman -S ripgrep; echo; read"));
        assert!(cmd.contains("Press any key to close..."));
    }

    #[test]
    fn formats_kitty_command_without_dash_e() {
        let cmd = format_terminal_command("kitty", "sudo pacman -S ripgrep");
        assert!(cmd.starts_with("kitty sh -c 'sudo pacman -S ripgrep;"));
    }

    #[test]
    fn formats_wezterm_command() {
        let cmd = format_terminal_command("/usr/bin/wezterm", "sudo pacman -S ripgrep");
        assert!(cmd.starts_with("/usr/bin/wezterm start -- sh -c 'sudo pacman -S ripgrep;"));
    }

    #[test]
    fn escapes_single_quotes_in_command() {
        let cmd = format_terminal_command("foot", "echo 'hello world'");
        assert!(cmd.contains("echo '\\''hello world'\\''"));
    }
}
