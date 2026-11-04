#[derive(Clone, Debug, PartialEq)]
pub struct UrlResult {
    pub url: String,
    pub host: String,
}

pub fn detect_url(text: &str) -> Option<UrlResult> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.contains('\n') || trimmed.contains(' ') {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    let is_url = lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("ftp://")
        || (lower.starts_with("www.") && lower.contains('.'));

    if !is_url {
        return None;
    }

    let without_scheme = if let Some(stripped) = lower.strip_prefix("https://") {
        stripped
    } else if let Some(stripped) = lower.strip_prefix("http://") {
        stripped
    } else if let Some(stripped) = lower.strip_prefix("ftp://") {
        stripped
    } else {
        &lower
    };

    let host = without_scheme
        .split(['/', ':', '?', '#'])
        .next()
        .unwrap_or(without_scheme);

    Some(UrlResult {
        url: trimmed.to_string(),
        host: host.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_urls() {
        let res = detect_url("https://github.com/rust-lang/rust").unwrap();
        assert_eq!(res.url, "https://github.com/rust-lang/rust");
        assert_eq!(res.host, "github.com");

        let res = detect_url("http://localhost:8080/dashboard").unwrap();
        assert_eq!(res.host, "localhost");

        let res = detect_url("www.google.com").unwrap();
        assert_eq!(res.host, "www.google.com");

        assert!(detect_url("not a url").is_none());
        assert!(detect_url("hello https://url.com").is_none());
    }
}
