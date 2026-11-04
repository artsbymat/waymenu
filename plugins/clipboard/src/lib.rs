use std::{
    ffi::{c_char, c_void, CStr},
    io::Write,
    panic::{catch_unwind, AssertUnwindSafe},
    process::{Command, Stdio},
};
use waymenu_plugin_api::{
    free_json, into_json, PluginAction, PluginApiV2, PluginResult, ABI_VERSION,
};

mod backend;
mod color;
mod image;
mod url;

struct State;

#[unsafe(no_mangle)]
pub extern "C" fn waymenu_plugin_v2() -> PluginApiV2 {
    PluginApiV2 {
        abi_version: ABI_VERSION,
        id: c"clipboard".as_ptr(),
        prefix: c"cb ".as_ptr(),
        name: c"Clipboard".as_ptr(),
        description: c"Search and paste clipboard history with clipvault".as_ptr(),
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
        into_json(&search_clipboard(raw_query))
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
        into_json(&execute_action(id_str))
    }))
    .unwrap_or(std::ptr::null_mut())
}

fn execute_action(action_id: &str) -> PluginAction {
    // action_id can be in form: "id" or "id|mime"
    let (id, mime) = if let Some((id, mime)) = action_id.split_once('|') {
        (id, Some(mime))
    } else {
        (action_id, None)
    };

    if id.is_empty() {
        return PluginAction::None;
    }

    let output = Command::new("clipvault")
        .args(["get", id])
        .output()
        .or_else(|_| Command::new("cliphist").args(["decode", id]).output());

    match output {
        Ok(out) if out.status.success() && !out.stdout.is_empty() => {
            let mut cmd = Command::new("wl-copy");
            if let Some(mime) = mime {
                cmd.args(["-t", mime]);
            }
            cmd.stdin(Stdio::piped());
            if let Ok(mut child) = cmd.spawn() {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(&out.stdout);
                }
                let _ = child.wait();
            }

            if mime.is_none() {
                if let Ok(text) = String::from_utf8(out.stdout) {
                    return PluginAction::CopyText(text);
                }
            }
            PluginAction::None
        }
        _ => {
            let shell_cmd = if let Some(mime) = mime {
                format!(
                    "clipvault get '{id}' 2>/dev/null | wl-copy -t '{mime}' || cliphist decode '{id}' | wl-copy -t '{mime}'"
                )
            } else {
                format!(
                    "clipvault get '{id}' 2>/dev/null | wl-copy || cliphist decode '{id}' | wl-copy"
                )
            };
            let _ = Command::new("sh").args(["-c", &shell_cmd]).status();
            PluginAction::None
        }
    }
}

fn search_clipboard(query: &str) -> Vec<PluginResult> {
    let raw_entries = match backend::list_entries() {
        Ok(entries) => entries,
        Err(err) => {
            return vec![PluginResult {
                id: String::new(),
                title: "Clipboard manager unavailable".into(),
                description: err,
                score: 1,
                icon: None,
                image: None,
                metadata: None,
            }];
        }
    };

    if raw_entries.is_empty() {
        return vec![PluginResult {
            id: String::new(),
            title: "Clipboard is empty".into(),
            description: "No entries stored in clipvault history".into(),
            score: 1,
            icon: None,
            image: None,
            metadata: None,
        }];
    }

    process_entries(raw_entries, query)
}

