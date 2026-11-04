use std::{
    ffi::{c_char, c_void, CStr},
    panic::{catch_unwind, AssertUnwindSafe},
    time::{Duration, Instant},
};
use waymenu_plugin_api::{
    free_json, into_json, PluginAction, PluginApiV2, PluginResult, ABI_VERSION,
};

mod proc;
use proc::ProcessEntry;

struct State {
    processes: Vec<ProcessEntry>,
    last_refresh: Instant,
}

impl State {
    fn refresh_if_needed(&mut self, is_empty_query: bool) {
        if is_empty_query
            || self.processes.is_empty()
            || self.last_refresh.elapsed() > Duration::from_millis(1000)
        {
            self.processes = proc::list_processes();
            self.last_refresh = Instant::now();
        }
    }

    fn search(&mut self, raw_query: &str) -> Vec<PluginResult> {
        let (signal, query) = parse_signal_and_query(raw_query);
        self.refresh_if_needed(query.is_empty());
        filter_and_rank_processes(&self.processes, query, signal)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn waymenu_plugin_v2() -> PluginApiV2 {
    PluginApiV2 {
        abi_version: ABI_VERSION,
        id: c"kill".as_ptr(),
        prefix: c"k ".as_ptr(),
        name: c"Kill Process".as_ptr(),
        description: c"Search and kill running processes.".as_ptr(),
        create: Some(create),
        destroy: Some(destroy),
        search: Some(search),
        action: Some(action),
        free_json: Some(free_json),
    }
}

unsafe extern "C" fn create() -> *mut c_void {
    catch_unwind(|| {
        Box::into_raw(Box::new(State {
            processes: Vec::new(),
            last_refresh: Instant::now()
                .checked_sub(Duration::from_secs(10))
                .unwrap_or_else(Instant::now),
        }))
        .cast()
    })
    .unwrap_or(std::ptr::null_mut())
}

unsafe extern "C" fn destroy(state: *mut c_void) {
    if !state.is_null() {
        // SAFETY: State was allocated with Box::into_raw in create().
        drop(unsafe { Box::from_raw(state.cast::<State>()) });
    }
}

unsafe extern "C" fn search(state: *mut c_void, query: *const c_char) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() || query.is_null() {
            return into_json(&Vec::<PluginResult>::new());
        }
        // SAFETY: state pointer is managed by the host and valid during search().
        let state = unsafe { &mut *state.cast::<State>() };
        let Ok(raw_query) = (unsafe { CStr::from_ptr(query) }).to_str() else {
            return into_json(&Vec::<PluginResult>::new());
        };
        into_json(&state.search(raw_query))
    }))
    .unwrap_or(std::ptr::null_mut())
}

unsafe extern "C" fn action(_state: *mut c_void, result_id: *const c_char) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        if result_id.is_null() {
            return into_json(&PluginAction::None);
        }
        let Ok(id_str) = (unsafe { CStr::from_ptr(result_id) }).to_str() else {
            return into_json(&PluginAction::None);
        };
        into_json(&execute_kill(id_str))
    }))
    .unwrap_or(std::ptr::null_mut())
}

pub fn parse_signal_and_query(input: &str) -> (i32, &str) {
    let trimmed = input.trim_start();
    if let Some(rest) = trimmed.strip_prefix('-') {
        let (first_token, remainder) = match rest.split_once(char::is_whitespace) {
            Some((tok, rem)) => (tok, rem.trim_start()),
            None => (rest, ""),
        };

        let token_upper = first_token.to_ascii_uppercase();
        let sig = match token_upper.as_str() {
            "9" | "KILL" | "SIGKILL" => Some(9),
            "15" | "TERM" | "SIGTERM" => Some(15),
            "2" | "INT" | "SIGINT" => Some(2),
            "1" | "HUP" | "SIGHUP" => Some(1),
            "3" | "QUIT" | "SIGQUIT" => Some(3),
            "6" | "ABRT" | "SIGABRT" => Some(6),
            "18" | "CONT" | "SIGCONT" => Some(18),
            "19" | "STOP" | "SIGSTOP" => Some(19),
            s if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) => s.parse::<i32>().ok(),
            _ => None,
        };

        if let Some(signal) = sig {
            return (signal, remainder);
        }
    }
    (9, trimmed)
}

pub fn signal_name(sig: i32) -> &'static str {
    match sig {
        1 => "SIGHUP",
        2 => "SIGINT",
        3 => "SIGQUIT",
        6 => "SIGABRT",
        9 => "SIGKILL",
        15 => "SIGTERM",
        18 => "SIGCONT",
        19 => "SIGSTOP",
        _ => "SIGNAL",
    }
}

