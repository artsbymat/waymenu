use std::{
    ffi::{c_char, c_void, CStr},
    panic::{catch_unwind, AssertUnwindSafe},
    process::Command,
};

use waymenu_plugin_api::{
    free_json, into_json, PluginAction, PluginApiV2, PluginResult, ABI_VERSION,
};

const UNIT: &str = "waymenu-hyprland-caffeine.service";
const DURATIONS: [(&str, &str); 5] = [
    ("30", "30 minutes"),
    ("60", "60 minutes"),
    ("90", "90 minutes"),
    ("120", "120 minutes"),
    ("infinity", "Infinity"),
];

#[unsafe(no_mangle)]
pub extern "C" fn waymenu_plugin_v2() -> PluginApiV2 {
    PluginApiV2 {
        abi_version: ABI_VERSION,
        id: c"hyprland-caffeine".as_ptr(),
        prefix: c"caf ".as_ptr(),
        name: c"Hyprland Caffeine".as_ptr(),
        description: c"Keep the screen on for a set time or until turned off.".as_ptr(),
        create: Some(create),
        destroy: Some(destroy),
        search: Some(search),
        action: Some(action),
        free_json: Some(free_json),
    }
}

unsafe extern "C" fn create() -> *mut c_void {
    // The host needs a non-null token. Caffeine keeps its state in the systemd unit.
    std::ptr::NonNull::<()>::dangling().as_ptr().cast()
}

unsafe extern "C" fn destroy(_state: *mut c_void) {}

unsafe extern "C" fn search(_state: *mut c_void, query: *const c_char) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        if query.is_null() {
            return into_json(&Vec::<PluginResult>::new());
        }
        // SAFETY: The host provides a NUL-terminated query for this callback.
        let Ok(query) = unsafe { CStr::from_ptr(query) }.to_str() else {
            return into_json(&Vec::<PluginResult>::new());
        };
        let results = match is_active() {
            Ok(active) => choices(active, query),
            Err(error) => vec![row("", "Could not check Caffeine status", &error)],
        };
        into_json(&results)
    }))
    .unwrap_or(std::ptr::null_mut())
}

unsafe extern "C" fn action(_state: *mut c_void, id: *const c_char) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        if !id.is_null() {
            // SAFETY: The host provides a NUL-terminated result ID for this callback.
            if let Ok(id) = unsafe { CStr::from_ptr(id) }.to_str() {
                if let Err(error) = apply_choice(id) {
                    eprintln!("waymenu-hyprland-caffeine: {error}");
                }
            }
        }
        into_json(&PluginAction::None)
    }))
    .unwrap_or(std::ptr::null_mut())
}

fn choices(active: bool, query: &str) -> Vec<PluginResult> {
    let query = query.trim().to_ascii_lowercase();
    if !active {
        if let Some((amount, unit)) = parse_duration(&query) {
            let unit_name = match unit {
                's' => "second",
                'h' => "hour",
                'd' => "day",
                _ => "minute",
            };
            let plural = if amount == 1 { "" } else { "s" };
            return vec![row(
                &query,
                &format!("{amount} {unit_name}{plural}"),
                "Keep the screen on for this duration",
            )];
        }
        if query.starts_with(|character: char| character.is_ascii_digit()) {
            return vec![row(
                "",
                "Invalid duration",
                "Enter a positive whole number, such as 45m or 2h",
            )];
        }
    }
    let available: Vec<_> = if active {
        vec![
            (
                "continue",
                "Continue",
                "Keep the current Caffeine session active",
            ),
            ("off", "Off", "Release the idle inhibitor"),
        ]
    } else {
        DURATIONS
            .iter()
            .map(|&(id, title)| {
                let description = if id == "infinity" {
                    "Keep the screen on until you select Off"
                } else {
                    "Keep the screen on for this duration"
                };
                (id, title, description)
            })
            .collect()
    };
    available
        .into_iter()
        .filter(|&(id, title, _)| {
            id.contains(&query) || title.to_ascii_lowercase().contains(&query)
        })
        .map(|(id, title, description)| row(id, title, description))
        .collect()
}

