mod currency;
mod history;

use rink_core::{output::QueryError, Context};
use std::{
    ffi::{c_char, c_void, CStr},
    panic::{catch_unwind, AssertUnwindSafe},
    task::Poll,
};
use waymenu_plugin_api::{
    free_json, into_json, PluginAction, PluginApiV2, PluginResult, ABI_VERSION,
    AUTOCOMPLETE_METADATA_PREFIX, PENDING_RESULT_METADATA,
};

use history::{Entry, History};

const HISTORY_ID_PREFIX: &str = "history:";

struct State {
    context: Context,
    rates: currency::Rates,
    history: History,
    current: Option<Entry>,
}

fn currency_status(title: &str, description: impl Into<String>, pending: bool) -> PluginResult {
    PluginResult {
        id: String::new(),
        title: title.into(),
        description: description.into(),
        score: 10_000,
        icon: Some("accessories-calculator".into()),
        image: None,
        metadata: pending.then(|| PENDING_RESULT_METADATA.into()),
    }
}

fn new_context() -> Option<Context> {
    let mut context = rink_core::simple_context().ok()?;
    if let Some(currency_definitions) = rink_core::CURRENCY_FILE {
        if let Err(error) = context.load_currency(None, currency_definitions) {
            eprintln!("waymenu-rink: could not load bundled currency definitions: {error}");
        }
    }
    Some(context)
}

#[unsafe(no_mangle)]
pub extern "C" fn waymenu_plugin_v2() -> PluginApiV2 {
    PluginApiV2 {
        abi_version: ABI_VERSION,
        id: c"rink".as_ptr(),
        prefix: c"= ".as_ptr(),
        name: c"Rink Calculator".as_ptr(),
        description: c"Calculator and unit conversion powered by Rink.".as_ptr(),
        create: Some(create),
        destroy: Some(destroy),
        search: Some(search),
        action: Some(action),
        free_json: Some(free_json),
    }
}

unsafe extern "C" fn create() -> *mut c_void {
    catch_unwind(|| match new_context() {
        Some(context) => Box::into_raw(Box::new(State {
            context,
            rates: currency::Rates::new(),
            history: History::load(History::state_path()),
            current: None,
        }))
        .cast(),
        None => {
            eprintln!("waymenu-rink: could not initialize Rink context");
            std::ptr::null_mut()
        }
    })
    .unwrap_or(std::ptr::null_mut())
}

unsafe extern "C" fn destroy(state: *mut c_void) {
    if !state.is_null() {
        // SAFETY: The host calls destroy once for a non-null pointer returned by create.
        drop(unsafe { Box::from_raw(state.cast::<State>()) });
    }
}

