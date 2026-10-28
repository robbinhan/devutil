//! Full-text search for the workbench.
//!
//! Matches are found in the whole string, off the engine thread when that
//! string is large. NSH 0.1.1 can select a UTF-16 range, but on macOS that
//! selection is invisible while the find field is focused, and
//! `set_selection` never scrolls. The field text therefore always starts at
//! the current match's line, and [`snippets`] draws every hit on the nearby
//! lines (the current one stronger) under the find bar.

use std::time::Duration;

use nsh_core::ui::prelude::*;

/// Stored hits. A pattern that matches every byte of a multi-megabyte
/// document is not kept in full.
pub const HIT_CAP: usize = 50_000;

/// Bytes searched on the engine thread. Larger documents use a worker.
pub const INLINE_BYTES: usize = 64 * 1024;

/// Wait after a keystroke before searching again.
pub const DEBOUNCE: Duration = Duration::from_millis(200);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextHit {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindOutcome {
    pub hits: Vec<TextHit>,
    pub truncated: bool,
    pub error: Option<String>,
}

impl FindOutcome {
    fn empty() -> Self {
        Self {
            hits: Vec::new(),
            truncated: false,
            error: None,
        }
    }

    fn invalid(message: String) -> Self {
        Self {
            hits: Vec::new(),
            truncated: false,
            error: Some(message),
        }
    }
}

/// A compiled query. Empty patterns match nothing: a regex of `""` would
/// otherwise report a hit between every character.
pub struct Needle {
    inner: Inner,
}

enum Inner {
    Empty,
    Literal { text: String, case_sensitive: bool },
    Pattern(regex::Regex),
}

impl Needle {
    pub fn compile(pattern: &str, case_sensitive: bool, regex_mode: bool) -> Result<Self, String> {
        if pattern.is_empty() {
            return Ok(Self { inner: Inner::Empty });
        }
        if regex_mode || (!case_sensitive && !pattern.is_ascii()) {
            let source = if regex_mode {
                pattern.to_string()
            } else {
                regex::escape(pattern)
            };
            let re = regex::RegexBuilder::new(&source)
                .case_insensitive(!case_sensitive)
                .size_limit(1 << 20)
                .build()
                .map_err(|err| err.to_string())?;
            return Ok(Self {
                inner: Inner::Pattern(re),
            });
        }
        Ok(Self {
            inner: Inner::Literal {
                text: pattern.to_string(),
                case_sensitive,
            },
        })
    }

    pub fn is_empty(&self) -> bool {
        matches!(self.inner, Inner::Empty)
    }

    pub fn matches(&self, text: &str) -> bool {
        match &self.inner {
            Inner::Empty => false,
            Inner::Literal {
                text: needle,
                case_sensitive: true,
            } => text.contains(needle),
            Inner::Literal {
                text: needle,
                case_sensitive: false,
            } => contains_ascii_ignore_case(text, needle),
            Inner::Pattern(re) => re.is_match(text),
        }
    }

    pub fn find(&self, text: &str) -> FindOutcome {
        self.find_limited(text, HIT_CAP)
    }

    pub fn find_limited(&self, text: &str, cap: usize) -> FindOutcome {
        if cap == 0 || self.is_empty() {
            return FindOutcome::empty();
        }
        match &self.inner {
            Inner::Empty => FindOutcome::empty(),
            Inner::Literal {
                text: needle,
                case_sensitive,
            } => find_literal(text, needle, *case_sensitive, cap),
            Inner::Pattern(re) => find_regex(text, re, cap),
        }
    }
}

pub fn search(
    text: &str,
    pattern: &str,
    case_sensitive: bool,
    regex_mode: bool,
) -> FindOutcome {
    match Needle::compile(pattern, case_sensitive, regex_mode) {
        Ok(needle) => needle.find(text),
        Err(message) => FindOutcome::invalid(message),
    }
}

fn find_literal(text: &str, needle: &str, case_sensitive: bool, cap: usize) -> FindOutcome {
    if needle.is_empty() {
        return FindOutcome::empty();
    }
    let mut hits = Vec::new();
    if case_sensitive {
        let mut from = 0;
        while from <= text.len() {
            let Some(at) = text[from..].find(needle) else {
                break;
            };
            let start = from + at;
            let end = start + needle.len();
            hits.push(TextHit { start, end });
            if hits.len() == cap {
                let truncated = text[end..].contains(needle);
                return FindOutcome {
                    hits,
                    truncated,
                    error: None,
                };
            }
            from = end;
        }
    } else {
        let hay = text.as_bytes();
        let needle_bytes = needle.as_bytes();
        let mut i = 0;
        while i + needle_bytes.len() <= hay.len() {
            if hay[i..i + needle_bytes.len()].eq_ignore_ascii_case(needle_bytes) {
                let end = i + needle_bytes.len();
                hits.push(TextHit { start: i, end });
                if hits.len() == cap {
                    let truncated = (end + needle_bytes.len() <= hay.len())
                        && hay[end..]
                            .windows(needle_bytes.len())
                            .any(|w| w.eq_ignore_ascii_case(needle_bytes));
                    return FindOutcome {
                        hits,
                        truncated,
                        error: None,
                    };
                }
                i = end;
            } else {
                i += 1;
            }
        }
    }
    FindOutcome {
        hits,
        truncated: false,
        error: None,
    }
}

fn find_regex(text: &str, re: &regex::Regex, cap: usize) -> FindOutcome {
    let mut hits = Vec::new();
    let mut from = 0;
    while from <= text.len() {
        let Some(found) = re.find_at(text, from) else {
            break;
        };
        if found.range().is_empty() {
            let skip = text[found.start()..]
                .chars()
                .next()
                .map(char::len_utf8)
                .unwrap_or(1);
            from = found.start() + skip;
            continue;
        }
        hits.push(TextHit {
            start: found.start(),
            end: found.end(),
        });
        if hits.len() == cap {
            let truncated = re.find_at(text, found.end()).is_some_and(|next| !next.range().is_empty());
            return FindOutcome {
                hits,
                truncated,
                error: None,
            };
        }
        from = found.end();
    }
    FindOutcome {
        hits,
        truncated: false,
        error: None,
    }
}

fn contains_ascii_ignore_case(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let hay = haystack.as_bytes();
    let needle = needle.as_bytes();
    hay.windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceKind {
    Plain,
    /// Another hit on the same line.
    Hit,
    /// The hit the count is pointing at.
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnippetPiece {
    pub text: String,
    pub kind: PieceKind,
}

/// One hit's line, with every hit on that line marked. Long lines keep a
/// window around the current (or first) hit so the preview stays small.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snippet {
    pub hit_index: usize,
    pub line: u32,
    pub pieces: Vec<SnippetPiece>,
}

/// Hits shown on either side of the current one.
pub const SNIPPET_RADIUS: usize = 4;

/// Up to `radius` hits on either side of `current`, one row per hit.
pub fn snippets(text: &str, hits: &[TextHit], current: usize, radius: usize) -> Vec<Snippet> {
    if hits.is_empty() {
        return Vec::new();
    }
    let current = current.min(hits.len() - 1);
    let from = current.saturating_sub(radius);
    let to = (current + radius + 1).min(hits.len());
    hits[from..to]
        .iter()
        .enumerate()
        .map(|(offset, hit)| {
            let index = from + offset;
            snippet_for(text, hits, index, hit)
        })
        .collect()
}

fn snippet_for(text: &str, hits: &[TextHit], index: usize, hit: &TextHit) -> Snippet {
    let start = hit.start.min(text.len());
    let line_at = text[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = text[start..]
        .find('\n')
        .map(|i| start + i)
        .unwrap_or(text.len());
    let line_no = (text[..start].bytes().filter(|b| *b == b'\n').count() + 1) as u32;
    let on_line: Vec<&TextHit> = hits
        .iter()
        .filter(|hit| hit.start >= line_at && hit.start < line_end && hit.end > hit.start)
        .collect();
    let line = &text[line_at..line_end];
    let focus = hit.start.saturating_sub(line_at).min(line.len());
    let focus_end = hit.end.saturating_sub(line_at).clamp(focus, line.len());
    let (slice_start, slice_end) = line_window(line, focus, focus_end);
    let base = line_at + slice_start;
    let end = line_at + slice_end;
    let mut pieces = Vec::new();
    if slice_start > 0 {
        pieces.push(SnippetPiece {
            text: "…".to_string(),
            kind: PieceKind::Plain,
        });
    }
    let mut cursor = base;
    for hit in on_line {
        let hit_start = hit.start.clamp(base, end);
        let hit_end = hit.end.clamp(base, end);
        if hit_end <= hit_start {
            continue;
        }
        if hit_start > cursor {
            pieces.push(SnippetPiece {
                text: slice_text(text, cursor, hit_start),
                kind: PieceKind::Plain,
            });
        }
        let kind = if hit.start == hits[index].start && hit.end == hits[index].end {
            PieceKind::Current
        } else {
            PieceKind::Hit
        };
        pieces.push(SnippetPiece {
            text: slice_text(text, hit_start, hit_end),
            kind,
        });
        cursor = hit_end;
    }
    if cursor < end {
        pieces.push(SnippetPiece {
            text: slice_text(text, cursor, end),
            kind: PieceKind::Plain,
        });
    }
    if slice_end < line_end - line_at {
        pieces.push(SnippetPiece {
            text: "…".to_string(),
            kind: PieceKind::Plain,
        });
    }
    Snippet {
        hit_index: index,
        line: line_no,
        pieces,
    }
}

fn line_window(line: &str, focus: usize, focus_end: usize) -> (usize, usize) {
    const LIMIT: usize = 160;
    let len = line.len();
    if len <= LIMIT {
        return (0, len);
    }
    let focus = floor_char(line, focus.min(len));
    let focus_end = ceil_char(line, focus_end.clamp(focus, len));
    let mut start = floor_char(line, focus.saturating_sub(40));
    let mut end = ceil_char(line, (start + LIMIT).min(len));
    if end < focus_end {
        end = focus_end.min(len);
        start = floor_char(line, end.saturating_sub(LIMIT));
    }
    if end > len {
        end = len;
    }
    (start, end.max(start))
}

fn slice_text(text: &str, start: usize, end: usize) -> String {
    let start = floor_char(text, start.min(text.len()));
    let end = ceil_char(text, end.clamp(start, text.len()));
    text[start..end].to_string()
}

fn ceil_char(text: &str, mut index: usize) -> usize {
    if index > text.len() {
        return text.len();
    }
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

/// Split `label` into plain and matching pieces for a tree row.
pub fn pieces(label: &str, pattern: &str, case_sensitive: bool, regex_mode: bool) -> Vec<SnippetPiece> {
    if pattern.is_empty() {
        return vec![SnippetPiece {
            text: label.to_string(),
            kind: PieceKind::Plain,
        }];
    }
    let outcome = search(label, pattern, case_sensitive, regex_mode);
    if outcome.hits.is_empty() {
        return vec![SnippetPiece {
            text: label.to_string(),
            kind: PieceKind::Plain,
        }];
    }
    let mut pieces = Vec::new();
    let mut cursor = 0;
    for hit in outcome.hits {
        if hit.start > cursor && hit.start <= label.len() {
            pieces.push(SnippetPiece {
                text: label[cursor..hit.start].to_string(),
                kind: PieceKind::Plain,
            });
        }
        if hit.end > hit.start && hit.end <= label.len() {
            pieces.push(SnippetPiece {
                text: label[hit.start..hit.end].to_string(),
                kind: PieceKind::Current,
            });
            cursor = hit.end;
        }
    }
    if cursor < label.len() {
        pieces.push(SnippetPiece {
            text: label[cursor..].to_string(),
            kind: PieceKind::Plain,
        });
    }
    pieces
}

pub fn status_label(
    query_empty: bool,
    busy: bool,
    error: &str,
    index: usize,
    total: usize,
    truncated: bool,
) -> String {
    if query_empty {
        return String::new();
    }
    if !error.is_empty() {
        return error.to_string();
    }
    if busy && total == 0 {
        return "Searching…".to_string();
    }
    if total == 0 {
        return "No matches".to_string();
    }
    let shown = index.saturating_add(1);
    if truncated {
        format!("{shown} / {total}+")
    } else {
        format!("{shown} / {total}")
    }
}

/// UTF-16 offsets of `start..end` inside `text`, for [`ElementRef::set_selection`].
/// The text of a UTF-16 selection. Used to check that a reveal highlights the hit.
#[cfg(test)]
pub fn slice_utf16(text: &str, start: u32, end: u32) -> String {
    let mut units = 0u32;
    let mut out = String::new();
    for ch in text.chars() {
        let next = units + ch.len_utf16() as u32;
        if next > start && units < end {
            out.push(ch);
        }
        units = next;
        if units >= end {
            break;
        }
    }
    out
}

pub fn byte_at_utf16(text: &str, units: usize) -> usize {
    let mut seen = 0usize;
    for (index, ch) in text.char_indices() {
        if seen >= units {
            return index;
        }
        seen += ch.len_utf16();
    }
    text.len()
}

pub fn utf16_span(text: &str, start: usize, end: usize) -> (u32, u32) {
    let start = floor_char(text, start.min(text.len()));
    let end = floor_char(text, end.clamp(start, text.len()));
    let units = |prefix: &str| prefix.chars().map(char::len_utf16).sum::<usize>() as u32;
    let from = units(&text[..start]);
    let to = from + units(&text[start..end]);
    (from, to)
}

fn floor_char(text: &str, mut index: usize) -> usize {
    if index > text.len() {
        index = text.len();
    }
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// Search state shared by every tool. Hits for a large document are filled
/// in from a worker; the label is what the bar shows.
#[derive(Clone, Copy)]
pub struct TextSearch {
    pub open: Signal<bool>,
    pub query: Signal<String>,
    pub case_sensitive: Signal<bool>,
    pub regex: Signal<bool>,
    pub hits: Signal<Vec<TextHit>>,
    pub truncated: Signal<bool>,
    pub error: Signal<String>,
    pub index: Signal<usize>,
    pub busy: Signal<bool>,
    pub generation: Signal<u64>,
    pub revision: Signal<u64>,
    pub focus: Signal<u64>,
    pub label: Signal<String>,
    /// Nearby lines, with every hit marked. Empty when find is closed or
    /// the tree is the thing being stepped through.
    pub snippets: Signal<Vec<Snippet>>,
}

impl TextSearch {
    pub fn new() -> Self {
        Self {
            open: signal(false),
            query: signal(String::new()),
            case_sensitive: signal(false),
            regex: signal(false),
            hits: signal(Vec::new()),
            truncated: signal(false),
            error: signal(String::new()),
            index: signal(0),
            busy: signal(false),
            generation: signal(0),
            revision: signal(0),
            focus: signal(0),
            label: signal(String::new()),
            snippets: signal(Vec::new()),
        }
    }

    /// Rebuild the rows under the find bar from the full document.
    pub fn publish_snippets(self, text: &str) {
        let built = self
            .hits
            .with(|hits| snippets(text, hits, self.index.get(), SNIPPET_RADIUS));
        self.snippets.set(built);
    }

    pub fn request_focus(self) {
        self.focus.update(|generation| *generation = generation.wrapping_add(1));
    }

    pub fn current(self) -> Option<TextHit> {
        self.hits.with(|hits| hits.get(self.index.get()).copied())
    }

    pub fn step(self, delta: isize) {
        let len = self.hits.with(Vec::len);
        if len == 0 {
            return;
        }
        let index = self.index.get() as isize;
        let next = (index + delta).rem_euclid(len as isize) as usize;
        self.index.set(next);
        self.publish_label();
    }

    /// Debounced search of whatever `document` returns when the timer fires,
    /// so a keystroke does not clone a multi-megabyte string.
    pub fn schedule(self, document: impl Fn() -> String + Copy + 'static) {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        if self.query.with(String::is_empty) {
            self.clear_hits();
            return;
        }
        set_timeout(DEBOUNCE, move || {
            if self.generation.get() != generation {
                return;
            }
            let text = document();
            let pattern = self.query.get();
            let case_sensitive = self.case_sensitive.get();
            let regex_mode = self.regex.get();
            if text.len() <= INLINE_BYTES {
                self.apply(generation, search(&text, &pattern, case_sensitive, regex_mode));
                return;
            }
            self.busy.set(true);
            self.publish_label();
            spawn(async move {
                let outcome = blocking(move || search(&text, &pattern, case_sensitive, regex_mode)).await;
                if self.generation.get() != generation {
                    return;
                }
                match outcome {
                    Ok(outcome) => self.apply(generation, outcome),
                    Err(err) => {
                        self.error.set(err.to_string());
                        self.busy.set(false);
                        self.publish_label();
                    }
                }
            });
        });
    }

    fn apply(self, generation: u64, outcome: FindOutcome) {
        if self.generation.get() != generation {
            return;
        }
        self.hits.set(outcome.hits);
        self.truncated.set(outcome.truncated);
        self.error.set(outcome.error.unwrap_or_default());
        self.index.set(0);
        self.busy.set(false);
        self.revision.update(|revision| *revision = revision.wrapping_add(1));
        self.publish_label();
    }

    pub fn clear_hits(self) {
        self.hits.set(Vec::new());
        self.snippets.set(Vec::new());
        self.truncated.set(false);
        self.error.set(String::new());
        self.index.set(0);
        self.busy.set(false);
        self.revision.update(|revision| *revision = revision.wrapping_add(1));
        self.publish_label();
    }

    pub fn publish_label(self) {
        let label = self.query.with(|query| {
            self.error.with(|error| {
                self.hits.with(|hits| {
                    status_label(
                        query.is_empty(),
                        self.busy.get(),
                        error,
                        self.index.get(),
                        hits.len(),
                        self.truncated.get(),
                    )
                })
            })
        });
        self.label.set(label);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_matches_nothing() {
        let outcome = search("abc abc", "", false, false);
        assert!(outcome.hits.is_empty());
        assert!(outcome.error.is_none());
    }

    #[test]
    fn literal_search_is_case_sensitive_when_asked() {
        let loose = search("Alpha alpha ALPHA", "alpha", false, false);
        assert_eq!(loose.hits.len(), 3);
        let strict = search("Alpha alpha ALPHA", "alpha", true, false);
        assert_eq!(strict.hits, vec![TextHit { start: 6, end: 11 }]);
    }

    #[test]
    fn regex_reports_an_invalid_pattern_and_skips_empty_matches() {
        let bad = search("abc", "(", false, true);
        assert!(bad.hits.is_empty());
        assert!(bad.error.is_some());

        let stars = search("ab", "a*", false, true);
        assert_eq!(stars.hits, vec![TextHit { start: 0, end: 1 }]);
    }

    #[test]
    fn hits_do_not_overlap_and_the_cap_is_honest() {
        let outcome = Needle::compile("aa", true, false)
            .unwrap()
            .find_limited("aaaa", 2);
        assert_eq!(
            outcome.hits,
            vec![TextHit { start: 0, end: 2 }, TextHit { start: 2, end: 4 }]
        );
        assert!(!outcome.truncated);

        let truncated = Needle::compile("a", true, false)
            .unwrap()
            .find_limited("aaaa", 2);
        assert_eq!(truncated.hits.len(), 2);
        assert!(truncated.truncated);
    }

    #[test]
    fn a_match_past_the_preview_is_still_found() {
        let mut text = "x".repeat(40_000);
        text.push_str("NEEDLE");
        let outcome = search(&text, "NEEDLE", true, false);
        assert_eq!(outcome.hits.len(), 1);
        assert!(outcome.hits[0].start >= 40_000);
        assert_eq!(&text[outcome.hits[0].start..outcome.hits[0].end], "NEEDLE");
    }

    #[test]
    fn emoji_before_a_hit_shifts_utf16() {
        let text = "😀ab";
        let (start, end) = utf16_span(text, "😀".len(), text.len());
        assert_eq!((start, end), (2, 4));
    }

    #[test]
    fn status_label_numbers_the_current_hit() {
        assert_eq!(status_label(true, false, "", 0, 0, false), "");
        assert_eq!(status_label(false, true, "", 0, 0, false), "Searching…");
        assert_eq!(status_label(false, false, "", 0, 0, false), "No matches");
        assert_eq!(status_label(false, false, "", 2, 17, false), "3 / 17");
        assert_eq!(status_label(false, false, "", 0, 50_000, true), "1 / 50000+");
        assert_eq!(status_label(false, false, "bad", 0, 0, false), "bad");
    }

    #[test]
    fn snippets_mark_the_current_hit_and_keep_character_boundaries() {
        let text = "red blue red\ngreen red";
        let hits = search(text, "red", true, false).hits;
        assert_eq!(hits.len(), 3);
        let rows = snippets(text, &hits, 1, 2);
        assert!(rows.len() >= 2);
        let current = rows.iter().find(|row| row.hit_index == 1).expect("current");
        assert_eq!(current.line, 1);
        assert!(current.pieces.iter().any(|piece| {
            piece.kind == PieceKind::Current && piece.text == "red"
        }));
        assert!(current.pieces.iter().any(|piece| piece.kind == PieceKind::Hit));

        let line = format!("{}NEEDLE{}", "你".repeat(80), "你".repeat(80));
        let text = format!("head\n{line}\ntail");
        let outcome = search(&text, "NEEDLE", true, false);
        let rows = snippets(&text, &outcome.hits, 0, 1);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].line, 2);
        let joined: String = rows[0].pieces.iter().map(|piece| piece.text.as_str()).collect();
        assert!(joined.contains("NEEDLE"));
        assert!(rows[0].pieces.iter().any(|piece| {
            piece.kind == PieceKind::Current && piece.text == "NEEDLE"
        }));
    }

    #[test]
    fn pieces_split_a_label_around_the_match() {
        let parts = pieces("city: Shanghai", "Shang", true, false);
        assert!(parts.iter().any(|piece| {
            piece.kind == PieceKind::Current && piece.text == "Shang"
        }));
        assert!(parts.iter().any(|piece| piece.kind == PieceKind::Plain && piece.text.contains("city")));
    }
}
