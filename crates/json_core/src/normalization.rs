use crate::strict_parser::StrictJSONParser;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    PlainJSON,
    QuotedJSON,
    EscapedJSON,
    MarkdownFence,
    Jsonp,
    Ndjson,
    UrlEncoded,
    Base64Encoded,
    EmbeddedJSON,
    Unknown,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NormalizationStep {
    pub kind: String,
    pub summary: String,
    pub automatic: bool,
    pub reversible: bool,
}

impl NormalizationStep {
    pub fn new(
        kind: impl Into<String>,
        summary: impl Into<String>,
        automatic: bool,
        reversible: bool,
    ) -> Self {
        Self {
            kind: kind.into(),
            summary: summary.into(),
            automatic,
            reversible,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NormalizationCandidate {
    pub kind: InputKind,
    pub text: String,
    pub steps: Vec<NormalizationStep>,
    pub confidence: f64,
    pub requires_user_action: bool,
}

impl NormalizationCandidate {
    pub fn new(
        kind: InputKind,
        text: impl Into<String>,
        steps: Vec<NormalizationStep>,
        confidence: f64,
        requires_user_action: bool,
    ) -> Self {
        Self {
            kind,
            text: text.into(),
            steps,
            confidence,
            requires_user_action,
        }
    }
}

/// Matches Swift `Character.isWhitespace` trimming.
pub fn json_trim_whitespace(source: &str) -> String {
    source.trim_matches(char::is_whitespace).to_string()
}

pub fn json_is_valid_json(source: &str) -> bool {
    StrictJSONParser::new().parse(source).is_ok()
}

/// Matches Swift `split(whereSeparator: \.isNewline)` with default
/// `omittingEmptySubsequences: true`.
pub fn split_newlines_omit_empty(source: &str) -> Vec<String> {
    source
        .split(is_swift_newline)
        .filter(|part| !part.is_empty())
        .map(|part| part.to_string())
        .collect()
}

fn is_swift_newline(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r' | '\u{000B}' | '\u{000C}' | '\u{0085}' | '\u{2028}' | '\u{2029}'
    )
}
