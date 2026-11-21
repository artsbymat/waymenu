mod history;

use super::{
    data_dir, BROWSER_EXECUTABLE, BROWSER_NAME, PLUGIN_DESCRIPTION, PLUGIN_ID, PLUGIN_NAME,
};

use std::{
    env,
    ffi::{c_char, c_void, CStr},
    panic::{catch_unwind, AssertUnwindSafe},
    path::PathBuf,
    process::Command,
};

use history::History;
use waymenu_plugin_api::{
    free_json, into_json, PluginAction, PluginApiV2, PluginResult, ABI_VERSION,
};

pub(super) fn absolute_env_dir(key: &str) -> Option<PathBuf> {
    env::var_os(key)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

pub(super) fn config_dir() -> Option<PathBuf> {
    absolute_env_dir("XDG_CONFIG_HOME")
        .or_else(|| absolute_env_dir("HOME").map(|home| home.join(".config")))
}

fn open_command(history: &History, id: &str) -> Option<Command> {
    let (profile, url) = id.split_once(':')?;
    let profile = history.profile_name(profile.parse().ok()?)?;
    let address = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    if address.is_empty() || address.chars().any(char::is_control) {
        return None;
    }

    let mut data_arg = std::ffi::OsString::from("--user-data-dir=");
    data_arg.push(history.root());
    let mut command = Command::new(BROWSER_EXECUTABLE);
    command
        .arg(data_arg)
        .arg(format!("--profile-directory={profile}"))
        .arg("--")
        .arg(url);
    Some(command)
}

#[unsafe(no_mangle)]
pub extern "C" fn waymenu_plugin_v2() -> PluginApiV2 {
    PluginApiV2 {
        abi_version: ABI_VERSION,
        id: PLUGIN_ID.as_ptr(),
        prefix: c"h ".as_ptr(),
        name: PLUGIN_NAME.as_ptr(),
        description: PLUGIN_DESCRIPTION.as_ptr(),
        create: Some(create),
        destroy: Some(destroy),
        search: Some(search),
        action: Some(action),
        free_json: Some(free_json),
    }
}

unsafe extern "C" fn create() -> *mut c_void {
    catch_unwind(|| {
        let state = data_dir()
            .ok_or_else(|| format!("{BROWSER_NAME} data directory is unknown"))
            .and_then(|root| History::load(root).map_err(|error| error.to_string()));
        Box::into_raw(Box::new(state)).cast()
    })
    .unwrap_or(std::ptr::null_mut())
}

type State = Result<History, String>;

unsafe extern "C" fn destroy(state: *mut c_void) {
    if !state.is_null() {
        // SAFETY: The host returns the pointer from create exactly once.
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
        let results = match state {
            Ok(history) => history.search(query.to_str().unwrap_or_default()),
            Err(error) => vec![history::notice(
                &format!("{BROWSER_NAME} history unavailable"),
                error.clone(),
            )],
        };
        into_json(&results)
    }))
    .unwrap_or(std::ptr::null_mut())
}

unsafe extern "C" fn action(state: *mut c_void, id: *const c_char) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        if !state.is_null() && !id.is_null() {
            // SAFETY: The host supplies a live plugin state and a NUL-terminated result ID.
            let (state, id) = unsafe { (&*state.cast::<State>(), CStr::from_ptr(id)) };
            if let Ok(history) = state {
                if let Some(mut command) = open_command(history, id.to_str().unwrap_or_default()) {
                    if let Err(error) = command.spawn() {
                        eprintln!(
                            "{}: could not start {BROWSER_EXECUTABLE}: {error}",
                            PLUGIN_ID.to_string_lossy()
                        );
                    }
                }
            }
        }
        into_json(&PluginAction::None)
    }))
    .unwrap_or(std::ptr::null_mut())
}
