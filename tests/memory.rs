#![cfg(target_os = "linux")]

use std::{
    io::Write,
    mem::MaybeUninit,
    process::{Command, Stdio},
};

fn wait_with_peak_rss(mut child: std::process::Child) -> (i32, i64) {
    let pid = child.id() as libc::pid_t;
    let mut status = 0;
    let mut usage = MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: pid names the direct child created above. Both pointers reference valid,
    // writable stack storage for the duration of wait4, and the child is reaped exactly once.
    let waited = unsafe { libc::wait4(pid, &mut status, 0, usage.as_mut_ptr()) };
    assert_eq!(waited, pid);
    // SAFETY: a successful wait4 initialized the complete rusage structure.
    let usage = unsafe { usage.assume_init() };
    // Child::drop does not wait or kill; stdin was already closed by each caller.
    let _ = child.stdin.take();
    (status, usage.ru_maxrss)
}

#[test]
fn application_query_stays_below_peak_memory_target() {
    let child = Command::new(env!("CARGO_BIN_EXE_waymenu"))
        .args(["--query", "firefox"])
        .env(
            "XDG_CONFIG_HOME",
            std::env::temp_dir().join("waymenu-memory-config"),
        )
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let (status, peak_kib) = wait_with_peak_rss(child);
    eprintln!("application_query_peak_rss_kib={peak_kib}");
    assert!(libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0);
    assert!(
        peak_kib < 50 * 1024,
        "application query peak RSS was {peak_kib} KiB"
    );
}

#[test]
fn large_dmenu_input_stays_below_peak_memory_target() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_waymenu"))
        .args(["--dmenu", "--query", "item-99999"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        for index in 0..100_000 {
            writeln!(stdin, "item-{index}").unwrap();
        }
    }
    drop(child.stdin.take());
    let (status, peak_kib) = wait_with_peak_rss(child);
    eprintln!("large_dmenu_peak_rss_kib={peak_kib}");
    assert!(libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0);
    assert!(
        peak_kib < 50 * 1024,
        "large dmenu peak RSS was {peak_kib} KiB"
    );
}