fn ordered_match(candidate: &str, query: &str) -> bool {
    let mut candidate_chars = candidate.chars().flat_map(char::to_lowercase);
    for query_char in query.chars().flat_map(char::to_lowercase) {
        if !candidate_chars.any(|c| c == query_char) {
            return false;
        }
    }
    true
}

fn filter_and_rank_processes(
    processes: &[ProcessEntry],
    query: &str,
    signal: i32,
) -> Vec<PluginResult> {
    let sig_name = signal_name(signal);

    if query.is_empty() {
        let mut sorted: Vec<_> = processes.iter().collect();
        sorted.sort_by(|a, b| {
            // Normal processes before kernel threads
            a.is_kernel
                .cmp(&b.is_kernel)
                // Current user's processes before others
                .then_with(|| b.is_current_user.cmp(&a.is_current_user))
                // Newer processes first (higher PID)
                .then_with(|| b.pid.cmp(&a.pid))
        });

        return sorted
            .into_iter()
            .take(100)
            .enumerate()
            .map(|(index, proc)| {
                let score = 10_000 - (index as i64 * 10).min(9_000);
                build_result(proc, signal, sig_name, score)
            })
            .collect();
    }

    let q = query.trim().to_lowercase();
    let mut scored: Vec<(&ProcessEntry, i64)> = Vec::new();

    for proc in processes {
        let pid_str = proc.pid.to_string();
        let comm_lower = proc.comm.to_lowercase();
        let cmd_lower = proc.cmdline.to_lowercase();
        let user_lower = proc.user.to_lowercase();

        let mut score: i64 = 0;
        if pid_str == q {
            score = 12_000;
        } else if comm_lower == q {
            score = 11_000;
        } else if comm_lower.starts_with(&q) {
            score = 9_500;
        } else if pid_str.starts_with(&q) {
            score = 9_000;
        } else if comm_lower.split_whitespace().any(|word| word == q) {
            score = 8_800;
        } else if comm_lower
            .split_whitespace()
            .any(|word| word.starts_with(&q))
        {
            score = 8_500;
        } else if comm_lower.contains(&q) {
            score = 8_000;
        } else if pid_str.contains(&q) {
            score = 7_500;
        } else if cmd_lower.contains(&q) {
            score = 7_000;
        } else if user_lower == q {
            score = 6_500;
        } else if user_lower.starts_with(&q) {
            score = 6_000;
        } else if ordered_match(&comm_lower, &q) {
            score = 5_500;
        } else if ordered_match(&cmd_lower, &q) {
            score = 4_500;
        }

        if score > 0 {
            if proc.is_current_user {
                score += 500;
            }
            if proc.is_kernel {
                score -= 2_000;
            }
            scored.push((proc, score));
        }
    }

    scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.pid.cmp(&a.0.pid)));

    if scored.is_empty() {
        return vec![PluginResult {
            id: String::new(),
            title: "No matching processes".into(),
            description: format!("No process found matching '{query}'"),
            score: 0,
            icon: None,
            image: None,
            metadata: Some("kill".into()),
        }];
    }

    scored
        .into_iter()
        .take(100)
        .map(|(proc, score)| build_result(proc, signal, sig_name, score))
        .collect()
}

fn build_result(proc: &ProcessEntry, signal: i32, sig_name: &str, score: i64) -> PluginResult {
    let display_cmd = if proc.cmdline.len() > 100 {
        let truncated = match proc.cmdline.char_indices().nth(100) {
            Some((idx, _)) => &proc.cmdline[..idx],
            None => &proc.cmdline,
        };
        format!("{truncated}…")
    } else {
        proc.cmdline.clone()
    };

    let title = format!("{}  (PID: {})", proc.comm, proc.pid);
    let description = format!(
        "kill -{signal} ({sig_name}) · {} · {display_cmd}",
        proc.user
    );

    let icon = if proc.is_kernel {
        None
    } else {
        Some(proc.comm.clone())
    };

    PluginResult {
        id: format!("{}:{}", proc.pid, signal),
        title,
        description,
        score,
        icon,
        image: None,
        metadata: Some("kill".into()),
    }
}

