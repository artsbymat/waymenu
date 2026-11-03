use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn dmenu_headless_keeps_stdout_clean() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_waymenu"))
        .args(["--dmenu", "--query", "ban"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"Apple\nBanana\nOrange\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "Banana\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn help_and_version_do_not_need_wayland() {
    for argument in ["--help", "--version"] {
        let output = Command::new(env!("CARGO_BIN_EXE_waymenu"))
            .arg(argument)
            .env_remove("WAYLAND_DISPLAY")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(!output.stdout.is_empty());
    }
}

#[test]
fn dmenu_headless_honors_case_and_raw_query_options() {
    let run = |args: &[&str], input: &[u8]| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_waymenu"))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.as_mut().unwrap().write_all(input).unwrap();
        child.wait_with_output().unwrap()
    };
    let sensitive = run(
        &[
            "--dmenu",
            "--query",
            "banana",
            "--case-sensitive",
            "--no-fuzzy",
            "--print-query",
        ],
        b"Banana\n",
    );
    assert_eq!(String::from_utf8(sensitive.stdout).unwrap(), "banana\n");
    let insensitive = run(&["--dmenu", "--query", "banana", "--no-fuzzy"], b"Banana\n");
    assert_eq!(String::from_utf8(insensitive.stdout).unwrap(), "Banana\n");
}

#[test]
fn password_dmenu_never_writes_input_or_selection_to_stdout() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_waymenu"))
        .args([
            "--dmenu",
            "--query",
            "secret",
            "--password",
            "--print-query",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"secret-entry\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn missing_option_values_fail_with_diagnostic() {
    for argument in ["--query", "--mode"] {
        let output = Command::new(env!("CARGO_BIN_EXE_waymenu"))
            .arg(argument)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("requires a value"));
    }
}
