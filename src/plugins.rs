use crate::{config::Config, Action, SearchResult};
use libloading::Library;
use std::{
    ffi::{c_char, c_void, CStr, CString},
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub trait Plugin {
    fn id(&self) -> &str;
    fn prefix(&self) -> &str;
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn search(&self, query: &str) -> Vec<SearchResult>;
    fn action(&self, result: &SearchResult) -> Action;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginMetadata {
    pub id: String,
    pub prefix: String,
    pub name: String,
    pub description: String,
}

pub struct ShellOutput {
    pub command: String,
    pub output: String,
    pub success: bool,
}

pub fn execute_shell_cancellable(
    shell: String,
    command: String,
    cancel: Arc<AtomicBool>,
) -> ShellOutput {
    use std::os::unix::process::CommandExt;
    let mut process = Command::new(shell);
    process
        .args(["-c", &command])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            return ShellOutput {
                command,
                output: error.to_string(),
                success: false,
            }
        }
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_reader = std::thread::spawn(move || read_stream(stdout));
    let stderr_reader = std::thread::spawn(move || read_stream(stderr));
    let process_group = child.id() as i32;
    let status = loop {
        if cancel.load(Ordering::Acquire) {
            // SAFETY: kill is a process-control syscall with no memory arguments. The child
            // was placed in a fresh process group whose id is its pid; the negative id targets
            // only that group and cannot alias the launcher process.
            unsafe {
                libc::kill(-process_group, libc::SIGKILL);
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => break None,
        }
    };
    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();
    let mut bytes = stdout.bytes;
    bytes.extend(stderr.bytes);
    let output = if cancel.load(Ordering::Acquire) {
        "Command cancelled".into()
    } else {
        let mut output = String::from_utf8_lossy(&bytes).into_owned();
        if stdout.truncated || stderr.truncated {
            output.push_str("\n[output truncated]");
        }
        output
    };
    ShellOutput {
        command,
        output,
        success: status.is_some_and(|status| status.success()),
    }
}

#[derive(Default)]
struct Captured {
    bytes: Vec<u8>,
    truncated: bool,
}

fn read_stream<T: Read>(stream: Option<T>) -> Captured {
    const LIMIT: usize = 64 * 1024;
    let mut captured = Captured::default();
    if let Some(mut stream) = stream {
        let mut buffer = [0u8; 4096];
        while let Ok(read) = stream.read(&mut buffer) {
            if read == 0 {
                break;
            }
            let remaining = LIMIT.saturating_sub(captured.bytes.len());
            captured
                .bytes
                .extend_from_slice(&buffer[..read.min(remaining)]);
            captured.truncated |= read > remaining;
        }
    }
    captured
}

pub struct PluginManager {
    plugins: Vec<Box<dyn Plugin>>,
}
impl PluginManager {
    pub fn from_config(config: &Config) -> Self {
        Self::from_config_at(config, &dynamic_plugin_dir())
    }

    fn from_config_at(config: &Config, directory: &Path) -> Self {
        let mut plugins: Vec<Box<dyn Plugin>> = vec![Box::new(ShellPlugin::new(
            config.plugins.shell_prefix.clone(),
            config.plugins.shell.clone(),
        ))];
        if config.plugins.dynamic {
            match discover_dynamic_plugins(directory) {
                Ok(paths) => {
                    for path in paths {
                        match DynamicPlugin::load(&path) {
                            Ok(plugin) => {
                                if plugins.iter().any(|loaded| {
                                    loaded.prefix() == plugin.prefix() || loaded.id() == plugin.id()
                                }) {
                                    eprintln!(
                                        "waymenu: skipping plugin {} from {} because its id or prefix is already registered",
                                        plugin.id(),
                                        path.display()
                                    );
                                } else {
                                    plugins.push(Box::new(plugin));
                                }
                            }
                            Err(error) => eprintln!(
                                "waymenu: could not load plugin {}: {error}",
                                path.display()
                            ),
                        }
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => eprintln!(
                    "waymenu: could not read plugin directory {}: {error}",
                    directory.display()
                ),
            }
        }
        Self { plugins }
    }
    pub fn select<'a>(&'a self, input: &'a str) -> Option<(&'a dyn Plugin, &'a str)> {
        self.plugins.iter().find_map(|plugin| {
            let prefix = plugin.prefix();
            let query = input.strip_prefix(prefix)?;
            if prefix.chars().last().is_some_and(char::is_alphanumeric)
                && !query.is_empty()
                && !query.starts_with(char::is_whitespace)
            {
                return None;
            }
            Some((plugin.as_ref(), query.trim_start()))
        })
    }

    pub fn metadata(&self) -> Vec<PluginMetadata> {
        self.plugins
            .iter()
            .map(|plugin| PluginMetadata {
                id: plugin.id().to_owned(),
                prefix: plugin.prefix().to_owned(),
                name: plugin.name().to_owned(),
                description: plugin.description().to_owned(),
            })
            .collect()
    }
}

fn dynamic_plugin_dir() -> PathBuf {
    std::env::var_os("WAYMENU_PLUGIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::config::config_dir().join("plugins"))
}

fn discover_dynamic_plugins(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let mut paths = std::fs::read_dir(directory)?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file() && path.extension().is_some_and(|extension| extension == "so")
        })
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

struct DynamicPlugin {
    _library: Library,
    api: waymenu_plugin_api::PluginApiV2,
    state: *mut c_void,
    id: String,
    prefix: String,
    name: String,
    description: String,
}

impl DynamicPlugin {
    fn load(path: &Path) -> Result<Self, String> {
        // SAFETY: This opens a user-installed native plugin. The plugin directory is an
        // explicit trust boundary; its ABI entry point and function table are checked below.
        let library = unsafe { Library::new(path) }.map_err(|error| error.to_string())?;
        // SAFETY: The plugin SDK requires each shared library to export this exact v1 symbol.
        let entry = unsafe {
            library.get::<unsafe extern "C" fn() -> waymenu_plugin_api::PluginApiV2>(
                b"waymenu_plugin_v2\0",
            )
        }
        .map_err(|error| format!("missing waymenu_plugin_v2 entry point: {error}"))?;
        // SAFETY: The function pointer came from this still-live library and follows the SDK.
        let api = unsafe { entry() };
        if api.abi_version != waymenu_plugin_api::ABI_VERSION {
            return Err(format!(
                "unsupported plugin ABI {}, expected {}",
                api.abi_version,
                waymenu_plugin_api::ABI_VERSION
            ));
        }
        let create = api.create.ok_or("plugin has no create callback")?;
        if api.destroy.is_none()
            || api.search.is_none()
            || api.action.is_none()
            || api.free_json.is_none()
        {
            return Err("plugin API is missing a required callback".into());
        }
        // SAFETY: Metadata pointers must be static NUL-terminated strings per ABI v1.
        let id = unsafe { copy_c_string(api.id) }.ok_or("plugin id is null")?;
        // SAFETY: Same metadata contract as above.
        let prefix = unsafe { copy_c_string(api.prefix) }.ok_or("plugin prefix is null")?;
        // SAFETY: Same metadata contract as above.
        let name = unsafe { copy_c_string(api.name) }.ok_or("plugin name is null")?;
        // SAFETY: Same metadata contract as above.
        let description =
            unsafe { copy_c_string(api.description) }.ok_or("plugin description is null")?;
        if id.is_empty() || prefix.is_empty() || name.is_empty() || description.is_empty() {
            return Err("plugin id, prefix, name, and description must not be empty".into());
        }
        // SAFETY: create is a valid ABI v1 callback from the still-live library.
        let state = unsafe { create() };
        if state.is_null() {
            return Err("plugin initialization failed".into());
        }
        Ok(Self {
            _library: library,
            api,
            state,
            id,
            prefix,
            name,
            description,
        })
    }

    fn response_json(&self, pointer: *mut c_char) -> Option<String> {
        if pointer.is_null() {
            return None;
        }
        // SAFETY: The plugin contract returns an owned NUL-terminated string and its matching
        // free_json callback releases it. The library is kept alive by self.
        let response = unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned();
        // SAFETY: pointer was returned by this same plugin instance and must be freed there.
        unsafe { (self.api.free_json.unwrap())(pointer) };
        Some(response)
    }
}

impl Drop for DynamicPlugin {
    fn drop(&mut self) {
        if !self.state.is_null() {
            // SAFETY: state came from create and destroy is from the same loaded library.
            unsafe { (self.api.destroy.unwrap())(self.state) };
            self.state = std::ptr::null_mut();
        }
    }
}

/// Copy ABI metadata into host-owned UTF-8 storage.
///
/// # Safety
/// `pointer` must be null or point to a valid NUL-terminated string for the duration of the call.
unsafe fn copy_c_string(pointer: *const c_char) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    // SAFETY: Caller guarantees the ABI metadata pointer is a valid static C string.
    Some(
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned(),
    )
}

impl Plugin for DynamicPlugin {
    fn id(&self) -> &str {
        &self.id
    }
    fn prefix(&self) -> &str {
        &self.prefix
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn search(&self, query: &str) -> Vec<SearchResult> {
        let Ok(query) = CString::new(query) else {
            return Vec::new();
        };
        // SAFETY: query is NUL-terminated and the state belongs to this plugin library.
        let pointer = unsafe { (self.api.search.unwrap())(self.state, query.as_ptr()) };
        let Some(response) = self.response_json(pointer) else {
            return Vec::new();
        };
        serde_json::from_str::<Vec<waymenu_plugin_api::PluginResult>>(&response)
            .unwrap_or_default()
            .into_iter()
            .map(|result| SearchResult {
                id: result.id,
                title: result.title,
                description: result.description,
                score: result.score,
                icon: result.icon,
                image: result.image,
                metadata: result.metadata,
            })
            .collect()
    }
    fn action(&self, result: &SearchResult) -> Action {
        let Ok(id) = CString::new(result.id.as_str()) else {
            return Action::None;
        };
        // SAFETY: id is NUL-terminated and the state belongs to this plugin library.
        let pointer = unsafe { (self.api.action.unwrap())(self.state, id.as_ptr()) };
        let Some(response) = self.response_json(pointer) else {
            return Action::None;
        };
        let Ok(action) = serde_json::from_str::<waymenu_plugin_api::PluginAction>(&response) else {
            return Action::None;
        };
        match action {
            waymenu_plugin_api::PluginAction::LaunchApplication(id) => {
                Action::LaunchApplication(id)
            }
            waymenu_plugin_api::PluginAction::LaunchBinary(path) => Action::LaunchBinary(path),
            waymenu_plugin_api::PluginAction::RunShell(command) => Action::RunShell(command),
            waymenu_plugin_api::PluginAction::CopyText(text) => Action::CopyText(text),
            waymenu_plugin_api::PluginAction::OpenUrl(url) => Action::OpenUrl(url),
            waymenu_plugin_api::PluginAction::None => Action::None,
        }
    }
}

pub struct ShellPlugin {
    prefix: String,
    shell: Option<String>,
}
impl ShellPlugin {
    pub fn new(prefix: String, shell: Option<String>) -> Self {
        Self { prefix, shell }
    }
    pub fn execute(&self, command: &str) -> io::Result<Output> {
        let environment_shell = std::env::var("SHELL").ok();
        let shell = self
            .shell
            .as_deref()
            .or(environment_shell.as_deref())
            .unwrap_or("/bin/sh");
        Command::new(shell)
            .args(["-c", command])
            .stdin(Stdio::null())
            .output()
    }
}
impl Plugin for ShellPlugin {
    fn id(&self) -> &str {
        "shell"
    }
    fn prefix(&self) -> &str {
        &self.prefix
    }
    fn name(&self) -> &str {
        "Shell"
    }
    fn description(&self) -> &str {
        "Run a shell command after explicit submission."
    }
    fn search(&self, query: &str) -> Vec<SearchResult> {
        if query.trim().is_empty() {
            Vec::new()
        } else {
            vec![SearchResult {
                id: query.into(),
                title: query.into(),
                description: "Execute with configured shell".into(),
                score: 0,
                icon: None,
                image: None,
                metadata: Some("shell".into()),
            }]
        }
    }
    fn action(&self, result: &SearchResult) -> Action {
        Action::RunShell(result.id.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_removes_known_prefix() {
        let manager = PluginManager::from_config(&Config::default());
        let (plugin, query) = manager.select("!printf test").unwrap();
        assert_eq!(plugin.id(), "shell");
        assert_eq!(query, "printf test");
    }

    #[test]
    fn manager_selects_space_delimited_prefix_without_hijacking_apps() {
        struct MockGooglePlugin;
        impl Plugin for MockGooglePlugin {
            fn id(&self) -> &str {
                "google"
            }
            fn prefix(&self) -> &str {
                "g "
            }
            fn name(&self) -> &str {
                "Google"
            }
            fn description(&self) -> &str {
                "Search"
            }
            fn search(&self, _: &str) -> Vec<SearchResult> {
                Vec::new()
            }
            fn action(&self, _: &SearchResult) -> Action {
                Action::None
            }
        }

        let manager = PluginManager {
            plugins: vec![
                Box::new(ShellPlugin::new("!".into(), None)),
                Box::new(MockGooglePlugin),
            ],
        };

        // Matches with query
        let (plugin, query) = manager.select("g hyprland linux").unwrap();
        assert_eq!(plugin.id(), "google");
        assert_eq!(query, "hyprland linux");

        // Matches prefix with trailing space and empty query
        let (plugin, query) = manager.select("g ").unwrap();
        assert_eq!(plugin.id(), "google");
        assert_eq!(query, "");

        // Does NOT hijack "g" alone or words starting with 'g' (e.g. google chrome, gimp, git)
        assert!(manager.select("g").is_none());
        assert!(manager.select("gimp").is_none());
        assert!(manager.select("google-chrome").is_none());
        assert!(manager.select("git").is_none());
    }

    #[test]
    fn loads_and_executes_built_google_plugin() {
        let so_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("plugins/google/target/release/libgoogle.so");
        if !so_path.exists() {
            return;
        }
        let plugin = DynamicPlugin::load(&so_path).unwrap();
        assert_eq!(plugin.id(), "google");
        assert_eq!(plugin.prefix(), "g ");

        let results = plugin.search("hyprland linux");
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].id,
            "https://www.google.com/search?q=hyprland+linux"
        );
        assert_eq!(results[0].title, "Search Google for \"hyprland linux\"");

        let action = plugin.action(&results[0]);
        assert_eq!(
            action,
            Action::OpenUrl("https://www.google.com/search?q=hyprland+linux".into())
        );
    }

    #[test]
    fn loads_and_executes_built_rink_plugin() {
        let so_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("plugins/rink/target/release/librink.so");
        if !so_path.exists() {
            return;
        }
        let plugin = DynamicPlugin::load(&so_path).unwrap();
        assert_eq!(plugin.id(), "rink");
        assert_eq!(plugin.prefix(), "= ");

        let results = plugin.search("2+2");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "4");
    }

    #[test]
    fn dynamic_plugins_are_optional_and_only_shared_objects_are_discovered() {
        let directory =
            std::env::temp_dir().join(format!("waymenu-plugins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("ignored.txt"), b"not a plugin").unwrap();
        std::fs::write(directory.join("libzeta.so"), b"not loaded by discovery").unwrap();
        std::fs::write(directory.join("libalpha.so"), b"not loaded by discovery").unwrap();

        let plugins = discover_dynamic_plugins(&directory).unwrap();
        assert_eq!(plugins[0].file_name().unwrap(), "libalpha.so");
        assert_eq!(plugins[1].file_name().unwrap(), "libzeta.so");

        let mut config = Config::default();
        config.plugins.dynamic = false;
        let manager = PluginManager::from_config_at(&config, &directory);
        assert_eq!(manager.plugins.len(), 1);
        assert!(manager.select("=2+2").is_none());
        assert_eq!(manager.select("!pwd").unwrap().0.id(), "shell");
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn cancellable_executor_captures_stdout() {
        let cancel = Arc::new(AtomicBool::new(false));
        let result = execute_shell_cancellable("/bin/sh".into(), "printf waymenu".into(), cancel);
        assert!(result.success);
        assert_eq!(result.output, "waymenu");
    }
    #[test]
    fn executor_bounds_large_output() {
        let cancel = Arc::new(AtomicBool::new(false));
        let result =
            execute_shell_cancellable("/bin/sh".into(), "yes x | head -c 100000".into(), cancel);
        assert!(result.output.len() < 70_000);
        assert!(result.output.ends_with("[output truncated]"));
    }

    #[test]
    fn executor_honors_preexisting_cancellation() {
        let cancel = Arc::new(AtomicBool::new(true));
        let started = std::time::Instant::now();
        let result = execute_shell_cancellable("/bin/sh".into(), "sleep 5".into(), cancel);
        assert!(!result.success);
        assert_eq!(result.output, "Command cancelled");
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
