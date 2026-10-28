//! URL percent-encode / decode engine with `application/x-www-form-urlencoded` behavior.
//!
//! Decode: `%XX` → byte, `+` → space. Encode: space → `+`, other non-unreserved bytes → `%XX`.
//! Also exposes lightweight URL / query-string structure parsing for readable decode results.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlCodecMode {
    Decode,
    Encode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryParam {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlStructure {
    pub scheme: Option<String>,
    pub host: Option<String>,
    pub path: Option<String>,
    pub query_params: Vec<QueryParam>,
}

impl UrlStructure {
    pub fn has_parts(&self) -> bool {
        self.scheme.is_some() || self.host.is_some() || self.path.is_some()
    }

    pub fn has_query(&self) -> bool {
        !self.query_params.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlCodecAnalysis {
    pub input: String,
    pub mode: UrlCodecMode,
    pub output: Option<String>,
    pub diagnostic: Option<String>,
    /// Present on successful decode when the output looks like a URL or query string.
    pub structure: Option<UrlStructure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlCodecError {
    pub message: String,
}

impl std::fmt::Display for UrlCodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for UrlCodecError {}

pub struct UrlCodecEngine;

impl Default for UrlCodecEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl UrlCodecEngine {
    pub fn new() -> Self {
        Self
    }

    /// Primary entry point used by the workbench. Default / primary action leans decode.
    pub fn convert(&self, input: &str, mode: UrlCodecMode) -> UrlCodecAnalysis {
        if input.is_empty() {
            return Self::failure(input, mode, "Enter text to encode or decode.");
        }

        let result = match mode {
            UrlCodecMode::Decode => decode(input),
            UrlCodecMode::Encode => encode(input),
        };

        match result {
            Ok(output) => {
                let structure = match mode {
                    UrlCodecMode::Decode => structure_for_decode(input, &output),
                    UrlCodecMode::Encode => None,
                };
                UrlCodecAnalysis {
                    input: input.to_string(),
                    mode,
                    output: Some(output),
                    diagnostic: None,
                    structure,
                }
            }
            Err(err) => Self::failure(input, mode, &err.message),
        }
    }

    fn failure(input: &str, mode: UrlCodecMode, diagnostic: &str) -> UrlCodecAnalysis {
        UrlCodecAnalysis {
            input: input.to_string(),
            mode,
            output: None,
            diagnostic: Some(diagnostic.to_string()),
            structure: None,
        }
    }
}

/// Percent-encode `input` using `application/x-www-form-urlencoded` rules.
///
/// Unreserved characters (`A-Z` `a-z` `0-9` `-` `.` `_` `~`) are left as-is.
/// Spaces become `+`. All other UTF-8 bytes become uppercase `%XX`.
pub fn encode(input: &str) -> Result<String, UrlCodecError> {
    let mut out = String::with_capacity(input.len());
    for &byte in input.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push(HEX[(byte >> 4) as usize] as char);
                out.push(HEX[(byte & 0x0f) as usize] as char);
            }
        }
    }
    Ok(out)
}

/// Percent-decode `input` using `application/x-www-form-urlencoded` rules.
///
/// `+` becomes space. `%XX` (hex, case-insensitive) becomes the corresponding byte.
/// Incomplete or invalid escapes, and non-UTF-8 results, return an error.
pub fn decode(input: &str) -> Result<String, UrlCodecError> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' => {
                if i + 2 >= bytes.len() {
                    return Err(UrlCodecError {
                        message: "Incomplete percent-escape sequence.".to_string(),
                    });
                }
                let hi = from_hex(bytes[i + 1]).ok_or_else(|| UrlCodecError {
                    message: "Invalid percent-escape sequence.".to_string(),
                })?;
                let lo = from_hex(bytes[i + 2]).ok_or_else(|| UrlCodecError {
                    message: "Invalid percent-escape sequence.".to_string(),
                })?;
                out.push((hi << 4) | lo);
                i += 3;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| UrlCodecError {
        message: "Decoded bytes are not valid UTF-8.".to_string(),
    })
}

/// Build the structured view for a successful decode.
///
/// Prefer parsing the *raw* input when it already has visible URL / query
/// structure (e.g. `/v1/x?cb=https%3A%2F%2Fh%3Fu%3D1%26v%3D2&os=0`): splitting
/// before decoding keeps percent-encoded `&` / `=` / `?` inside a value instead
/// of promoting them to top-level params. Each component is decoded afterwards.
/// Falls back to parsing the decoded text (fully-encoded input).
pub fn structure_for_decode(input: &str, decoded: &str) -> Option<UrlStructure> {
    let raw = input.trim();
    let raw_structure = if raw != decoded.trim() {
        parse_structured(raw).map(|mut structure| {
            let soft = |s: String| decode(&s).unwrap_or(s);
            structure.host = structure.host.map(soft);
            structure.path = structure.path.map(soft);
            structure
        })
    } else {
        None
    };
    match raw_structure {
        // Raw split found real params: most faithful view.
        Some(structure) if structure.has_query() => Some(structure),
        // e.g. `/v1%2Fx%3Fa%3D1`: the query only appears after decoding.
        raw_structure => parse_structured(decoded).or(raw_structure),
    }
}

/// Parse text as a URL, relative URL / path, or bare query string for structured display.
///
/// Recognised shapes, in order:
/// 1. Absolute URLs (`scheme://host/path?query#frag`).
/// 2. Protocol-relative (`//host/path?query`), path-only / relative URLs
///    (`/v1/x?a=b`, `api/x?a=b`, `?a=b`): split into path + query.
/// 3. Bare query strings (`a=b&c=d`).
///
/// The path / query split always happens on the FIRST `?`, so a nested URL inside
/// a value (`callback=https://h/cb?x=1`) stays inside that value.
/// Returns `None` for plain prose.
pub fn parse_structured(text: &str) -> Option<UrlStructure> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    if let Some(structure) = parse_as_url(text) {
        return Some(structure);
    }

    if let Some(structure) = parse_as_relative(text) {
        return Some(structure);
    }

    if looks_like_query_string(text) {
        let query_params = parse_query_string(text);
        if query_params.is_empty() {
            return None;
        }
        return Some(UrlStructure {
            scheme: None,
            host: None,
            path: None,
            query_params,
        });
    }

    None
}