unsafe extern "C" fn search(state: *mut c_void, query: *const c_char) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() || query.is_null() {
            return into_json(&Vec::<PluginResult>::new());
        }
        // SAFETY: The host passes the state created by this plugin and a NUL-terminated query.
        let state = unsafe { &mut *state.cast::<State>() };
        // SAFETY: The host's query pointer is valid for the duration of this callback.
        let Ok(query) = unsafe { CStr::from_ptr(query) }.to_str() else {
            return into_json(&Vec::<PluginResult>::new());
        };
        let query = query.trim();
        state.current = None;
        if query.is_empty() {
            let results: Vec<_> = state
                .history
                .entries
                .iter()
                .enumerate()
                .map(|(index, entry)| PluginResult {
                    id: format!("{HISTORY_ID_PREFIX}{}", entry.expression),
                    title: entry.expression.clone(),
                    description: entry.answer.clone(),
                    score: -(index as i64),
                    icon: Some("accessories-calculator".into()),
                    image: None,
                    metadata: Some(format!(
                        "{AUTOCOMPLETE_METADATA_PREFIX}{}",
                        entry.expression
                    )),
                })
                .collect();
            return into_json(&results);
        }
        let mut answer = rink_core::eval(&mut state.context, query);
        if matches!(answer, Err(QueryError::MissingDeps(_))) {
            match state.rates.poll(&mut state.context) {
                Poll::Pending => {
                    return into_json(&[currency_status(
                        "Fetching exchange rates…",
                        "Downloading from rinkcalc.app",
                        true,
                    )])
                }
                Poll::Ready(Err(error)) => {
                    return into_json(&[currency_status(
                        "Exchange rates unavailable",
                        error,
                        false,
                    )])
                }
                Poll::Ready(Ok(())) => answer = rink_core::eval(&mut state.context, query),
            }
        }
        match answer {
            Ok(reply) => {
                let answer = reply.to_string();
                let (title, description) = answer
                    .split_once(" (")
                    .map(|(title, description)| {
                        (
                            title.to_owned(),
                            description.trim_end_matches(')').to_owned(),
                        )
                    })
                    .unwrap_or((answer, "Calculator and unit conversion".to_owned()));
                state.current = Some(Entry {
                    expression: query.to_owned(),
                    answer: title.clone(),
                });
                into_json(&vec![PluginResult {
                    id: title.clone(),
                    title,
                    description,
                    score: 10_000,
                    icon: Some("accessories-calculator".into()),
                    image: None,
                    metadata: Some("rink".into()),
                }])
            }
            Err(error @ QueryError::MissingDeps(_)) => into_json(&[currency_status(
                "Exchange rate unavailable",
                error.to_string(),
                false,
            )]),
            Err(_) => into_json(&Vec::<PluginResult>::new()),
        }
    }))
    .unwrap_or(std::ptr::null_mut())
}

unsafe extern "C" fn action(state: *mut c_void, result_id: *const c_char) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() || result_id.is_null() {
            return into_json(&PluginAction::None);
        }
        // SAFETY: The host passes a live plugin state and NUL-terminated result id.
        let state = unsafe { &mut *state.cast::<State>() };
        // SAFETY: The host's result id pointer is valid for this callback.
        let Ok(result) = unsafe { CStr::from_ptr(result_id) }.to_str() else {
            return into_json(&PluginAction::None);
        };
        if let Some(entry) = state.current.as_ref() {
            if !result.is_empty() && entry.answer == result {
                let copy = PluginAction::CopyText(entry.answer.clone());
                if let Err(error) = state.history.record(entry.clone()) {
                    eprintln!("waymenu-rink: could not save calculator history: {error}");
                }
                return into_json(&copy);
            }
        } else if let Some(entry) = result
            .strip_prefix(HISTORY_ID_PREFIX)
            .and_then(|expression| {
                state
                    .history
                    .entries
                    .iter()
                    .find(|entry| entry.expression == expression)
            })
        {
            return into_json(&PluginAction::CopyText(entry.answer.clone()));
        }
        into_json(&PluginAction::None)
    }))
    .unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
