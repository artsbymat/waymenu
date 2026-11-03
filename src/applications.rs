use std::{
    collections::HashSet,
    env,
    ffi::OsStr,
    fs, io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command},
};

use crate::SearchResult;

#[derive(Clone, Debug)]
pub struct DesktopApplication {
    pub id: String,
    pub name: String,
    pub description: String,
    pub exec: String,
    pub icon: Option<String>,
    pub terminal: bool,
    pub working_directory: Option<PathBuf>,
    pub desktop_file: PathBuf,
}

impl DesktopApplication {
    pub fn as_result(&self, history_score: i64) -> SearchResult {
        SearchResult {
            id: self.id.clone(),
            title: self.name.clone(),
            description: self.description.clone(),
            score: history_score,
            icon: self.icon.clone(),
            image: None,
            metadata: None,
        }
    }

    pub fn launch(&self, terminal_command: Option<&str>) -> io::Result<Child> {
        let args = expand_exec(
            &self.exec,
            &self.name,
            self.icon.as_deref(),
            Some(&self.desktop_file),
        )?;
        let (program, rest) = args.split_first().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "desktop entry has an empty Exec value",
            )
        })?;
        let mut command = if self.terminal {
            let environment_terminal = env::var("TERMINAL").ok();
            let terminal = terminal_command
                .or(environment_terminal.as_deref())
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::Unsupported,
                        "Terminal=true entry requires general.terminal or $TERMINAL",
                    )
                })?;
            let mut parts = split_command(terminal)?;
            let executable = parts.first().cloned().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "terminal command is empty")
            })?;
            let mut command = Command::new(executable);
            command
                .args(parts.drain(1..))
                .arg("-e")
                .arg(program)
                .args(rest);
            command
        } else {
            let mut command = Command::new(program);
            command.args(rest);
            command
        };
        if let Some(directory) = &self.working_directory {
            command.current_dir(directory);
        }
        command.spawn()
    }
}

pub fn discover() -> Vec<DesktopApplication> {
    let mut result = Vec::new();
    let mut ids = HashSet::new();
    for directory in application_dirs() {
        visit_desktop_files(&directory, &mut |path| {
            let id = path
                .strip_prefix(&directory)
                .ok()
                .and_then(Path::to_str)
                .unwrap_or_default()
                .replace('/', "-");
            if ids.contains(&id) {
                return;
            }
            ids.insert(id.clone());
            if let Ok(text) = fs::read_to_string(path) {
                if let Some(app) = parse_desktop(&id, path, &text) {
                    result.push(app);
                }
            }
        });
    }
    result
}

pub fn launch_by_id(id: &str, terminal: Option<&str>) -> io::Result<Child> {
    discover()
        .into_iter()
        .find(|application| application.id == id)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "application is no longer installed",
            )
        })?
        .launch(terminal)
}

fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let data_home = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    if let Some(home) = data_home {
        dirs.push(home.join("applications"));
    }
    let system =
        env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    dirs.extend(env::split_paths(&system).map(|path| path.join("applications")));
    dirs
}

fn visit_desktop_files(dir: &Path, visit: &mut impl FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            visit_desktop_files(&path, visit);
        } else if path.extension() == Some(OsStr::new("desktop")) {
            visit(&path);
        }
    }
}

fn parse_desktop(id: &str, path: &Path, input: &str) -> Option<DesktopApplication> {
    let mut in_entry = false;
    let mut name = None;
    let mut description = String::new();
    let mut exec = None;
    let mut icon = None;
    let mut terminal = false;
    let mut working_directory = None;
    let mut only_show_in = None;
    let mut not_show_in = None;
    let mut try_exec = None;
    let mut generic_name = None;
    let mut keywords = String::new();
    let mut categories = String::new();
    let locale = current_locale();
    let mut localized_name = None;
    let mut localized_description = None;
    for raw in input.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "Type" if value != "Application" => return None,
            "Hidden" | "NoDisplay" if value.eq_ignore_ascii_case("true") => return None,
            "Name" => name = Some(unescape_desktop_value(value)?),
            "Comment" => description = unescape_desktop_value(value)?,
            "GenericName" => generic_name = Some(unescape_desktop_value(value)?),
            "Keywords" => keywords = unescape_desktop_value(value)?.replace(';', " "),
            "Categories" => categories = unescape_desktop_value(value)?.replace(';', " "),
            key if localized_key_priority(key, "Name", &locale).is_some() => {
                let priority = localized_key_priority(key, "Name", &locale)?;
                if localized_name
                    .as_ref()
                    .is_none_or(|(current, _)| priority > *current)
                {
                    localized_name = Some((priority, unescape_desktop_value(value)?));
                }
            }
            key if localized_key_priority(key, "Comment", &locale).is_some() => {
                let priority = localized_key_priority(key, "Comment", &locale)?;
                if localized_description
                    .as_ref()
                    .is_none_or(|(current, _)| priority > *current)
                {
                    localized_description = Some((priority, unescape_desktop_value(value)?));
                }
            }
            "Exec" => exec = Some(value.to_owned()),
            "TryExec" => try_exec = Some(unescape_desktop_value(value)?),
            "Icon" => icon = Some(unescape_desktop_value(value)?),
            "Terminal" => terminal = value.eq_ignore_ascii_case("true"),
            "Path" => working_directory = Some(PathBuf::from(unescape_desktop_value(value)?)),
            "OnlyShowIn" => only_show_in = Some(unescape_desktop_value(value)?),
            "NotShowIn" => not_show_in = Some(unescape_desktop_value(value)?),
            _ => {}
        }
    }
    let desktops = current_desktops();
    if only_show_in
        .as_deref()
        .is_some_and(|list| !desktop_list_matches(list, &desktops))
        || not_show_in
            .as_deref()
            .is_some_and(|list| desktop_list_matches(list, &desktops))
        || try_exec
            .as_deref()
            .is_some_and(|program| !executable_exists(program))
    {
        return None;
    }
    description = localized_description
        .map(|(_, value)| value)
        .unwrap_or(description);
    if let Some(generic) = generic_name {
        if !description.contains(&generic) {
            description.push_str(" · ");
            description.push_str(&generic);
        }
    }
    for aliases in [keywords, categories] {
        if !aliases.is_empty() {
            description.push_str(" · ");
            description.push_str(&aliases);
        }
    }
    Some(DesktopApplication {
        id: id.to_owned(),
        name: localized_name.map(|(_, value)| value).or(name)?,
        description,
        exec: exec?,
        icon,
        terminal,
        working_directory,
        desktop_file: path.to_owned(),
    })
}

