//! Small C ABI surface for optional Waymenu shared-library plugins.
//!
//! Plugins and the host exchange JSON over owned C strings so Rust layouts and allocators
//! never cross the shared-library boundary.

use serde::{Deserialize, Serialize};
use std::ffi::{c_char, c_void, CString};

pub const ABI_VERSION: u32 = 2;
pub const AUTOCOMPLETE_METADATA_PREFIX: &str = "autocomplete:";
pub const PENDING_RESULT_METADATA: &str = "plugin-pending";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginResult {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub score: i64,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub metadata: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", content = "value", rename_all = "snake_case")]
pub enum PluginAction {
    LaunchApplication(String),
    LaunchBinary(String),
    RunShell(String),
    CopyText(String),
    OpenUrl(String),
    None,
}

/// Function table returned from a plugin's `waymenu_plugin_v2` entry point.
///
/// All strings are UTF-8 NUL-terminated strings. JSON result pointers are allocated by the
/// plugin and must be released using that plugin's `free_json` callback. Plugin code is
/// trusted native code and executes inside the Waymenu process.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PluginApiV2 {
    pub abi_version: u32,
    pub id: *const c_char,
    pub prefix: *const c_char,
    pub name: *const c_char,
    pub description: *const c_char,
    pub create: Option<unsafe extern "C" fn() -> *mut c_void>,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub search: Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_char>,
    pub action: Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_char>,
    pub free_json: Option<unsafe extern "C" fn(*mut c_char)>,
}

/// Convert a serializable value to a plugin-owned JSON C string.
pub fn into_json<T: Serialize>(value: &T) -> *mut c_char {
    let Ok(json) = serde_json::to_string(value) else {
        return std::ptr::null_mut();
    };
    let Ok(json) = CString::new(json) else {
        return std::ptr::null_mut();
    };
    json.into_raw()
}

/// Release a string returned by [`into_json`].
///
/// # Safety
/// `value` must be null or a pointer previously returned by [`into_json`] in this library.
pub unsafe extern "C" fn free_json(value: *mut c_char) {
    if !value.is_null() {
        // SAFETY: Guaranteed by this function's API contract.
        drop(unsafe { CString::from_raw(value) });
    }
}
