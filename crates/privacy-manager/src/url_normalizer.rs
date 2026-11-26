//! URL and omnibox address bar input normalization.

use url::Url;

/// Normalizes raw user input from the address bar into a valid navigation URL.
pub fn normalize_omnibox_input(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return "https://www.google.com".to_string();
    }

    // Explicit scheme
    if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("about:")
        || trimmed.starts_with("edge://")
    {
        return trimmed.to_string();
    }

    // Common localhost
    if trimmed.starts_with("localhost") {
        return format!("http://{}", trimmed);
    }

    // Check if it looks like a domain name (contains dot and no spaces)
    if !trimmed.contains(' ') && trimmed.contains('.') {
        if let Ok(parsed) = Url::parse(&format!("https://{}", trimmed)) {
            if parsed.host_str().is_some() {
                return format!("https://{}", trimmed);
            }
        }
    }

    // Fall back to Google Search
    let query = urlencoding_encode(trimmed);
    format!("https://www.google.com/search?q={}", query)
}

fn urlencoding_encode(s: &str) -> String {
    let mut encoded = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            encoded.push(b as char);
        } else if b == b' ' {
            encoded.push('+');
        } else {
            encoded.push_str(&format!("%{:02X}", b));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_urls() {
        assert_eq!(
            normalize_omnibox_input("https://github.com"),
            "https://github.com"
        );
        assert_eq!(
            normalize_omnibox_input("http://example.org/path?k=v"),
            "http://example.org/path?k=v"
        );
        assert_eq!(normalize_omnibox_input("about:blank"), "about:blank");
    }

    #[test]
    fn test_domain_auto_prefix() {
        assert_eq!(normalize_omnibox_input("crates.io"), "https://crates.io");
        assert_eq!(
            normalize_omnibox_input("rust-lang.org/learn"),
            "https://rust-lang.org/learn"
        );
    }

    #[test]
    fn test_search_queries() {
        assert_eq!(
            normalize_omnibox_input("rust 2026 async await"),
            "https://www.google.com/search?q=rust+2026+async+await"
        );
        assert_eq!(
            normalize_omnibox_input("how to bake bread"),
            "https://www.google.com/search?q=how+to+bake+bread"
        );
    }
}
