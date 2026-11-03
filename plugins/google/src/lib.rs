use std::{
    ffi::{c_char, c_void, CStr},
    panic::{catch_unwind, AssertUnwindSafe},
};
use waymenu_plugin_api::{
    free_json, into_json, PluginAction, PluginApiV2, PluginResult, ABI_VERSION,
};

struct State;

#[unsafe(no_mangle)]
pub extern "C" fn waymenu_plugin_v2() -> PluginApiV2 {
    PluginApiV2 {
        abi_version: ABI_VERSION,
        id: c"google".as_ptr(),
        prefix: c"g ".as_ptr(),
        name: c"Google Search".as_ptr(),
        description: c"Search the web with Google.".as_ptr(),
        create: Some(create),
        destroy: Some(destroy),
        search: Some(search),
        action: Some(action),
        free_json: Some(free_json),
    }
}

unsafe extern "C" fn create() -> *mut c_void {
    catch_unwind(|| Box::into_raw(Box::new(State)).cast()).unwrap_or(std::ptr::null_mut())
}

unsafe extern "C" fn destroy(state: *mut c_void) {
    if !state.is_null() {
        drop(unsafe { Box::from_raw(state.cast::<State>()) });
    }
}

unsafe extern "C" fn search(state: *mut c_void, query: *const c_char) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() || query.is_null() {
            return into_json(&Vec::<PluginResult>::new());
        }
        let Ok(raw_query) = (unsafe { CStr::from_ptr(query) }).to_str() else {
            return into_json(&Vec::<PluginResult>::new());
        };
        let query = raw_query.trim();
        let (url, title, description) = if query.is_empty() {
            (
                "https://www.google.com".to_owned(),
                "Google Search".to_owned(),
                "https://www.google.com".to_owned(),
            )
        } else {
            let encoded = url_encode(query);
            let url = format!("https://www.google.com/search?q={encoded}");
            (url.clone(), format!("Search Google for \"{query}\""), url)
        };

        into_json(&vec![PluginResult {
            id: url,
            title,
            description,
            score: 10_000,
            icon: Some("web-browser".into()),
            image: None,
            metadata: Some("google".into()),
        }])
    }))
    .unwrap_or(std::ptr::null_mut())
}

unsafe extern "C" fn action(state: *mut c_void, result_id: *const c_char) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() || result_id.is_null() {
            return into_json(&PluginAction::None);
        }
        let Ok(url) = (unsafe { CStr::from_ptr(result_id) }).to_str() else {
            return into_json(&PluginAction::None);
        };
        into_json(&PluginAction::OpenUrl(url.to_owned()))
    }))
    .unwrap_or(std::ptr::null_mut())
}

fn url_encode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push('+'),
            _ => {
                use std::fmt::Write;
                let _ = write!(encoded, "%{byte:02X}");
            }
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn encodes_search_query_with_spaces_and_special_characters() {
        assert_eq!(url_encode("hyprland linux"), "hyprland+linux");
        assert_eq!(url_encode("c++ programming"), "c%2B%2B+programming");
        assert_eq!(url_encode("rust & wayland"), "rust+%26+wayland");
    }

    #[test]
    fn search_builds_google_url_from_query() {
        let state = unsafe { create() };
        assert!(!state.is_null());

        let query = CString::new("hyprland linux").unwrap();
        let ptr = unsafe { search(state, query.as_ptr()) };
        assert!(!ptr.is_null());

        let json = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap();
        let results: Vec<PluginResult> = serde_json::from_str(json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].id,
            "https://www.google.com/search?q=hyprland+linux"
        );
        assert_eq!(
            results[0].description,
            "https://www.google.com/search?q=hyprland+linux"
        );
        assert_eq!(results[0].title, "Search Google for \"hyprland linux\"");

        unsafe { free_json(ptr) };

        // Test action returns OpenUrl
        let id = CString::new(results[0].id.as_str()).unwrap();
        let action_ptr = unsafe { action(state, id.as_ptr()) };
        assert!(!action_ptr.is_null());
        let action_json = unsafe { CStr::from_ptr(action_ptr) }.to_str().unwrap();
        let action_res: PluginAction = serde_json::from_str(action_json).unwrap();
        match action_res {
            PluginAction::OpenUrl(url) => {
                assert_eq!(url, "https://www.google.com/search?q=hyprland+linux");
            }
            other => panic!("unexpected action: {other:?}"),
        }
        unsafe { free_json(action_ptr) };

        unsafe { destroy(state) };
    }

    #[test]
    fn search_empty_query_points_to_google_home() {
        let state = unsafe { create() };
        let query = CString::new("").unwrap();
        let ptr = unsafe { search(state, query.as_ptr()) };
        let json = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap();
        let results: Vec<PluginResult> = serde_json::from_str(json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "https://www.google.com");
        assert_eq!(results[0].title, "Google Search");

        unsafe { free_json(ptr) };
        unsafe { destroy(state) };
    }

    #[test]
    fn exports_expected_prefix() {
        let api = waymenu_plugin_v2();
        let prefix = unsafe { CStr::from_ptr(api.prefix) }.to_str().unwrap();
        assert_eq!(prefix, "g ");
    }
}
