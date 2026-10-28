//! Hash workbench labels. Digests stay in `hash_core`.

use std::path::Path;
use std::time::Duration;

use hash_core::{DigestRow, matching_labels};

use crate::json_model::{self, FieldPreview, field_text};

/// Wait after a keystroke before hashing. Huge input is not hashed on every
/// change, and anything past [`INLINE_LIMIT`] also leaves the engine thread.
pub const DEBOUNCE: Duration = Duration::from_millis(200);

/// UTF-8 bytes hashed on the engine thread. Larger input uses a blocking worker.
pub const INLINE_LIMIT: usize = 64 * 1024;

pub const SAMPLE: &str = "The quick brown fox jumps over the lazy dog";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputMode {
    Text,
    File,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Muted,
    Info,
    Success,
    Danger,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusLine {
    pub text: String,
    pub tone: Tone,
}

pub fn hash_inline(len: usize) -> bool {
    len <= INLINE_LIMIT
}

/// The prefix the text view may show. Hashing uses the full string.
pub fn displayed_input(full: &str) -> FieldPreview {
    field_text(full)
}

pub fn cap_message(full_bytes: usize, preview_bytes: usize) -> String {
    format!(
        "Showing the first {} of {}. Hash uses the full text. The text view stops at {} UTF-16 units and {} lines.",
        json_model::format_bytes(preview_bytes),
        json_model::format_bytes(full_bytes),
        json_model::FIELD_LIMIT,
        json_model::FIELD_LINE_LIMIT
    )
}

pub fn file_name(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
}

pub fn text_status(bytes: usize, busy: bool, hmac: bool) -> StatusLine {
    if busy && !hash_inline(bytes) {
        return StatusLine {
            text: format!(
                "Hashing {} on a background thread.",
                json_model::format_bytes(bytes)
            ),
            tone: Tone::Info,
        };
    }
    if busy {
        return StatusLine {
            text: "Hashing…".to_string(),
            tone: Tone::Info,
        };
    }
    if hmac {
        return StatusLine {
            text: "HMAC with the UTF-8 key. BLAKE3 and CRC32 have no HMAC, so those rows stay blank."
                .to_string(),
            tone: Tone::Info,
        };
    }
    StatusLine {
        text: "MD5, SHA-1, SHA-224, SHA-256, SHA-384, SHA-512, SHA3-256, BLAKE3, and CRC32."
            .to_string(),
        tone: Tone::Muted,
    }
}

pub fn file_status(path: &str, size: Option<u64>, done: u64, busy: bool, error: &str) -> StatusLine {
    if !error.is_empty() {
        return StatusLine {
            text: error.to_string(),
            tone: Tone::Danger,
        };
    }
    if path.is_empty() {
        return StatusLine {
            text: "Open a file. It is read in 1 MB chunks on a background thread, so a multi-gigabyte file stays flat in memory."
                .to_string(),
            tone: Tone::Muted,
        };
    }
    let name = file_name(path);
    let size = size.unwrap_or(0);
    if busy {
        let pct = if size == 0 {
            0
        } else {
            ((done as f64 / size as f64) * 100.0).clamp(0.0, 100.0) as u64
        };
        return StatusLine {
            text: format!(
                "Hashing {name} · {} / {} ({pct}%)",
                json_model::format_bytes(done as usize),
                json_model::format_bytes(size as usize)
            ),
            tone: Tone::Info,
        };
    }
    StatusLine {
        text: format!("{name} · {}", json_model::format_bytes(size as usize)),
        tone: Tone::Success,
    }
}

pub fn compare_status(rows: &[DigestRow], expected: &str) -> StatusLine {
    if expected.trim().is_empty() {
        return StatusLine {
            text: "Paste an expected hash to highlight the algorithm that matches."
                .to_string(),
            tone: Tone::Muted,
        };
    }
    let labels = matching_labels(rows, expected);
    if labels.is_empty() {
        return StatusLine {
            text: "No algorithm matches this hash.".to_string(),
            tone: Tone::Danger,
        };
    }
    StatusLine {
        text: format!("Matches {}.", labels.join(", ")),
        tone: Tone::Success,
    }
}

pub fn progress_width(done: u64, size: u64) -> f32 {
    if size == 0 {
        return 0.0;
    }
    ((done as f64 / size as f64).clamp(0.0, 1.0) * 220.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use hash_core::hash_text;

    #[test]
    fn preview_is_not_what_gets_hashed() {
        let full = "a".repeat(80 * 1024);
        let shown = displayed_input(&full);
        assert!(shown.capped);
        assert!(shown.text.len() < full.len());
        let full_rows = hash_text(&full, None);
        let preview_rows = hash_text(&shown.text, None);
        assert_ne!(full_rows[0].bytes, preview_rows[0].bytes);
        assert!(!hash_inline(full.len()));
        assert!(hash_inline(SAMPLE.len()));
    }

    #[test]
    fn compare_status_names_the_matching_algorithm() {
        let rows = hash_text("abc", None);
        let expected = rows
            .iter()
            .find(|row| row.label == "SHA-256")
            .unwrap()
            .display(true, false);
        let hit = compare_status(&rows, &expected);
        assert_eq!(hit.tone, Tone::Success);
        assert_eq!(hit.text, "Matches SHA-256.");
        assert_eq!(compare_status(&rows, "nope").tone, Tone::Danger);
        assert_eq!(compare_status(&rows, "  ").tone, Tone::Muted);
    }

    #[test]
    fn file_status_reports_size_and_progress() {
        let idle = file_status("", None, 0, false, "");
        assert_eq!(idle.tone, Tone::Muted);
        let busy = file_status("/tmp/demo.bin", Some(2_000_000), 500_000, true, "");
        assert!(busy.text.contains("demo.bin"));
        assert!(busy.text.contains("25%"));
        let done = file_status("/tmp/demo.bin", Some(2048), 2048, false, "");
        assert_eq!(done.tone, Tone::Success);
        assert_eq!(file_status("/tmp/demo.bin", Some(1), 0, false, "denied").tone, Tone::Danger);
        assert_eq!(progress_width(50, 100), 110.0);
    }
}
