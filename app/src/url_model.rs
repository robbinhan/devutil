//! URL status text. Encoding stays in `url_codec_core`.

use url_codec_core::{UrlCodecAnalysis, UrlCodecMode};

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

pub fn status_line(
    source: &str,
    mode: UrlCodecMode,
    analysis: Option<&UrlCodecAnalysis>,
) -> StatusLine {
    if let Some(analysis) = analysis {
        if let Some(diagnostic) = &analysis.diagnostic {
            return StatusLine {
                text: diagnostic.clone(),
                tone: Tone::Danger,
            };
        }
        let verb = match analysis.mode {
            UrlCodecMode::Decode => "Decoded",
            UrlCodecMode::Encode => "Encoded",
        };
        return StatusLine {
            text: format!("{verb} successfully."),
            tone: Tone::Success,
        };
    }
    if source.is_empty() {
        StatusLine {
            text: "Supports %XX escapes and + → space (application/x-www-form-urlencoded)."
                .to_string(),
            tone: Tone::Muted,
        }
    } else {
        let text = match mode {
            UrlCodecMode::Decode => "Ready to decode percent-encoded text.",
            UrlCodecMode::Encode => "Ready to encode text for URLs / form bodies.",
        };
        StatusLine {
            text: text.to_string(),
            tone: Tone::Info,
        }
    }
}

/// Mode changes recompute when a result exists or the field is non-empty.
/// Whitespace counts: the GPUI workbench does not trim URL input.
pub fn should_recompute(has_analysis: bool, source: &str) -> bool {
    has_analysis || !source.is_empty()
}

pub fn action_label(mode: UrlCodecMode) -> &'static str {
    match mode {
        UrlCodecMode::Decode => "Decode",
        UrlCodecMode::Encode => "Encode",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use url_codec_core::UrlCodecEngine;

    #[test]
    fn empty_and_blank_url_input_differ() {
        let empty = status_line("", UrlCodecMode::Decode, None);
        assert_eq!(empty.tone, Tone::Muted);
        let blank = status_line(" ", UrlCodecMode::Decode, None);
        assert_eq!(blank.tone, Tone::Info);
        assert!(!should_recompute(false, ""));
        assert!(should_recompute(false, " "));
        assert!(should_recompute(true, ""));
    }

    #[test]
    fn a_decode_reports_success() {
        let analysis = UrlCodecEngine::new().convert("hello%20world", UrlCodecMode::Decode);
        let line = status_line("hello%20world", UrlCodecMode::Decode, Some(&analysis));
        assert_eq!(line.tone, Tone::Success);
        assert_eq!(line.text, "Decoded successfully.");
        assert_eq!(analysis.output.as_deref(), Some("hello world"));
    }
}