fn execute_kill(result_id: &str) -> PluginAction {
    if result_id.is_empty() {
        return PluginAction::None;
    }
    let (pid_str, sig_str) = result_id.split_once(':').unwrap_or((result_id, "9"));
    let pid: i32 = pid_str.parse().unwrap_or(0);
    let signal: i32 = sig_str.parse().unwrap_or(9);

    if pid > 1 {
        // SAFETY: kill is a POSIX syscall targeting valid positive PID > 1.
        let ret = unsafe { libc::kill(pid, signal) };
        if ret != 0 {
            let _ = std::process::Command::new("kill")
                .args([format!("-{signal}"), pid.to_string()])
                .status();
        }
    }

    PluginAction::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_signals_correctly() {
        assert_eq!(parse_signal_and_query(""), (9, ""));
        assert_eq!(parse_signal_and_query("firefox"), (9, "firefox"));
        assert_eq!(parse_signal_and_query("-9"), (9, ""));
        assert_eq!(parse_signal_and_query("-9 firefox"), (9, "firefox"));
        assert_eq!(parse_signal_and_query("-15 code"), (15, "code"));
        assert_eq!(parse_signal_and_query("-term code"), (15, "code"));
        assert_eq!(parse_signal_and_query("-kill node"), (9, "node"));
        assert_eq!(parse_signal_and_query("-HUP nginx"), (1, "nginx"));
        assert_eq!(parse_signal_and_query("-int bash"), (2, "bash"));
        assert_eq!(parse_signal_and_query("-STOP steam"), (19, "steam"));
        assert_eq!(parse_signal_and_query("-CONT steam"), (18, "steam"));
        assert_eq!(parse_signal_and_query("-bash"), (9, "-bash"));
    }

    #[test]
    fn filters_and_ranks_mock_processes() {
        let procs = vec![
            ProcessEntry {
                pid: 100,
                user: "root".into(),
                comm: "systemd".into(),
                cmdline: "/sbin/init".into(),
                is_kernel: false,
                is_current_user: false,
            },
            ProcessEntry {
                pid: 1234,
                user: "user".into(),
                comm: "firefox".into(),
                cmdline: "/usr/lib/firefox/firefox --new-window".into(),
                is_kernel: false,
                is_current_user: true,
            },
            ProcessEntry {
                pid: 5678,
                user: "user".into(),
                comm: "code".into(),
                cmdline: "/usr/lib/code/code --unity-launch".into(),
                is_kernel: false,
                is_current_user: true,
            },
            ProcessEntry {
                pid: 2,
                user: "root".into(),
                comm: "kthreadd".into(),
                cmdline: "[kthreadd]".into(),
                is_kernel: true,
                is_current_user: false,
            },
        ];

        // Empty query
        let results = filter_and_rank_processes(&procs, "", 9);
        assert_eq!(results.len(), 4);
        assert_eq!(results[0].id, "5678:9"); // Higher PID user proc first
        assert_eq!(results[1].id, "1234:9");
        assert_eq!(results[3].id, "2:9"); // Kernel proc last

        // Query by comm
        let results = filter_and_rank_processes(&procs, "fire", 9);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "1234:9");
        assert!(results[0].title.contains("firefox"));
        assert!(results[0].description.contains("kill -9 (SIGKILL)"));

        // Query by PID with custom signal
        let results = filter_and_rank_processes(&procs, "5678", 15);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "5678:15");
        assert!(results[0].title.contains("code"));
        assert!(results[0].description.contains("kill -15 (SIGTERM)"));

        // No match query
        let results = filter_and_rank_processes(&procs, "nonexistent_proc_xyz", 9);
        assert_eq!(results.len(), 1);
        assert!(results[0].id.is_empty());
        assert_eq!(results[0].title, "No matching processes");
    }

    #[test]
    fn abi_v2_export_contract() {
        let api = waymenu_plugin_v2();
        assert_eq!(api.abi_version, ABI_VERSION);

        let id = unsafe { CStr::from_ptr(api.id) }.to_str().unwrap();
        assert_eq!(id, "kill");

        let prefix = unsafe { CStr::from_ptr(api.prefix) }.to_str().unwrap();
        assert_eq!(prefix, "k ");

        let name = unsafe { CStr::from_ptr(api.name) }.to_str().unwrap();
        assert_eq!(name, "Kill Process");

        let state = unsafe { create() };
        assert!(!state.is_null());

        let query = std::ffi::CString::new("").unwrap();
        let ptr = unsafe { search(state, query.as_ptr()) };
        assert!(!ptr.is_null());

        let json = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap();
        let results: Vec<PluginResult> = serde_json::from_str(json).unwrap();
        assert!(!results.is_empty());

        unsafe { free_json(ptr) };

        // Test action on empty ID returns PluginAction::None
        let empty_id = std::ffi::CString::new("").unwrap();
        let act_ptr = unsafe { action(state, empty_id.as_ptr()) };
        assert!(!act_ptr.is_null());
        let act_json = unsafe { CStr::from_ptr(act_ptr) }.to_str().unwrap();
        let act: PluginAction = serde_json::from_str(act_json).unwrap();
        match act {
            PluginAction::None => {}
            other => panic!("expected PluginAction::None, got {other:?}"),
        }
        unsafe { free_json(act_ptr) };

        unsafe { destroy(state) };
    }
}
