use std::{path::PathBuf, process::Command};

#[derive(Clone, Debug, PartialEq)]
pub struct ImageEntry {
    pub id: String,
    pub size: String,
    pub mime: String,
    pub dimensions: String,
}

pub fn parse_image_line(id: &str, line: &str) -> Option<ImageEntry> {
    let trimmed = line.trim();
    if !trimmed.starts_with("[[ binary data") || !trimmed.ends_with("]]") {
        return None;
    }

    let inner = trimmed
        .trim_start_matches("[[ binary data")
        .trim_end_matches("]]")
        .trim();

    let tokens: Vec<&str> = inner.split_whitespace().collect();
    let mut mime = String::new();
    let mut size = String::new();
    let mut dimensions = String::new();

    for token in tokens {
        if token.starts_with("image/") {
            mime = token.to_string();
        } else if token.contains('x') && token.chars().all(|c| c.is_ascii_digit() || c == 'x') {
            dimensions = token.to_string();
        } else if token.ends_with("kB")
            || token.ends_with("MB")
            || token.ends_with('B')
            || token.ends_with("KiB")
            || token.ends_with("MiB")
        {
            size = token.to_string();
        }
    }

    if !mime.starts_with("image/") {
        return None;
    }

    Some(ImageEntry {
        id: id.to_string(),
        size,
        mime,
        dimensions,
    })
}

pub fn ensure_thumbnail(id: &str, mime: &str) -> Option<PathBuf> {
    let cache_dir = std::env::temp_dir().join("waymenu-clipboard");
    let _ = std::fs::create_dir_all(&cache_dir);
    let ext = match mime {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        _ => "png",
    };
    let file_path = cache_dir.join(format!("{id}.{ext}"));
    if file_path.is_file() {
        if let Ok(metadata) = file_path.metadata() {
            if metadata.len() > 0 {
                return Some(file_path);
            }
        }
    }

    // Try clipvault first, then cliphist
    let output = Command::new("clipvault")
        .args(["get", id])
        .output()
        .or_else(|_| Command::new("cliphist").args(["decode", id]).output())
        .ok()?;

    if output.status.success() && !output.stdout.is_empty() {
        let _ = std::fs::write(&file_path, &output.stdout);
        Some(file_path)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clipvault_image_line() {
        let line = "[[ binary data 18kB image/png 228x224 ]]";
        let entry = parse_image_line("1", line).unwrap();
        assert_eq!(entry.id, "1");
        assert_eq!(entry.mime, "image/png");
        assert_eq!(entry.size, "18kB");
        assert_eq!(entry.dimensions, "228x224");
    }

    #[test]
    fn ignores_non_image_binary() {
        let line = "[[ binary data 200kB application/pdf ]]";
        assert!(parse_image_line("2", line).is_none());
    }
}