unsafe extern "C" fn waymenu_plugin_delete_result_v2(
    state: *mut c_void,
    id: *const c_char,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        if state.is_null() || id.is_null() {
            return false;
        }
        // SAFETY: The host passes a live plugin state and NUL-terminated result ID.
        let (state, id) = unsafe { (&mut *state.cast::<State>(), CStr::from_ptr(id)) };
        let Some(expression) = id
            .to_str()
            .ok()
            .and_then(|id| id.strip_prefix(HISTORY_ID_PREFIX))
        else {
            return false;
        };
        if state.current.is_some()
            || !state
                .history
                .entries
                .iter()
                .any(|entry| entry.expression == expression)
        {
            return false;
        }
        match state.history.delete(expression) {
            Ok(()) => true,
            Err(error) => {
                eprintln!("waymenu-rink: could not delete calculator history entry: {error}");
                false
            }
        }
    }))
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode<T: serde::de::DeserializeOwned>(json: *mut c_char) -> T {
        assert!(!json.is_null());
        // SAFETY: Each callback returns its own JSON pointer, released by free_json below.
        unsafe {
            let result = serde_json::from_slice(CStr::from_ptr(json).to_bytes());
            free_json(json);
            result.unwrap()
        }
    }

    #[test]
    fn saves_history_on_selection_and_recalls_it_without_evaluating() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");
        let mut state = State {
            context: new_context().unwrap(),
            rates: currency::Rates::new(),
            history: History::load(Some(path.clone())),
            current: None,
        };
        let state_ptr = (&mut state as *mut State).cast();

        // SAFETY: The state lives throughout the test, and each query and ID is NUL-terminated.
        unsafe {
            let preview: Vec<PluginResult> = decode(search(state_ptr, c"2+2".as_ptr()));
            let id = std::ffi::CString::new(preview[0].id.as_str()).unwrap();
            assert!(!path.exists());
            let copy: PluginAction = decode(action(state_ptr, id.as_ptr()));
            assert!(matches!(copy, PluginAction::CopyText(text) if text == preview[0].title));
        }

        state.history = History::load(Some(path));
        let state_ptr = (&mut state as *mut State).cast();
        // SAFETY: The reloaded state and the NUL-terminated strings live through each callback.
        unsafe {
            let history: Vec<PluginResult> = decode(search(state_ptr, c"".as_ptr()));
            assert_eq!(history.len(), 1);
            let entry = &history[0];
            assert_eq!(entry.title, "2+2");
            assert_eq!(entry.metadata.as_deref(), Some("autocomplete:2+2"));
            let id = std::ffi::CString::new(entry.id.as_str()).unwrap();
            let copy: PluginAction = decode(action(state_ptr, id.as_ptr()));
            assert!(matches!(copy, PluginAction::CopyText(text) if text == entry.description));
            let stale: PluginAction = decode(action(state_ptr, c"4".as_ptr()));
            assert!(matches!(stale, PluginAction::None));
        }
    }

    #[test]
    fn deletes_only_the_selected_saved_expression() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.json");
        let mut state = State {
            context: new_context().unwrap(),
            rates: currency::Rates::new(),
            history: History::load(Some(path.clone())),
            current: None,
        };
        for expression in ["2+2", "3+1"] {
            state
                .history
                .record(Entry {
                    expression: expression.into(),
                    answer: "4".into(),
                })
                .unwrap();
        }
        let state_ptr = (&mut state as *mut State).cast();

        // SAFETY: The state and NUL-terminated IDs remain live throughout the callbacks.
        unsafe {
            let live: Vec<PluginResult> = decode(search(state_ptr, c"2+2".as_ptr()));
            assert_eq!(live[0].id, "4");
            assert!(!waymenu_plugin_delete_result_v2(state_ptr, c"4".as_ptr()));
            let history: Vec<PluginResult> = decode(search(state_ptr, c"".as_ptr()));
            let id = std::ffi::CString::new(history[1].id.as_str()).unwrap();
            assert!(waymenu_plugin_delete_result_v2(state_ptr, id.as_ptr()));
            assert!(!waymenu_plugin_delete_result_v2(state_ptr, id.as_ptr()));
            let remaining: Vec<PluginResult> = decode(search(state_ptr, c"".as_ptr()));
            assert_eq!(remaining.len(), 1);
            assert_eq!(remaining[0].title, "3+1");
        }
        let reloaded = History::load(Some(path));
        assert_eq!(reloaded.entries.len(), 1);
        assert_eq!(reloaded.entries[0].expression, "3+1");
    }

    #[test]
    fn evaluates_calculations_and_unit_conversions() {
        let mut context = new_context().unwrap();
        let calculation = rink_core::one_line(&mut context, "2+2").unwrap();
        assert!(calculation.starts_with('4'));
        let conversion = rink_core::one_line(&mut context, "1 mile -> km").unwrap();
        assert!(conversion.contains("1.609344 kilometer"));
    }

    #[test]
    fn exports_expected_prefix() {
        let api = super::waymenu_plugin_v2();
        let prefix = unsafe { std::ffi::CStr::from_ptr(api.prefix) }
            .to_str()
            .unwrap();
        assert_eq!(prefix, "= ");
    }
}
