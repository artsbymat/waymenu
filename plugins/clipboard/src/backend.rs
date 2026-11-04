use std::process::Command;

#[derive(Clone, Debug, PartialEq)]
pub struct ClipboardRawEntry {
    pub id: String,
    pub content: String,
}

pub fn list_entries() -> Result<Vec<ClipboardRawEntry>, String> {
    let output = Command::new("clipvault")
        .arg("list")
        .output()
        .or_else(|_| Command::new("cliphist").arg("list").output())
        .map_err(|e| format!("could not run clipvault or cliphist: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("clipboard manager failed: {err}"));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some((id, content)) = line.split_once('\t') {
            entries.push(ClipboardRawEntry {
                id: id.trim().to_string(),
                content: content.to_string(),
            });
        } else if let Some((id, content)) = line.split_once(' ') {
            entries.push(ClipboardRawEntry {
                id: id.trim().to_string(),
                content: content.to_string(),
            });
        } else {
            entries.push(ClipboardRawEntry {
                id: line.to_string(),
                content: line.to_string(),
            });
        }
    }

    Ok(entries)
}
