mod languages;

use std::{
    ffi::{c_char, c_void, CStr},
    fmt::Write,
    panic::{catch_unwind, AssertUnwindSafe},
};

use waymenu_plugin_api::{
    free_json, into_json, PluginAction, PluginApiV2, PluginResult, ABI_VERSION,
    AUTOCOMPLETE_METADATA_PREFIX,
};

const URL_PREFIX: &str = "https://translate.google.com/?sl=auto&tl=";
const DEFAULT_TARGET: &str = "id";

#[unsafe(no_mangle)]
pub extern "C" fn waymenu_plugin_v2() -> PluginApiV2 {
    PluginApiV2 {
        abi_version: ABI_VERSION,
        id: c"google-translate".as_ptr(),
        prefix: c"tr ".as_ptr(),
        name: c"Google Translate".as_ptr(),
        description: c"Open Google Translate with text and a target language.".as_ptr(),
        create: Some(create),
        destroy: Some(destroy),
        search: Some(search),
        action: Some(action),
        free_json: Some(free_json),
    }
}

unsafe extern "C" fn create() -> *mut c_void {
    // The host requires a non-null token. This plugin has no mutable state.
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
        into_json(&translation_results(query))
    }))
    .unwrap_or(std::ptr::null_mut())
}

unsafe extern "C" fn action(_state: *mut c_void, id: *const c_char) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        if !id.is_null() {
            // SAFETY: The host provides a NUL-terminated result ID for this callback.
            if let Ok(id) = unsafe { CStr::from_ptr(id) }.to_str() {
                return into_json(&selected_action(id));
            }
        }
        into_json(&PluginAction::None)
    }))
    .unwrap_or(std::ptr::null_mut())
}

fn selected_action(id: &str) -> PluginAction {
    if id.starts_with(URL_PREFIX) && !id.chars().any(|ch| ch.is_control() || ch.is_whitespace()) {
        PluginAction::OpenUrl(id.to_owned())
    } else {
        PluginAction::None
    }
}

fn translation_results(input: &str) -> Vec<PluginResult> {
    let input = input.trim_start();
    let (target, text) = if let Some(override_input) = input.strip_prefix('@') {
        match override_input.split_once(char::is_whitespace) {
            Some((target, text)) if is_language_code(target) => (target, text.trim()),
            _ => {
                let suggestions = languages::suggestions(override_input.trim());
                return if suggestions.is_empty() {
                    vec![row(
                        input,
                        "No matching language",
                        "Try another name or enter a language code followed by text",
                    )]
                } else {
                    suggestions
                };
            }
        }
    } else {
        (DEFAULT_TARGET, input.trim_end())
    };

    let target_label = languages::name(target)
        .map(|name| format!("{name} (@{target})"))
        .unwrap_or_else(|| format!("@{target}"));
    if text.is_empty() {
        let description = if input.is_empty() {
            "Type @ to browse target languages".into()
        } else {
            format!("Target {target_label}")
        };
        return vec![row(input, "Enter text to translate", &description)];
    }

    let mut url = format!("{URL_PREFIX}{target}&text=");
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            url.push(char::from(byte));
        } else {
            write!(url, "%{byte:02X}").expect("writing to a String succeeds");
        }
    }
    url.push_str("&op=translate");

    vec![PluginResult {
        id: url,
        ..row(
            input,
            "Open Google Translate",
            &format!("Detect source language · Target {target_label}"),
        )
    }]
}

fn is_language_code(code: &str) -> bool {
    let mut subtags = code.split('-');
    let primary = subtags.next().unwrap_or_default();
    (2..=3).contains(&primary.len())
        && primary.bytes().all(|byte| byte.is_ascii_alphabetic())
        && subtags.all(|subtag| {
            (2..=8).contains(&subtag.len())
                && subtag.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
}

fn row(completion: &str, title: &str, description: &str) -> PluginResult {
    PluginResult {
        id: String::new(),
        title: title.into(),
        description: description.into(),
        score: 0,
        icon: None,
        image: None,
        metadata: Some(format!("{AUTOCOMPLETE_METADATA_PREFIX}{completion}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_translation_url_and_escapes_text() {
        let default = &translation_results(" good morning ")[0];
        assert_eq!(
            default.id,
            "https://translate.google.com/?sl=auto&tl=id&text=good%20morning&op=translate"
        );
        let override_row = &translation_results("@zh-CN café + 水&tl=en#test")[0];
        assert_eq!(override_row.id, "https://translate.google.com/?sl=auto&tl=zh-CN&text=caf%C3%A9%20%2B%20%E6%B0%B4%26tl%3Den%23test&op=translate");
        assert!(
            matches!(selected_action(&override_row.id), PluginAction::OpenUrl(url) if url == override_row.id)
        );
    }

    #[test]
    fn placeholders_and_invalid_targets_have_no_open_action() {
        for input in ["", "@en", "@en&sl=ja hello", "@en- hello"] {
            let row = &translation_results(input)[0];
            assert!(row.id.is_empty(), "{input}");
            assert!(matches!(selected_action(&row.id), PluginAction::None));
        }
        assert!(matches!(
            selected_action("https://example.org"),
            PluginAction::None
        ));
    }

    #[test]
    fn suggests_languages_and_completes_to_a_ready_to_type_query() {
        assert!(translation_results("@").len() > 10);
        for (input, title, completion) in [
            ("@jep", "Japanese", "@ja "),
            ("@English", "English", "@en "),
            ("@inggris", "English", "@en "),
            ("@Chinese traditional", "Chinese (Traditional)", "@zh-TW "),
            ("@ZH-cn", "Chinese (Simplified)", "@zh-CN "),
        ] {
            let result = &translation_results(input)[0];
            assert_eq!(result.title, title);
            assert_eq!(
                result.metadata.as_deref().unwrap(),
                format!("autocomplete:{completion}")
            );
            assert!(matches!(selected_action(&result.id), PluginAction::None));
            let prompt = &translation_results(completion)[0];
            assert_eq!(prompt.title, "Enter text to translate");
            assert!(prompt.description.contains(title));
        }
        assert_eq!(
            translation_results("@zu good morning")[0].id,
            "https://translate.google.com/?sl=auto&tl=zu&text=good%20morning&op=translate"
        );
    }
}