fn parse_as_url(text: &str) -> Option<UrlStructure> {
    let scheme_end = text.find(':')?;
    if scheme_end == 0 {
        return None;
    }
    let scheme = &text[..scheme_end];
    if !is_valid_scheme(scheme) {
        return None;
    }

    let rest = &text[scheme_end + 1..];
    // Not a hierarchical URL (e.g. `mailto:`): no structured view.
    let after_slash = rest.strip_prefix("//")?;
    parse_hierarchical(Some(scheme), after_slash)
}

/// Parse `authority/path?query#frag` (the part after `scheme://` or `//`).
fn parse_hierarchical(scheme: Option<&str>, after_slash: &str) -> Option<UrlStructure> {
    let (authority, path_and_query) = split_authority(after_slash);

    let host = {
        let host = authority
            .split('@')
            .next_back()
            .unwrap_or(authority)
            .trim();
        if host.is_empty() {
            None
        } else {
            Some(host.to_string())
        }
    };

    let (path, query) = split_path_query(path_and_query);
    let path = if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    };
    let query_params = query.map(parse_query_string).unwrap_or_default();

    // Require at least a host or path or query so we don't claim structure for `http://`.
    if host.is_none() && path.is_none() && query_params.is_empty() {
        return None;
    }

    Some(UrlStructure {
        scheme: scheme.map(str::to_string),
        host,
        path,
        query_params,
    })
}

/// Relative URL without scheme: `//host/...`, `/path?query`, `path?query`, `?query`.
fn parse_as_relative(text: &str) -> Option<UrlStructure> {
    if let Some(rest) = text.strip_prefix("//") {
        if rest.starts_with('/') || rest.chars().any(char::is_whitespace) {
            return None;
        }
        return parse_hierarchical(None, rest);
    }

    let first_q = text.find('?');
    let first_eq = text.find('=');
    // `?` before the first `=` means everything before it is a path, not a key.
    let query_before_eq = matches!((first_q, first_eq), (Some(q), Some(e)) if q < e);
    let starts_with_slash = text.starts_with('/');
    if !starts_with_slash && !query_before_eq {
        return None;
    }

    let (path, query) = split_path_query(text);
    if path.chars().any(char::is_whitespace) {
        return None;
    }
    let query_params = query.map(parse_query_string).unwrap_or_default();
    // `abc?` / prose ending in `?` without any params is not a URL.
    if !starts_with_slash && query_params.is_empty() {
        return None;
    }
    let path = if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    };
    if path.is_none() && query_params.is_empty() {
        return None;
    }
    Some(UrlStructure {
        scheme: None,
        host: None,
        path,
        query_params,
    })
}

/// Split on the FIRST `?` only; strip a trailing `#fragment`.
fn split_path_query(path_and_query: &str) -> (&str, Option<&str>) {
    match path_and_query.split_once('?') {
        Some((path, query_and_frag)) => {
            let query = query_and_frag.split('#').next().unwrap_or(query_and_frag);
            (path, Some(query))
        }
        None => {
            let path = path_and_query.split('#').next().unwrap_or(path_and_query);
            (path, None)
        }
    }
}

fn split_authority(after_slash: &str) -> (&str, &str) {
    // Authority ends at first `/`, `?`, or `#`.
    let end = after_slash
        .find(|c| c == '/' || c == '?' || c == '#')
        .unwrap_or(after_slash.len());
    (&after_slash[..end], &after_slash[end..])
}

fn is_valid_scheme(scheme: &str) -> bool {
    let mut chars = scheme.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
}

fn looks_like_query_string(text: &str) -> bool {
    if text.contains(' ') && !text.contains('=') {
        return false;
    }
    // Must look like form pairs, not a sentence.
    if !text.contains('=') {
        return false;
    }
    // Reject if it already looked like a URL scheme attempt without //.
    if let Some(idx) = text.find(':') {
        let scheme = &text[..idx];
        if is_valid_scheme(scheme) && text[idx + 1..].starts_with("//") {
            return false;
        }
    }
    // Every `&`-separated segment should be a key or key=value (allow empty value).
    text.split('&').all(|part| {
        if part.is_empty() {
            return true;
        }
        // Keys should not contain whitespace for a "query-like" bare string.
        let key = part.split_once('=').map(|(k, _)| k).unwrap_or(part);
        !key.is_empty() && !key.contains(' ')
    })
}

/// Split `application/x-www-form-urlencoded` query text into params.
/// Keeps duplicate keys. Values may be empty. Optional leading `?` is stripped.
pub fn parse_query_string(query: &str) -> Vec<QueryParam> {
    let query = query.strip_prefix('?').unwrap_or(query);
    if query.is_empty() {
        return Vec::new();
    }

    let mut params = Vec::new();
    for part in query.split('&') {
        if part.is_empty() {
            continue;
        }
        let (raw_key, raw_value) = match part.split_once('=') {
            Some((k, v)) => (k, v),
            None => (part, ""),
        };
        // Soft-decode each component; fall back to raw on failure so the table still shows.
        let key = decode(raw_key).unwrap_or_else(|_| raw_key.to_string());
        let value = decode(raw_value).unwrap_or_else(|_| raw_value.to_string());
        params.push(QueryParam { key, value });
    }
    params
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