pub fn expand_exec(
    exec: &str,
    name: &str,
    icon: Option<&str>,
    desktop_file: Option<&Path>,
) -> io::Result<Vec<String>> {
    let words = split_command(exec)?;
    let mut output = Vec::new();
    for word in words {
        match word.as_str() {
            "%f" | "%F" | "%u" | "%U" => {}
            "%i" => {
                if let Some(icon) = icon {
                    output.extend(["--icon".to_owned(), icon.to_owned()]);
                }
            }
            "%c" => output.push(name.to_owned()),
            "%k" => {
                if let Some(path) = desktop_file {
                    output.push(path.display().to_string());
                }
            }
            _ => output.push(expand_exec_word(&word, name, desktop_file)?),
        }
    }
    Ok(output)
}

fn expand_exec_word(word: &str, name: &str, desktop_file: Option<&Path>) -> io::Result<String> {
    let mut output = String::with_capacity(word.len());
    let mut chars = word.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            output.push(ch);
            continue;
        }
        match chars.next() {
            Some('%') => output.push('%'),
            Some('c') => output.push_str(name),
            Some('k') => {
                if let Some(path) = desktop_file {
                    output.push_str(&path.display().to_string());
                }
            }
            Some('f' | 'F' | 'u' | 'U') => {}
            Some('i') => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "%i must be a standalone Exec argument",
                ))
            }
            Some(code) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unsupported Exec field code %{code}"),
                ))
            }
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "trailing % in Exec",
                ))
            }
        }
    }
    Ok(output)
}

fn current_locale() -> String {
    env::var("LC_MESSAGES")
        .or_else(|_| env::var("LANG"))
        .unwrap_or_default()
        .split('.')
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn localized_key_priority(key: &str, base: &str, locale: &str) -> Option<u8> {
    let tag = key
        .strip_prefix(base)
        .and_then(|suffix| suffix.strip_prefix('['))
        .and_then(|suffix| suffix.strip_suffix(']'))?;
    if locale.is_empty() {
        None
    } else if tag == locale {
        Some(2)
    } else if locale.split(['_', '@']).next() == Some(tag) {
        Some(1)
    } else {
        None
    }
}

fn unescape_desktop_value(value: &str) -> Option<String> {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }
        output.push(match chars.next()? {
            's' => ' ',
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            '\\' => '\\',
            _ => return None,
        });
    }
    Some(output)
}

fn current_desktops() -> Vec<String> {
    env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

fn desktop_list_matches(list: &str, desktops: &[String]) -> bool {
    list.split(';').filter(|item| !item.is_empty()).any(|item| {
        desktops
            .iter()
            .any(|desktop| desktop.eq_ignore_ascii_case(item))
    })
}

fn executable_exists(program: &str) -> bool {
    let path = Path::new(program);
    if path.components().count() > 1 {
        return is_executable(path);
    }
    env::var_os("PATH").is_some_and(|paths| {
        env::split_paths(&paths).any(|directory| is_executable(&directory.join(program)))
    })
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

fn split_command(input: &str) -> io::Result<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if quoted || escaped {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid quoting in Exec",
        ));
    }
    if !current.is_empty() {
        words.push(current);
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_and_expands_entry() {
        let app = parse_desktop("x.desktop", Path::new("/x.desktop"), "[Desktop Entry]\nType=Application\nName=My App\nExec=my-app --name %c %F\nComment=Demo\n").unwrap();
        assert_eq!(
            expand_exec(&app.exec, &app.name, None, Some(&app.desktop_file)).unwrap(),
            ["my-app", "--name", "My App"]
        );
    }
    #[test]
    fn expands_embedded_single_value_codes() {
        assert_eq!(
            expand_exec("app --title=%c %%", "Editor", None, None).unwrap(),
            ["app", "--title=Editor", "%"]
        );
    }
    #[test]
    fn desktop_string_escapes_and_locale_priority_are_deterministic() {
        assert_eq!(
            unescape_desktop_value(r"Hello\sWorld\nLine\\End").unwrap(),
            "Hello World\nLine\\End"
        );
        assert_eq!(unescape_desktop_value(r"bad\qescape"), None);
        assert_eq!(
            localized_key_priority("Name[en_US]", "Name", "en_US"),
            Some(2)
        );
        assert_eq!(localized_key_priority("Name[en]", "Name", "en_US"), Some(1));
        assert_eq!(localized_key_priority("Name[id]", "Name", "en_US"), None);
    }
}
