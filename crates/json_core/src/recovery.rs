use crate::diagnostics::Diagnostic;
use crate::normalization::{
    json_is_valid_json, json_trim_whitespace, split_newlines_omit_empty, NormalizationStep,
};
use crate::strict_parser::StrictJSONParser;

#[derive(Clone, Debug, PartialEq)]
pub struct RecoveryResult {
    pub repaired_text: String,
    pub steps: Vec<NormalizationStep>,
    pub diagnostics: Vec<Diagnostic>,
}

impl RecoveryResult {
    pub fn new(
        repaired_text: impl Into<String>,
        steps: Vec<NormalizationStep>,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        Self {
            repaired_text: repaired_text.into(),
            steps,
            diagnostics,
        }
    }
}

pub struct RecoveryParser;

impl RecoveryParser {
    pub fn new() -> Self {
        Self
    }

    pub fn repair(&self, source: &str) -> Result<RecoveryResult, Diagnostic> {
        let trimmed = json_trim_whitespace(source);
        if json_is_valid_json(&trimmed) {
            return Ok(RecoveryResult::new(trimmed, Vec::new(), Vec::new()));
        }

        if let Some(ndjson) = recover_ndjson(&trimmed) {
            return Ok(ndjson);
        }

        let mut working = trimmed;
        let mut steps: Vec<NormalizationStep> = Vec::new();
        let diagnostics: Vec<Diagnostic> = Vec::new();

        if let Some(jsonp) = strip_jsonp_wrapper(&working) {
            working = json_trim_whitespace(&jsonp);
            steps.push(step("strip-jsonp", "Removed the JSONP function wrapper"));
        }

        let comment_result = remove_comments(&working)?;
        working = comment_result.0;
        if comment_result.1 {
            steps.push(step(
                "strip-comments",
                "Removed comments outside JSON strings",
            ));
        }

        let token_result = normalize_loose_tokens(&working)?;
        working = token_result.0;
        steps.extend(token_result.1);

        let comma_result = remove_trailing_commas(&working);
        working = comma_result.0;
        if comma_result.1 > 0 {
            steps.push(step(
                "strip-trailing-commas",
                &format!("Removed {} trailing comma(s)", comma_result.1),
            ));
        }

        let closure_result = close_unfinished_containers(&working)?;
        working = closure_result.0;
        if closure_result.1 > 0 {
            steps.push(step(
                "close-containers",
                &format!("Closed {} unclosed container(s)", closure_result.1),
            ));
        }

        if let Err(diagnostic) = StrictJSONParser::new().parse(&working) {
            return Err(diagnostic);
        }

        let compacted = compact_json_text(&working);
        if compacted != working {
            working = compacted;
            steps.push(step(
                "compact-repaired-json",
                "Removed whitespace outside JSON strings",
            ));
        }

        Ok(RecoveryResult::new(working, steps, diagnostics))
    }
}

impl Default for RecoveryParser {
    fn default() -> Self {
        Self::new()
    }
}

fn recover_ndjson(source: &str) -> Option<RecoveryResult> {
    let lines: Vec<String> = split_newlines_omit_empty(source)
        .into_iter()
        .map(|line| json_trim_whitespace(&line))
        .collect();
    if lines.len() <= 1 || !lines.iter().all(|line| json_is_valid_json(line)) {
        return None;
    }
    Some(RecoveryResult::new(
        format!("[{}]", lines.join(",")),
        vec![step(
            "detect-ndjson",
            "Wrapped newline-delimited JSON values in an array",
        )],
        Vec::new(),
    ))
}

fn remove_comments(source: &str) -> Result<(String, bool), Diagnostic> {
    let characters: Vec<char> = source.chars().collect();
    let mut output: Vec<char> = Vec::new();
    let mut index = 0;
    let mut removed = false;

    while index < characters.len() {
        let character = characters[index];
        if character == '"' || character == '\'' {
            let quote = character;
            output.push(character);
            index += 1;
            let mut escaped = false;
            while index < characters.len() {
                let current = characters[index];
                output.push(current);
                index += 1;
                if escaped {
                    escaped = false;
                } else if current == '\\' {
                    escaped = true;
                } else if current == quote {
                    break;
                }
            }
            continue;
        }

        if character == '/' && index + 1 < characters.len() && characters[index + 1] == '/' {
            removed = true;
            index += 2;
            while index < characters.len() && !is_swift_newline(characters[index]) {
                index += 1;
            }
            continue;
        }

        if character == '/' && index + 1 < characters.len() && characters[index + 1] == '*' {
            removed = true;
            index += 2;
            let mut closed = false;
            while index + 1 < characters.len() {
                if characters[index] == '*' && characters[index + 1] == '/' {
                    index += 2;
                    closed = true;
                    break;
                }
                index += 1;
            }
            if !closed {
                return Err(Diagnostic::new("Unterminated block comment").with_location(1, 1));
            }
            continue;
        }

        output.push(character);
        index += 1;
    }

    Ok((output.into_iter().collect(), removed))
}

