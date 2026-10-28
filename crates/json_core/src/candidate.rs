use crate::classifier::InputClassifier;
use crate::json_value::JSONValue;
use crate::normalization::{
    json_is_valid_json, json_trim_whitespace, split_newlines_omit_empty, InputKind,
    NormalizationCandidate, NormalizationStep,
};
use crate::strict_parser::StrictJSONParser;

pub struct CandidateNormalizer {
    parser: StrictJSONParser,
    classifier: InputClassifier,
}

impl CandidateNormalizer {
    pub fn new() -> Self {
        Self {
            parser: StrictJSONParser::new(),
            classifier: InputClassifier::new(),
        }
    }

    pub fn generate_candidates(&self, source: &str) -> Vec<NormalizationCandidate> {
        let trimmed = json_trim_whitespace(source);
        let mut candidates: Vec<NormalizationCandidate> = Vec::new();

        let mut append = |kind: InputKind,
                          text: String,
                          steps: Vec<NormalizationStep>,
                          confidence: f64,
                          requires_user_action: bool| {
            if text == source || !json_is_valid_json(&text) {
                return;
            }
            if candidates
                .iter()
                .any(|c| c.kind == kind && c.text == text)
            {
                return;
            }
            candidates.push(NormalizationCandidate::new(
                kind,
                text,
                steps,
                confidence,
                requires_user_action,
            ));
        };

        if let Some(fenced) = strip_markdown_fence(&trimmed) {
            append(
                InputKind::MarkdownFence,
                fenced,
                vec![step(
                    "strip-markdown-fence",
                    "Removed the surrounding Markdown JSON fence",
                    true,
                )],
                1.0,
                false,
            );
        }

        let mut decoded = trimmed.clone();
        let mut string_steps: Vec<NormalizationStep> = Vec::new();
        for _ in 0..4 {
            let Ok(value) = self.parser.parse(&decoded) else {
                break;
            };
            let JSONValue::String(inner) = value else {
                break;
            };
            let next = json_trim_whitespace(&inner);
            if next == decoded {
                break;
            }
            decoded = next;
            string_steps.push(step(
                "unwrap-json-string",
                "Unwrapped one JSON string layer",
                true,
            ));
            if json_is_valid_json(&decoded) {
                append(
                    if string_steps.len() == 1 {
                        InputKind::QuotedJSON
                    } else {
                        InputKind::EscapedJSON
                    },
                    decoded.clone(),
                    string_steps.clone(),
                    0.99,
                    false,
                );
            }
        }

        if json_looks_like_raw_escaped_container(&trimmed) {
            let mut raw_escaped = trimmed.clone();
            let mut raw_steps: Vec<NormalizationStep> = Vec::new();

            for _ in 0..6 {
                let Some(next) = json_remove_one_raw_escape_layer(&raw_escaped) else {
                    break;
                };
                if next == raw_escaped {
                    break;
                }
                raw_escaped = next;
                raw_steps.push(step(
                    "unwrap-raw-escape-layer",
                    "Removed one raw escape layer from JSON quotes",
                    true,
                ));

                if json_is_valid_json(&raw_escaped) {
                    append(
                        InputKind::EscapedJSON,
                        raw_escaped,
                        raw_steps,
                        0.985,
                        false,
                    );
                    break;
                }
            }
        }

        if let Some(jsonp) = strip_jsonp(&trimmed) {
            append(
                InputKind::Jsonp,
                jsonp,
                vec![step(
                    "strip-jsonp",
                    "Removed the JSONP function wrapper",
                    true,
                )],
                0.98,
                false,
            );
        }

        if self.classifier.is_ndjson(&trimmed) {
            let lines: Vec<String> = split_newlines_omit_empty(&trimmed)
                .into_iter()
                .map(|line| json_trim_whitespace(&line))
                .collect();
            append(
                InputKind::Ndjson,
                format!("[{}]", lines.join(",")),
                vec![step(
                    "detect-ndjson",
                    "Wrapped newline-delimited JSON values in an array",
                    true,
                )],
                0.98,
                false,
            );
        }

        if let Some(decoded_url) = json_percent_decode(&trimmed) {
            append(
                InputKind::UrlEncoded,
                decoded_url,
                vec![step(
                    "url-decode",
                    "Found a URL-decoded JSON candidate",
                    false,
                )],
                0.94,
                true,
            );
        }

        if let Some(decoded_base64) = json_base64_decode(&trimmed) {
            append(
                InputKind::Base64Encoded,
                decoded_base64,
                vec![step(
                    "base64-decode",
                    "Found a Base64-decoded JSON candidate",
                    false,
                )],
                0.91,
                true,
            );
        }

        if let Some(embedded) = json_extract_balanced_json(&trimmed) {
            append(
                InputKind::EmbeddedJSON,
                embedded,
                vec![step(
                    "extract-embedded-json",
                    "Extracted a balanced JSON value from surrounding text",
                    false,
                )],
                0.86,
                true,
            );
        }

        candidates.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.steps.len().cmp(&a.steps.len()))
        });
        candidates
    }
}

impl Default for CandidateNormalizer {
    fn default() -> Self {
        Self::new()
    }
}

fn step(kind: &str, summary: &str, automatic: bool) -> NormalizationStep {
    NormalizationStep::new(kind, summary, automatic, true)
}

