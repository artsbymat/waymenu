use std::{collections::HashMap, fs, os::unix::fs::MetadataExt, path::Path};

#[derive(Clone, Debug, PartialEq)]
pub struct ProcessEntry {
    pub pid: u32,
    pub user: String,
    pub comm: String,
    pub cmdline: String,
    pub is_kernel: bool,
    pub is_current_user: bool,
}

pub fn list_processes() -> Vec<ProcessEntry> {
    let my_pid = std::process::id();
    // SAFETY: getuid is a POSIX syscall that returns the current real UID.
    let my_uid = unsafe { libc::getuid() };

    let mut processes = list_from_proc(my_pid, my_uid);
    if processes.is_empty() {
        processes = list_from_ps(my_pid, my_uid);
    }
    processes
}

fn list_from_proc(my_pid: u32, my_uid: u32) -> Vec<ProcessEntry> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };

    let mut user_cache = HashMap::new();
    let mut processes = Vec::new();

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }

        let file_name = entry.file_name();
        let Some(name_str) = file_name.to_str() else {
            continue;
        };
        let Ok(pid) = name_str.parse::<u32>() else {
            continue;
        };
        if pid == 0 || pid == my_pid {
            continue;
        }

        let path = entry.path();
        let (uid, is_current_user) = match entry.metadata() {
            Ok(meta) => {
                let uid = meta.uid();
                (uid, uid == my_uid)
            }
            Err(_) => (0, false),
        };

        let comm = read_proc_comm(&path).unwrap_or_else(|| name_str.to_string());
        let (cmdline, is_kernel) = match read_proc_cmdline(&path) {
            Some(cmd) if !cmd.is_empty() => (cmd, false),
            _ => (format!("[{comm}]"), true),
        };

        let user = resolve_username(uid, &mut user_cache);

        processes.push(ProcessEntry {
            pid,
            user,
            comm,
            cmdline,
            is_kernel,
            is_current_user,
        });
    }

    processes
}

fn read_proc_comm(proc_path: &Path) -> Option<String> {
    let content = fs::read_to_string(proc_path.join("comm")).ok()?;
    let trimmed = content.trim_end_matches(['\n', '\r']);
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn read_proc_cmdline(proc_path: &Path) -> Option<String> {
    let bytes = fs::read(proc_path.join("cmdline")).ok()?;
    if bytes.is_empty() {
        return None;
    }
    let s = bytes
        .split(|&b| b == 0)
        .filter(|part| !part.is_empty())
        .map(String::from_utf8_lossy)
        .collect::<Vec<_>>()
        .join(" ");
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn resolve_username(uid: u32, cache: &mut HashMap<u32, String>) -> String {
    if let Some(cached) = cache.get(&uid) {
        return cached.clone();
    }
    let name = if uid == 0 {
        "root".to_string()
    } else {
        let mut pwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut result = std::ptr::null_mut();
        let mut buf = vec![0; 1024];
        // SAFETY: getpwuid_r is POSIX thread-safe user lookup into caller-supplied buffer.
        let ret = unsafe {
            libc::getpwuid_r(
                uid,
                pwd.as_mut_ptr(),
                buf.as_mut_ptr(),
                buf.len(),
                &mut result,
            )
        };
        if ret == 0 && !result.is_null() {
            // SAFETY: pwd initialized successfully by getpwuid_r.
            let pw = unsafe { pwd.assume_init() };
            if !pw.pw_name.is_null() {
                // SAFETY: pw.pw_name points to NUL-terminated username in buf.
                unsafe { std::ffi::CStr::from_ptr(pw.pw_name) }
                    .to_string_lossy()
                    .into_owned()
            } else {
                uid.to_string()
            }
        } else {
            uid.to_string()
        }
    };
    cache.insert(uid, name.clone());
    name
}

fn list_from_ps(my_pid: u32, my_uid: u32) -> Vec<ProcessEntry> {
    let output = match std::process::Command::new("ps")
        .args(["-eo", "pid=,user=,comm=,args="])
        .output()
    {
        Ok(out) if out.status.success() => out.stdout,
        _ => return Vec::new(),
    };
    let text = String::from_utf8_lossy(&output);
    let my_user = std::env::var("USER").unwrap_or_else(|_| {
        let mut cache = HashMap::new();
        resolve_username(my_uid, &mut cache)
    });

    let mut processes = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        let Ok(pid) = parts[0].parse::<u32>() else {
            continue;
        };
        if pid == 0 || pid == my_pid {
            continue;
        }
        let user = parts[1];
        let comm = parts[2];
        let args = if parts.len() > 3 {
            parts[3..].join(" ")
        } else {
            comm.to_string()
        };
        let is_kernel = args.starts_with('[') && args.ends_with(']');
        let is_current_user = !my_user.is_empty() && user == my_user;
        processes.push(ProcessEntry {
            pid,
            user: user.to_string(),
            comm: comm.to_string(),
            cmdline: args,
            is_kernel,
            is_current_user,
        });
    }
    processes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_at_least_one_process() {
        let list = list_processes();
        assert!(!list.is_empty(), "expected to find running processes");
        for proc in &list {
            assert!(proc.pid > 0);
            assert!(!proc.comm.is_empty());
        }
    }
}
