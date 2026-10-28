use crate::candidate::{json_base64_decode, json_extract_balanced_json, json_percent_decode};
use crate::normalization::{
    json_is_valid_json, json_trim_whitespace, split_newlines_omit_empty, InputKind,
};

pub struct InputClassifier;

impl InputClassifier {
    pub fn new() -> Self {
        Self
    }

    pub fn classify(&self, source: &str) -> Vec<InputKind> {
        let trimmed = json_trim_whitespace(source);
        let mut kinds = Vec::new();

        if json_is_valid_json(&trimmed) {
            kinds.push(InputKind::PlainJSON);
        }
        if self.is_markdown_fence(&trimmed) {
            kinds.push(InputKind::MarkdownFence);
        }
        if trimmed.starts_with('"') {
            kinds.push(InputKind::QuotedJSON);
        }
        if contains_escaped_quote(&trimmed) {
            kinds.push(InputKind::EscapedJSON);
        }
        if self.is_jsonp(&trimmed) {
            kinds.push(InputKind::Jsonp);
        }
        if self.is_ndjson(&trimmed) {
            kinds.push(InputKind::Ndjson);
        }
        if json_percent_decode(&trimmed).is_some() {
            kinds.push(InputKind::UrlEncoded);
        }
        if json_base64_decode(&trimmed).is_some() {
            kinds.push(InputKind::Base64Encoded);
        }
        if json_extract_balanced_json(&trimmed).is_some()
            && !trimmed.starts_with('{')
            && !trimmed.starts_with('[')
        {
            kinds.push(InputKind::EmbeddedJSON);
        }
        if kinds.is_empty() {
            kinds.push(InputKind::Unknown);
        }
        kinds
    }

    pub fn is_markdown_fence(&self, source: &str) -> bool {
        if !source.starts_with("```") {
            return false;
        }
        let Some(newline) = source.find('\n') else {
            return false;
        };
        source.ends_with("```") && newline < source.len().saturating_sub(1)
    }

    pub fn is_jsonp(&self, source: &str) -> bool {
        let Some(open) = source.find('(') else {
            return false;
        };
        if !(source.ends_with(')') || source.ends_with(");")) {
            return false;
        }
        let prefix = &source[..open];
        !prefix.is_empty()
            && prefix
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
    }

    pub fn is_ndjson(&self, source: &str) -> bool {
        let lines = split_newlines_omit_empty(source);
        if lines.len() <= 1 {
            return false;
        }
        lines
            .iter()
            .all(|line| json_is_valid_json(&json_trim_whitespace(line)))
    }
}

impl Default for InputClassifier {
    fn default() -> Self {
        Self::new()
    }
}

fn contains_escaped_quote(source: &str) -> bool {
    let mut escaped = false;
    for character in source.chars() {
        if escaped && character == '"' {
            return true;
        }
        escaped = character == '\\' && !escaped;
    }
    false
}
