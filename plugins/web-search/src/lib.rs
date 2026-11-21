use std::{
    env,
    ffi::{c_char, c_void, CStr},
    fmt::Write,
    fs,
    io::ErrorKind,
    panic::{catch_unwind, AssertUnwindSafe},
    path::{Path, PathBuf},
};

use serde::Deserialize;
use waymenu_plugin_api::{
    free_json, into_json, PluginAction, PluginApiV2, PluginResult, ABI_VERSION,
    AUTOCOMPLETE_METADATA_PREFIX,
};

#[derive(Deserialize)]
struct Provider {
    name: String,
    #[serde(flatten)]
    source: Source,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Source {
    Search {
        url: String,
    },
    Bookmarks {
        #[serde(default)]
        bookmarks: Vec<Bookmark>,
    },
}

#[derive(Deserialize)]
struct Bookmark {
    name: String,
    url: String,
}

#[derive(Default)]
struct State {
    providers: Vec<(String, Provider)>,
}

impl State {
    fn load(directory: &Path) -> Self {
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) => {
                if error.kind() != ErrorKind::NotFound {
                    eprintln!("web-search: {}: {error}", directory.display());
                }
                return Self::default();
            }
        };
        let mut providers = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(shortcut) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if path.extension().is_none_or(|ext| ext != "toml")
                || !shortcut.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'-' | b'_')
                })
                || !path.is_file()
            {
                continue;
            }

            match load_provider(&path) {
                Ok(provider) => providers.push((shortcut.to_owned(), provider)),
                Err(error) => eprintln!("web-search: {}: {error}", path.display()),
            }
        }
        providers.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
        Self { providers }
    }

    fn search(&self, input: &str) -> Vec<PluginResult> {
        let input = input.trim();
        let (shortcut, query) = input.split_once(char::is_whitespace).unwrap_or((input, ""));
        let query = query.trim();

        if let Some((_, provider)) = self.providers.iter().find(|(key, _)| key == shortcut) {
            return match &provider.source {
                Source::Search { .. } if query.is_empty() => vec![result(
                    "",
                    shortcut,
                    format!("Enter a search query for {}", provider.name),
                )],
                Source::Search { url } => {
                    let url = url.replace("{query}", &encode_query(query));
                    vec![PluginResult {
                        metadata: Some(format!("{AUTOCOMPLETE_METADATA_PREFIX}{shortcut} {query}")),
                        ..result(
                            &url,
                            format!("Search {} for \"{query}\"", provider.name),
                            url.clone(),
                        )
                    }]
                }
                Source::Bookmarks { bookmarks } => {
                    let query = query.to_lowercase();
                    bookmarks
                        .iter()
                        .filter(|bookmark| {
                            query.is_empty()
                                || bookmark.name.to_lowercase().contains(&query)
                                || bookmark.url.to_lowercase().contains(&query)
                        })
                        .map(|bookmark| PluginResult {
                            metadata: Some(format!(
                                "{AUTOCOMPLETE_METADATA_PREFIX}{shortcut} {}",
                                bookmark.name
                            )),
                            ..result(&bookmark.url, &bookmark.name, &bookmark.url)
                        })
                        .collect()
                }
            };
        }

        if self.providers.is_empty() {
            return vec![result(
                "",
                "No web search shortcuts configured",
                "Add a .toml file to your Waymenu web-search directory",
            )];
        }
        self.providers
            .iter()
            .filter(|(key, _)| query.is_empty() && key.starts_with(shortcut))
            .map(|(key, provider)| result("", key, &provider.name))
            .collect()
    }
}

fn load_provider(path: &Path) -> Result<Provider, String> {
    let config = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let provider: Provider = toml::from_str(&config).map_err(|err| err.message().to_owned())?;
    if provider.name.trim().is_empty() {
        return Err("name must not be empty".into());
    }
    match &provider.source {
        Source::Search { url } => {
            let query = url.split('#').next().and_then(|url| url.split_once('?'));
            if !is_http_url(url)
                || url.matches("{query}").count() != 1
                || !query.is_some_and(|(_, query)| query.contains("{query}"))
            {
                return Err(
                    "url must be HTTP or HTTPS with one {query} in the query string".into(),
                );
            }
        }
        Source::Bookmarks { bookmarks } => {
            if bookmarks
                .iter()
                .any(|bookmark| bookmark.name.trim().is_empty() || !is_http_url(&bookmark.url))
            {
                return Err("bookmarks require a name and an HTTP or HTTPS URL".into());
            }
        }
    }
    Ok(provider)
}

fn is_http_url(url: &str) -> bool {
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return false;
    };
    rest.split(['/', '?', '#'])
        .next()
        .is_some_and(|host| !host.is_empty())
        && !rest
            .chars()
            .any(|ch| ch.is_whitespace() || ch.is_control() || ch == '\\')
}

