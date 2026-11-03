use crate::{config::Config, Action, SearchResult};
use std::{
    io::{self, Read},
    process::{Command, Output, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub trait Plugin: Send + Sync {
    fn id(&self) -> &'static str;
    fn prefix(&self) -> &str;
    fn name(&self) -> &'static str;
    fn search(&self, query: &str) -> Vec<SearchResult>;
    fn action(&self, result: &SearchResult) -> Action;
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
        Self {
            plugins: vec![Box::new(ShellPlugin::new(
                config.plugins.shell_prefix.clone(),
                config.plugins.shell.clone(),
            ))],
        }
    }
    pub fn select<'a>(&'a self, input: &'a str) -> Option<(&'a dyn Plugin, &'a str)> {
        self.plugins.iter().find_map(|plugin| {
            input
                .strip_prefix(plugin.prefix())
                .map(|query| (plugin.as_ref(), query))
        })
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
    fn id(&self) -> &'static str {
        "shell"
    }
    fn prefix(&self) -> &str {
        &self.prefix
    }
    fn name(&self) -> &'static str {
        "Shell"
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