fn normalize_loose_tokens(source: &str) -> Result<(String, Vec<NormalizationStep>), Diagnostic> {
    let characters: Vec<char> = source.chars().collect();
    let mut output: Vec<char> = Vec::new();
    let mut index = 0;
    let mut steps: Vec<NormalizationStep> = Vec::new();

    while index < characters.len() {
        let character = characters[index];

        if character == '"' {
            index = append_double_quoted_string(&characters, index, &mut output)?;
            continue;
        }

        if character == '\'' {
            index = append_single_quoted_string(&characters, index, &mut output)?;
            steps.push(step(
                "normalize-single-quotes",
                "Converted a single-quoted string to JSON string syntax",
            ));
            continue;
        }

        if is_identifier_start(character) {
            let prev = previous_significant_character(&output);
            if prev == Some('{') || prev == Some(',') {
                let end = identifier_end(&characters, index);
                let next = next_significant_character(&characters, end);
                if next == Some(':') {
                    output.push('"');
                    output.extend_from_slice(&characters[index..end]);
                    output.push('"');
                    steps.push(step(
                        "quote-object-key",
                        "Added quotes around an unquoted object key",
                    ));
                    index = end;
                    continue;
                }
            }
        }

        output.push(character);
        index += 1;
    }

    Ok((output.into_iter().collect(), steps))
}

fn append_double_quoted_string(
    characters: &[char],
    start: usize,
    output: &mut Vec<char>,
) -> Result<usize, Diagnostic> {
    let mut index = start;
    let mut escaped = false;
    while index < characters.len() {
        let character = characters[index];
        output.push(character);
        index += 1;
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return Ok(index);
        }
    }
    Err(Diagnostic::new("Unterminated string").with_location(1, 1))
}

fn append_single_quoted_string(
    characters: &[char],
    start: usize,
    output: &mut Vec<char>,
) -> Result<usize, Diagnostic> {
    output.push('"');
    let mut index = start + 1;
    while index < characters.len() {
        let character = characters[index];
        if character == '\'' {
            output.push('"');
            return Ok(index + 1);
        }
        if character == '\\' {
            if index + 1 >= characters.len() {
                return Err(
                    Diagnostic::new("Unterminated single-quoted string").with_location(1, 1)
                );
            }
            let escaped = characters[index + 1];
            if escaped == '\'' {
                output.push('\'');
            } else if escaped == '"' {
                output.push('\\');
                output.push('"');
            } else {
                output.push('\\');
                output.push(escaped);
            }
            index += 2;
            continue;
        }
        if is_swift_newline(character) {
            output.push('\\');
            output.push('n');
        } else if character == '"' {
            output.push('\\');
            output.push('"');
        } else {
            output.push(character);
        }
        index += 1;
    }
    Err(Diagnostic::new("Unterminated single-quoted string").with_location(1, 1))
}

fn remove_trailing_commas(source: &str) -> (String, usize) {
    let characters: Vec<char> = source.chars().collect();
    let mut output: Vec<char> = Vec::new();
    let mut index = 0;
    let mut removed = 0;

    while index < characters.len() {
        if characters[index] == '"' {
            index = append_double_quoted_string(&characters, index, &mut output)
                .unwrap_or(characters.len());
            continue;
        }
        if characters[index] == ',' {
            let mut next = index + 1;
            while next < characters.len() && characters[next].is_whitespace() {
                next += 1;
            }
            if next < characters.len() && (characters[next] == '}' || characters[next] == ']') {
                removed += 1;
                index += 1;
                continue;
            }
        }
        output.push(characters[index]);
        index += 1;
    }

    (output.into_iter().collect(), removed)
}

fn close_unfinished_containers(source: &str) -> Result<(String, usize), Diagnostic> {
    let characters: Vec<char> = source.chars().collect();
    let mut stack: Vec<char> = Vec::new();
    let mut index = 0;

    while index < characters.len() {
        if characters[index] == '"' {
            let mut discarded: Vec<char> = Vec::new();
            index = append_double_quoted_string(&characters, index, &mut discarded)?;
            continue;
        }

        match characters[index] {
            '{' | '[' => stack.push(characters[index]),
            '}' => {
                if stack.pop() != Some('{') {
                    return Err(Diagnostic::new("Unexpected closing brace").with_location(1, 1));
                }
            }
            ']' => {
                if stack.pop() != Some('[') {
                    return Err(Diagnostic::new("Unexpected closing bracket").with_location(1, 1));
                }
            }
            _ => {}
        }
        index += 1;
    }

    let mut output = source.to_string();
    let closed = stack.len();
    for open in stack.into_iter().rev() {
        output.push(if open == '{' { '}' } else { ']' });
    }
    Ok((output, closed))
}

fn strip_jsonp_wrapper(source: &str) -> Option<String> {
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
    let start = open + 1;
    Some(source[start..close].to_string())
}

fn previous_significant_character(output: &[char]) -> Option<char> {
    output.iter().rev().copied().find(|c| !c.is_whitespace())
}

fn next_significant_character(characters: &[char], start: usize) -> Option<char> {
    let mut index = start;
    while index < characters.len() && characters[index].is_whitespace() {
        index += 1;
    }
    characters.get(index).copied()
}

fn identifier_end(characters: &[char], start: usize) -> usize {
    let mut index = start;
    while index < characters.len() && is_identifier_part(characters[index]) {
        index += 1;
    }
    index
}

fn is_identifier_start(character: char) -> bool {
    character == '_' || character.is_alphabetic()
}

fn is_identifier_part(character: char) -> bool {
    is_identifier_start(character) || character.is_numeric() || character == '-'
}

fn step(kind: &str, summary: &str) -> NormalizationStep {
    NormalizationStep::new(kind, summary, true, true)
}

fn compact_json_text(source: &str) -> String {
    let mut output: Vec<char> = Vec::new();
    let mut in_string = false;
    let mut escaped = false;

    for character in source.chars() {
        if in_string {
            output.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
        } else if character == '"' {
            in_string = true;
            output.push(character);
        } else if !character.is_whitespace() {
            output.push(character);
        }
    }

    output.into_iter().collect()
}

fn is_swift_newline(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r' | '\u{000B}' | '\u{000C}' | '\u{0085}' | '\u{2028}' | '\u{2029}'
    )
}
