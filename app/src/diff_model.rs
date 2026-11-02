//! Plain helpers for the Diff tool: JSON normalization before a compare,
//! and the sample. No UI here, so it is tested without a window.

use json_core::{CanonicalSerializer, JSONValue, StrictJSONParser};

/// Diff inputs larger than this (both sides together) are compared on a
/// worker thread.
pub const INLINE_BYTES: usize = 256 * 1024;

/// Rows built for each view. The summary and the patch cover everything.
pub const ROW_CAP: usize = 5_000;

/// Inputs beyond this are not compared at all.
pub const MAX_BYTES: usize = 32 * 1024 * 1024;

pub const SAMPLE_ORIGINAL: &str = "\
# Sample settings (synthetic example)
name = \"demo-service\"
version = \"1.4.0\"
port = 8080
log_level = \"info\"
features = [\"search\", \"export\"]
retry_limit = 3
timeout_ms = 1500
owner = \"team-example\"
";

pub const SAMPLE_CHANGED: &str = "\
# Sample settings (synthetic example)
name = \"demo-service\"
version = \"1.5.0\"
port = 8080
log_level = \"debug\"
features = [\"search\", \"export\", \"preview\"]
timeout_ms = 1500
cache_size = 256
owner = \"team-example\"
";

/// Pretty JSON with object keys sorted, so key order and indentation do
/// not show up as changes. `None` when the text is not strict JSON.
pub fn normalize_json(text: &str) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    let mut value = StrictJSONParser::new().parse(text).ok()?;
    sort_keys(&mut value);
    let mut out = CanonicalSerializer::pretty(&value);
    out.push('\n');
    Some(out)
}

fn sort_keys(value: &mut JSONValue) {
    match value {
        JSONValue::Object(members) => {
            members.sort_by(|a, b| a.key.cmp(&b.key));
            for member in members.iter_mut() {
                sort_keys(&mut member.value);
            }
        }
        JSONValue::Array(values) => {
            for value in values.iter_mut() {
                sort_keys(value);
            }
        }
        _ => {}
    }
}

/// Both sides normalized when both are JSON; otherwise the texts as given.
pub fn prepare(old: String, new: String, format_json: bool) -> (String, String, bool) {
    if !format_json {
        return (old, new, false);
    }
    match (normalize_json(&old), normalize_json(&new)) {
        (Some(old), Some(new)) => (old, new, true),
        _ => (old, new, false),
    }
}

/// File name for the patch header (`a/<name>`).
pub fn patch_name(prefix: &str, name: &str, fallback: &str) -> String {
    let base = std::path::Path::new(name)
        .file_name()
        .map(|base| base.to_string_lossy().into_owned())
        .filter(|base| !base.is_empty())
        .unwrap_or_else(|| fallback.to_string());
    format!("{prefix}/{base}")
}

pub fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_keys_are_sorted_and_pretty() {
        let left = normalize_json("{\"b\":1,\"a\":{\"d\":[1,2],\"c\":null}}").unwrap();
        let right =
            normalize_json("{\n  \"a\": {\"c\": null, \"d\": [1, 2]},\n  \"b\": 1\n}").unwrap();
        assert_eq!(left, right);
        assert!(left.starts_with("{\n  \"a\""));
    }

    #[test]
    fn not_json_compares_as_text() {
        assert_eq!(normalize_json("key = value"), None);
        let (old, new, applied) = prepare("{}".into(), "not json".into(), true);
        assert!(!applied);
        assert_eq!(old, "{}");
        assert_eq!(new, "not json");
    }

    #[test]
    fn samples_differ() {
        let result = diff_core::compare(
            SAMPLE_ORIGINAL,
            SAMPLE_CHANGED,
            diff_core::Options::default(),
            diff_core::Layout::default(),
        );
        assert!(!result.summary.identical());
        assert_eq!(result.summary.removed, 4);
        assert_eq!(result.summary.added, 4);
    }

    #[test]
    fn patch_names() {
        assert_eq!(
            patch_name("a", "/tmp/x/config.toml", "original"),
            "a/config.toml"
        );
        assert_eq!(patch_name("b", "", "changed"), "b/changed");
    }
}