fn process_entries(raw_entries: Vec<backend::ClipboardRawEntry>, query: &str) -> Vec<PluginResult> {
    let query = query.trim().to_lowercase();
    let mut results = Vec::new();

    for (index, entry) in raw_entries.into_iter().enumerate() {
        // Base score decreases with recency
        let base_score = 10_000 - (index as i64 * 10).min(9_000);

        // 1. Check if binary image
        if let Some(img) = image::parse_image_line(&entry.id, &entry.content) {
            let dim_str = if img.dimensions.is_empty() {
                "Image".to_string()
            } else {
                format!("{} px", img.dimensions.replace('x', " × "))
            };
            let mime_short = img
                .mime
                .strip_prefix("image/")
                .unwrap_or(&img.mime)
                .to_uppercase();
            let title = format!("Image ({mime_short}) · {dim_str}");
            let description = format!("{} · {} · Enter to copy image", img.size, img.mime);

            let thumb = image::ensure_thumbnail(&img.id, &img.mime)
                .map(|p| p.to_string_lossy().to_string());

            let match_score = if query.is_empty() {
                base_score
            } else {
                let mut score = 0;
                let search_target =
                    format!("{} {} {} {}", title, description, img.dimensions, img.mime)
                        .to_lowercase();
                if search_target.contains(&query) {
                    score = base_score + 1000;
                }
                score
            };

            if match_score > 0 {
                results.push(PluginResult {
                    id: format!("{}|{}", entry.id, img.mime),
                    title,
                    description,
                    score: match_score,
                    icon: thumb,
                    image: None,
                    metadata: Some(format!("image:{}", img.mime)),
                });
            }
            continue;
        }

        // 2. Check if Color (HEX, RGB, HSL, HSV, CMYK, OKLCH)
        if let Some(color) = color::detect_color(&entry.content) {
            let hex = color.to_hex();
            let title = entry.content.clone();
            let description = format!("{} Color · {}", color.format_name, color.summary);

            let match_score = if query.is_empty() {
                base_score
            } else {
                let target = format!("{} {} {}", title, description, hex).to_lowercase();
                if target.contains(&query) {
                    base_score + 1000
                } else {
                    0
                }
            };

            if match_score > 0 {
                results.push(PluginResult {
                    id: entry.id,
                    title,
                    description,
                    score: match_score,
                    icon: Some(format!("color:{hex}")),
                    image: None,
                    metadata: Some("color".into()),
                });
            }
            continue;
        }

        // 3. Check if URL/Link
        if let Some(url_info) = url::detect_url(&entry.content) {
            let title = url_info.url;
            let description = format!("Web Link · {}", url_info.host);

            let match_score = if query.is_empty() {
                base_score
            } else {
                let target = format!("{} {}", title, description).to_lowercase();
                if target.contains(&query) {
                    base_score + 1000
                } else {
                    0
                }
            };

            if match_score > 0 {
                results.push(PluginResult {
                    id: entry.id,
                    title,
                    description,
                    score: match_score,
                    icon: Some("🌍".to_string()),
                    image: None,
                    metadata: Some("url".into()),
                });
            }
            continue;
        }

        // 4. Regular Text Entry
        let first_line = entry
            .content
            .lines()
            .next()
            .unwrap_or(&entry.content)
            .trim();
        let title = if first_line.len() > 80 {
            format!("{}...", &first_line[..77])
        } else {
            first_line.to_string()
        };

        let line_count = entry.content.lines().count();
        let description = if line_count > 1 {
            format!(
                "Multiline text · {} lines · {} chars",
                line_count,
                entry.content.len()
            )
        } else {
            format!("Text · {} chars", entry.content.len())
        };

        let match_score = if query.is_empty() {
            base_score
        } else {
            let target = entry.content.to_lowercase();
            if target.contains(&query) {
                base_score + 1000
            } else {
                0
            }
        };

        if match_score > 0 {
            results.push(PluginResult {
                id: entry.id,
                title,
                description,
                score: match_score,
                icon: None,
                image: None,
                metadata: Some("text".into()),
            });
        }
    }

    results.sort_by_key(|r| std::cmp::Reverse(r.score));
    results.truncate(50);
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_expected_prefix_and_metadata() {
        let api = waymenu_plugin_v2();
        let prefix = unsafe { CStr::from_ptr(api.prefix) }.to_str().unwrap();
        assert_eq!(prefix, "cb ");
        let id = unsafe { CStr::from_ptr(api.id) }.to_str().unwrap();
        assert_eq!(id, "clipboard");
    }

    #[test]
    fn execute_action_handles_empty_id() {
        assert!(matches!(execute_action(""), PluginAction::None));
    }

    #[test]
    fn process_entries_formats_color_url_and_image_entries() {
        let entries = vec![
            backend::ClipboardRawEntry {
                id: "1".into(),
                content: "#3498db".into(),
            },
            backend::ClipboardRawEntry {
                id: "2".into(),
                content: "oklch(0.65 0.24 25)".into(),
            },
            backend::ClipboardRawEntry {
                id: "3".into(),
                content: "https://github.com/rust-lang/rust".into(),
            },
            backend::ClipboardRawEntry {
                id: "4".into(),
                content: "[[ binary data 18kB image/png 228x224 ]]".into(),
            },
            backend::ClipboardRawEntry {
                id: "5".into(),
                content: "Regular plain text\nSecond line".into(),
            },
        ];

        let results = process_entries(entries, "");
        assert_eq!(results.len(), 5);

        // 1. Color HEX
        assert_eq!(results[0].title, "#3498db");
        assert_eq!(results[0].icon, Some("color:#3498db".into()));
        assert!(results[0].description.contains("HEX Color"));

        // 2. Color OKLCH
        assert_eq!(results[1].title, "oklch(0.65 0.24 25)");
        assert!(results[1].icon.as_ref().unwrap().starts_with("color:#"));
        assert!(results[1].description.contains("OKLCH Color"));

        // 3. URL with 🌍
        assert_eq!(results[2].title, "https://github.com/rust-lang/rust");
        assert_eq!(results[2].icon, Some("🌍".into()));
        assert!(results[2].description.contains("github.com"));

        // 4. Image with dimensions and size
        assert_eq!(results[3].id, "4|image/png");
        assert!(results[3].title.contains("228 × 224 px"));
        assert!(results[3].description.contains("18kB"));
        assert_eq!(results[3].metadata, Some("image:image/png".into()));

        // 5. Plain multiline text
        assert_eq!(results[4].title, "Regular plain text");
        assert!(results[4].description.contains("2 lines"));
    }
}
