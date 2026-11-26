//! Filename sanitization protecting against path traversal, reserved Windows names, and collisions.

const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

const ILLEGAL_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

pub fn sanitize_filename(input: &str) -> String {
    // 1. Defeat path traversal by eliminating ".." sequences
    let no_traversal = input.replace("..", "__");

    // 2. Replace illegal Windows filesystem characters and control characters with '_'
    let mut cleaned: String = no_traversal
        .chars()
        .map(|c| {
            if ILLEGAL_CHARS.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();

    // 3. Strip leading and trailing dots, spaces, and boundary underscores
    cleaned = cleaned
        .trim_matches(|c| c == '.' || c == ' ' || c == '_')
        .to_string();

    if cleaned.is_empty() {
        return "download".to_string();
    }

    // 4. Check reserved Windows device names (e.g., CON.txt, NUL, COM1)
    let stem = cleaned.split('.').next().unwrap_or("").to_ascii_uppercase();

    if RESERVED_NAMES.contains(&stem.as_str()) {
        cleaned = format!("_{}", cleaned);
    }

    // 5. Truncate to maximum 255 chars
    if cleaned.len() > 255 {
        cleaned.truncate(255);
    }

    cleaned
}

/// Resolves a collision-free target file path by appending `(1)`, `(2)`, etc. if file exists.
pub fn resolve_unique_path(target_dir: &std::path::Path, raw_filename: &str) -> std::path::PathBuf {
    let sanitized = sanitize_filename(raw_filename);
    let path = std::path::Path::new(&sanitized);

    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("download");
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e))
        .unwrap_or_default();

    let mut candidate = target_dir.join(format!("{}{}", stem, ext));
    let mut counter = 1;

    while candidate.exists() {
        candidate = target_dir.join(format!("{} ({}){}", stem, counter, ext));
        counter += 1;
    }

    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_traversal_sanitization() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "etc_passwd");
        assert_eq!(
            sanitize_filename("..\\..\\Windows\\System32\\cmd.exe"),
            "Windows_System32_cmd.exe"
        );
    }

    #[test]
    fn test_reserved_windows_names() {
        assert_eq!(sanitize_filename("CON.txt"), "_CON.txt");
        assert_eq!(sanitize_filename("nul"), "_nul");
        assert_eq!(sanitize_filename("COM1.pdf"), "_COM1.pdf");
    }

    #[test]
    fn test_illegal_characters() {
        assert_eq!(
            sanitize_filename("report:2026?*<.pdf"),
            "report_2026___.pdf"
        );
    }

    #[test]
    fn test_empty_and_dot_names() {
        assert_eq!(sanitize_filename("   "), "download");
        assert_eq!(sanitize_filename("..."), "download");
        assert_eq!(sanitize_filename("___"), "download");
    }
}
