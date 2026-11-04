use std::{
    ffi::{c_char, c_void, CStr},
    panic::{catch_unwind, AssertUnwindSafe},
    time::{Duration, Instant},
};
use waymenu_plugin_api::{
    free_json, into_json, PluginAction, PluginApiV2, PluginResult, ABI_VERSION,
};

mod package;
mod terminal;

use package::PackageEntry;

struct State {
    packages: Vec<PackageEntry>,
    last_refresh: Instant,
}

impl State {
    fn refresh_if_needed(&mut self) {
        if self.packages.is_empty() || self.last_refresh.elapsed() > Duration::from_secs(60) {
            self.packages = package::discover_packages();
            self.last_refresh = Instant::now();
        }
    }

    fn search(&mut self, query: &str) -> Vec<PluginResult> {
        self.refresh_if_needed();
        package::filter_and_rank_packages(&self.packages, query)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn waymenu_plugin_v2() -> PluginApiV2 {
    PluginApiV2 {
        abi_version: ABI_VERSION,
        id: c"pacman".as_ptr(),
        prefix: c"pac ".as_ptr(),
        name: c"Pacman".as_ptr(),
        description: c"Search, install, and manage Pacman packages.".as_ptr(),
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
            packages: Vec::new(),
            last_refresh: Instant::now()
                .checked_sub(Duration::from_secs(120))
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
        // SAFETY: State pointer is managed by the host and valid during search().
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
        into_json(&execute_pacman_action(id_str))
    }))
    .unwrap_or(std::ptr::null_mut())
}

fn execute_pacman_action(id: &str) -> PluginAction {
    if id.is_empty() || id == "help" {
        return PluginAction::None;
    }

    if id == "sysupgrade" {
        return terminal::run_in_terminal(&terminal::upgrade_command());
    }

    if id == "cleancache" {
        return terminal::run_in_terminal("sudo pacman -Sc");
    }

    if id == "orphans" {
        let script = "sh -c 'orphans=$(pacman -Qtdq); if [ -n \"$orphans\" ]; then echo \"Found orphan packages:\"; echo \"$orphans\"; sudo pacman -Rns $orphans; else echo \"No orphan packages found.\"; fi'";
        return terminal::run_in_terminal(script);
    }

    if let Some(pkg) = id.strip_prefix("install:") {
        let cmd = format!("{} -- {}", terminal::install_command_prefix(), pkg);
        return terminal::run_in_terminal(&cmd);
    }

    if let Some(pkg) = id.strip_prefix("remove:") {
        let cmd = format!("{} -- {}", terminal::remove_command_prefix(), pkg);
        return terminal::run_in_terminal(&cmd);
    }

    if let Some(pkg) = id.strip_prefix("info:") {
        let cmd = format!("pacman -Qi -- {pkg} || pacman -Si -- {pkg}");
        return terminal::run_in_terminal(&cmd);
    }

    // Default fallback: attempt to install the given ID
    let cmd = format!("{} -- {}", terminal::install_command_prefix(), id);
    terminal::run_in_terminal(&cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_v2_export_contract() {
        let api = waymenu_plugin_v2();
        assert_eq!(api.abi_version, ABI_VERSION);
        assert!(!api.id.is_null());
        assert!(!api.prefix.is_null());
        assert!(!api.name.is_null());
        assert!(!api.description.is_null());

        let id = unsafe { CStr::from_ptr(api.id) }.to_str().unwrap();
        let prefix = unsafe { CStr::from_ptr(api.prefix) }.to_str().unwrap();
        assert_eq!(id, "pacman");
        assert_eq!(prefix, "pac ");

        assert!(api.create.is_some());
        assert!(api.destroy.is_some());
        assert!(api.search.is_some());
        assert!(api.action.is_some());
        assert!(api.free_json.is_some());
    }

    #[test]
    fn executes_pacman_actions() {
        let action = execute_pacman_action("help");
        assert!(matches!(action, PluginAction::None));

        let action = execute_pacman_action("install:ripgrep");
        match action {
            PluginAction::RunShell(cmd) => {
                assert!(cmd.contains("sudo pacman -S -- ripgrep") || cmd.contains("ripgrep"));
            }
            PluginAction::CopyText(cmd) => {
                assert!(cmd.contains("sudo pacman -S -- ripgrep") || cmd.contains("ripgrep"));
            }
            _ => panic!("Expected RunShell or CopyText"),
        }

        let action = execute_pacman_action("remove:ripgrep");
        match action {
            PluginAction::RunShell(cmd) => {
                assert!(cmd.contains("sudo pacman -Rns -- ripgrep") || cmd.contains("ripgrep"));
            }
            PluginAction::CopyText(cmd) => {
                assert!(cmd.contains("sudo pacman -Rns -- ripgrep") || cmd.contains("ripgrep"));
            }
            _ => panic!("Expected RunShell or CopyText"),
        }
    }
}