fn parse_duration(input: &str) -> Option<(u32, char)> {
    let (digits, unit) = match input.chars().next_back()? {
        unit @ ('s' | 'm' | 'h' | 'd') => (&input[..input.len() - 1], unit),
        _ => (input, 'm'),
    };
    if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let amount: u32 = digits.parse().ok()?;
    (amount > 0).then_some((amount, unit))
}

fn row(id: &str, title: &str, description: &str) -> PluginResult {
    PluginResult {
        id: id.into(),
        title: title.into(),
        description: description.into(),
        score: 0,
        icon: None,
        image: None,
        metadata: None,
    }
}

fn is_active() -> Result<bool, String> {
    let state = run(
        "systemctl",
        &["--user", "show", "--property=ActiveState", "--value", UNIT],
    )?;
    match state.as_str() {
        "active" | "activating" | "deactivating" | "reloading" | "refreshing" => Ok(true),
        "inactive" | "failed" => Ok(false),
        _ => Err(format!("Unexpected systemd unit state: {state:?}")),
    }
}

fn apply_choice(id: &str) -> Result<(), String> {
    if id == "off" {
        if let Err(error) = run("systemctl", &["--user", "stop", UNIT]) {
            if is_active() != Ok(false) {
                return Err(error);
            }
        }
        return Ok(());
    }
    let duration = if id == "infinity" {
        id.to_owned()
    } else if let Some((amount, unit)) = parse_duration(id) {
        format!("{amount}{unit}")
    } else {
        return Ok(());
    };
    run(
        "systemd-run",
        &[
            "--user",
            "--unit",
            UNIT,
            "--collect",
            "--quiet",
            "--property=Type=exec",
            "--",
            "systemd-inhibit",
            "--no-ask-password",
            "--what=idle",
            "--mode=block",
            "--who=waymenu-hyprland-caffeine",
            "--why=Keep the screen on",
            "sleep",
            &duration,
        ],
    )?;
    Ok(())
}

fn run(program: &str, arguments: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(arguments)
        .output()
        .map_err(|error| format!("Could not run {program}: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        let error = String::from_utf8_lossy(&output.stderr);
        Err(format!("{program}: {}", error.trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(active: bool, query: &str) -> Vec<String> {
        choices(active, query)
            .into_iter()
            .map(|row| row.id)
            .collect()
    }

    #[test]
    fn shows_durations_when_off_and_controls_when_active() {
        assert_eq!(ids(false, ""), ["30", "60", "90", "120", "infinity"]);
        assert_eq!(ids(false, "90"), ["90"]);
        assert_eq!(ids(false, "off"), Vec::<String>::new());
        assert_eq!(ids(true, ""), ["continue", "off"]);
        assert_eq!(ids(true, "OFF"), ["off"]);
        assert_eq!(ids(true, "120"), Vec::<String>::new());
    }

    #[test]
    fn continue_does_not_need_a_service_command() {
        assert!(apply_choice("continue").is_ok());
        assert!(apply_choice("unknown").is_ok());
    }

    #[test]
    fn accepts_custom_durations_only_while_inactive() {
        for (input, amount, suffix, title) in [
            ("45", 45, 'm', "45 minutes"),
            ("90s", 90, 's', "90 seconds"),
            ("2h", 2, 'h', "2 hours"),
            ("1d", 1, 'd', "1 day"),
        ] {
            assert_eq!(parse_duration(input), Some((amount, suffix)));
            let rows = choices(false, input);
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].id, input);
            assert_eq!(rows[0].title, title);
            assert!(choices(true, input).is_empty());
        }
        assert_eq!(ids(false, " 45M "), ["45m"]);
        for invalid in ["0", "0m", "+45m", "-1m", "1.5h", "45x", "4294967296m"] {
            assert!(parse_duration(invalid).is_none());
            assert!(choices(false, invalid)
                .iter()
                .all(|result| result.id.is_empty()));
        }
    }
}
