use rink_core::Context;
use std::{
    ffi::{c_char, c_void, CStr},
    panic::{catch_unwind, AssertUnwindSafe},
};
use waymenu_plugin_api::{
    free_json, into_json, PluginAction, PluginApiV2, PluginResult, ABI_VERSION,
};

struct State {
    context: Context,
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
        Some(context) => Box::into_raw(Box::new(State { context })).cast(),
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
        if query.trim().is_empty() {
            return into_json(&Vec::<PluginResult>::new());
        }
        match rink_core::one_line(&mut state.context, query.trim()) {
            Ok(answer) => {
                let (title, description) = answer
                    .split_once(" (")
                    .map(|(title, description)| {
                        (
                            title.to_owned(),
                            description.trim_end_matches(')').to_owned(),
                        )
                    })
                    .unwrap_or((answer, "Calculator and unit conversion".to_owned()));
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
        let _state = unsafe { &mut *state.cast::<State>() };
        // SAFETY: The host's result id pointer is valid for this callback.
        let Ok(result) = unsafe { CStr::from_ptr(result_id) }.to_str() else {
            return into_json(&PluginAction::None);
        };
        into_json(&PluginAction::CopyText(result.to_owned()))
    }))
    .unwrap_or(std::ptr::null_mut())
}

#[cfg(test)]
mod tests {
    use super::new_context;

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