fn strip_markdown_fence(source: &str) -> Option<String> {
    if !source.starts_with("```") {
        return None;
    }
    if !source.ends_with("```") {
        return None;
    }
    let first_newline = source.find('\n')?;
    let content_start = first_newline + 1;
    let closing_start = source.len().checked_sub(3)?;
    if content_start > closing_start {
        return None;
    }
    let content = json_trim_whitespace(&source[content_start..closing_start]);
    if content.is_empty() {
        None
    } else {
        Some(content)
    }
}

fn strip_jsonp(source: &str) -> Option<String> {
    let open = source.find('(')?;
    if !(source.ends_with(')') || source.ends_with(");")) {
        return None;
    }
    let prefix = &source[..open];
    if prefix.is_empty()
        || !prefix
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
    {
        return None;
    }
    let close_offset = if source.ends_with(");") { 2 } else { 1 };
    let close = source.len() - close_offset;
    let content_start = open + 1;
    if content_start > close {
        return None;
    }
    Some(json_trim_whitespace(&source[content_start..close]))
}

fn json_looks_like_raw_escaped_container(source: &str) -> bool {
    let mut chars = source.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first != '{' && first != '[' {
        return false;
    }
    for character in chars {
        if character.is_whitespace() {
            continue;
        }
        return character == '\\';
    }
    false
}

fn json_remove_one_raw_escape_layer(source: &str) -> Option<String> {
    let characters: Vec<char> = source.chars().collect();
    let mut output: Vec<char> = Vec::new();
    let mut index = 0;
    let mut changed = false;

    while index < characters.len() {
        if characters[index] != '\\' {
            output.push(characters[index]);
            index += 1;
            continue;
        }

        let mut run_end = index;
        while run_end < characters.len() && characters[run_end] == '\\' {
            run_end += 1;
        }

        if run_end >= characters.len() || characters[run_end] != '"' {
            output.extend_from_slice(&characters[index..run_end]);
            index = run_end;
            continue;
        }

        for _ in 0..(run_end - index - 1) {
            output.push('\\');
        }
        output.push('"');
        changed = true;
        index = run_end + 1;
    }

    if changed {
        Some(output.into_iter().collect())
    } else {
        None
    }
}

pub fn json_percent_decode(source: &str) -> Option<String> {
    let bytes = source.as_bytes();
    let mut output: Vec<u8> = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == 37 {
            if index + 2 >= bytes.len() {
                return None;
            }
            let high = hex_byte(bytes[index + 1])?;
            let low = hex_byte(bytes[index + 2])?;
            output.push(high * 16 + low);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }

    let decoded = String::from_utf8(output.clone()).ok()?;
    if decoded.as_bytes() != output.as_slice() || decoded == source {
        return None;
    }
    Some(decoded)
}

pub fn json_base64_decode(source: &str) -> Option<String> {
    let compact: String = source.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() < 4 || compact.len() % 4 != 0 {
        return None;
    }

    let alphabet: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        .chars()
        .collect();
    let mut values: Vec<u8> = Vec::new();
    for character in compact.chars() {
        if character == '=' {
            values.push(64);
        } else if let Some(index) = alphabet.iter().position(|&c| c == character) {
            values.push(index as u8);
        } else if character == '-' {
            values.push(62);
        } else if character == '_' {
            values.push(63);
        } else {
            return None;
        }
    }

    let mut output: Vec<u8> = Vec::new();
    for offset in (0..values.len()).step_by(4) {
        let chunk = &values[offset..offset + 4];
        if chunk[0] >= 64 || chunk[1] >= 64 {
            return None;
        }
        output.push((chunk[0] << 2) | (chunk[1] >> 4));
        if chunk[2] < 64 {
            output.push((chunk[1] << 4) | (chunk[2] >> 2));
            if chunk[3] < 64 {
                output.push((chunk[2] << 6) | chunk[3]);
            } else if chunk[3] != 64 {
                return None;
            }
        } else if chunk[2] != 64 || chunk[3] != 64 {
            return None;
        }
    }

    let decoded = String::from_utf8(output.clone()).ok()?;
    if decoded.as_bytes() != output.as_slice() || decoded == source {
        return None;
    }
    Some(decoded)
}

fn hex_byte(byte: u8) -> Option<u8> {
    match byte {
        48..=57 => Some(byte - 48),
        65..=70 => Some(byte - 55),
        97..=102 => Some(byte - 87),
        _ => None,
    }
}

pub fn json_extract_balanced_json(source: &str) -> Option<String> {
    let chars: Vec<(usize, char)> = source.char_indices().collect();
    let start_pos = chars.iter().position(|(_, c)| *c == '{' || *c == '[')?;
    let start_byte = chars[start_pos].0;

    let mut stack: Vec<char> = Vec::new();
    let mut in_string = false;
    let mut escaped = false;

    for (i, &(_, character)) in chars.iter().enumerate() {
        if i < start_pos {
            continue;
        }
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }

        if character == '"' {
            in_string = true;
            continue;
        }
        if character == '{' || character == '[' {
            stack.push(character);
        } else if character == '}' || character == ']' {
            let open = stack.pop()?;
            let matches = (open == '{' && character == '}') || (open == '[' && character == ']');
            if !matches {
                return None;
            }
            if stack.is_empty() {
                let end_byte = chars[i].0 + character.len_utf8();
                return Some(source[start_byte..end_byte].to_string());
            }
        }
    }

    None
}