fn encode_query(query: &str) -> String {
    let mut encoded = String::with_capacity(query.len());
    for byte in query.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push('+'),
            _ => write!(encoded, "%{byte:02X}").unwrap(),
        }
    }
    encoded
}

fn result(id: &str, title: impl Into<String>, description: impl Into<String>) -> PluginResult {
    PluginResult {
        id: id.into(),
        title: title.into(),
        description: description.into(),
        score: 10_000,
        icon: Some("web-browser".into()),
        image: None,
        metadata: None,
    }
}

fn config_directory() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .map(|path| path.join("waymenu/web-search"))
}

#[unsafe(no_mangle)]
pub extern "C" fn waymenu_plugin_v2() -> PluginApiV2 {
    PluginApiV2 {
        abi_version: ABI_VERSION,
        id: c"web-search".as_ptr(),
        prefix: c"s ".as_ptr(),
        name: c"Web Search".as_ptr(),
        description: c"Search websites and open bookmarks from TOML shortcuts.".as_ptr(),
        create: Some(create),
        destroy: Some(destroy),
        search: Some(search),
        action: Some(action),
        free_json: Some(free_json),
    }
}

unsafe extern "C" fn create() -> *mut c_void {
    catch_unwind(|| {
        let state = config_directory().map_or_else(State::default, |path| State::load(&path));
        Box::into_raw(Box::new(state)).cast()
    })
    .unwrap_or(std::ptr::null_mut())
}

unsafe extern "C" fn destroy(state: *mut c_void) {
    if !state.is_null() {
        // SAFETY: The host returns the pointer allocated in create exactly once.
        drop(unsafe { Box::from_raw(state.cast::<State>()) });
    }
}

unsafe extern "C" fn search(state: *mut c_void, query: *const c_char) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() || query.is_null() {
            return into_json(&Vec::<PluginResult>::new());
        }
        // SAFETY: The host supplies a live plugin state and a NUL-terminated query.
        let (state, query) = unsafe { (&*state.cast::<State>(), CStr::from_ptr(query)) };
        into_json(&state.search(query.to_str().unwrap_or_default()))
    }))
    .unwrap_or(std::ptr::null_mut())
}

unsafe extern "C" fn action(_state: *mut c_void, id: *const c_char) -> *mut c_char {
    catch_unwind(|| {
        if id.is_null() {
            return into_json(&PluginAction::None);
        }
        // SAFETY: The host supplies a NUL-terminated result ID.
        let id = unsafe { CStr::from_ptr(id) }.to_str().unwrap_or_default();
        if is_http_url(id) {
            into_json(&PluginAction::OpenUrl(id.to_owned()))
        } else {
            into_json(&PluginAction::None)
        }
    })
    .unwrap_or(std::ptr::null_mut())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn loads_shortcuts_from_toml_and_routes_queries() {
        let directory = env::temp_dir().join(format!(
            "waymenu-web-search-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        let empty = State::load(&directory);
        assert!(empty.search("")[0].id.is_empty());
        assert!(!directory.exists());
        fs::create_dir(&directory).unwrap();
        fs::write(
            directory.join("yt.toml"),
            r#"
name = "YouTube"
type = "search"
url = "https://www.youtube.com/results?search_query={query}&sort=recent"
"#,
        )
        .unwrap();
        fs::write(
            directory.join("next.toml"),
            r#"
name = "Next.js"
type = "bookmarks"
[[bookmarks]]
name = "Installation Guide"
url = "https://nextjs.org/docs/app/installation"
[[bookmarks]]
name = "Routing"
url = "https://nextjs.org/docs/app/routing"
"#,
        )
        .unwrap();
        fs::write(
            directory.join("bad.toml"),
            r#"
name = "Bad"
type = "search"
url = "https://{query}/"
"#,
        )
        .unwrap();
        let state = State::load(&directory);
        fs::remove_dir_all(directory).unwrap();

        let shortcuts = state.search("");
        assert_eq!(
            shortcuts
                .iter()
                .map(|row| row.title.as_str())
                .collect::<Vec<_>>(),
            ["next", "yt"]
        );
        let partial_shortcut = state.search("y");
        assert_eq!(partial_shortcut[0].title, "yt");
        assert!(partial_shortcut[0].metadata.is_none());
        assert!(state.search("yt")[0].id.is_empty());
        let search = state.search("yt C++ & é #");
        assert_eq!(
            search[0].id,
            "https://www.youtube.com/results?search_query=C%2B%2B+%26+%C3%A9+%23&sort=recent"
        );
        assert_eq!(
            search[0].metadata.as_deref(),
            Some("autocomplete:yt C++ & é #")
        );
        assert_eq!(state.search("next").len(), 2);
        let bookmark = state.search("next GUIDE");
        assert_eq!(bookmark[0].title, "Installation Guide");
        assert_eq!(
            bookmark[0].metadata.as_deref(),
            Some("autocomplete:next Installation Guide")
        );
        assert_eq!(state.search("next /routing")[0].title, "Routing");
        assert!(state.search("missing query").is_empty());
    }
}
