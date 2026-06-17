use serde::de::DeserializeOwned;

/// Parses a single non-empty JSON line into `T`.
///
/// Leading/trailing whitespace is trimmed and empty lines are ignored.
pub fn parse_json_line<T: DeserializeOwned>(line: &str) -> Option<T> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}
