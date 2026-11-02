//! Text compare for the DevUtil Diff tool.
//!
//! Lines are matched with `similar` (Myers, with a deadline so a hostile
//! input cannot stall a worker). Lines that pair up inside a replaced block
//! get a second, token-level pass, so the UI can mark the words that changed.
//! Ignore-whitespace and ignore-case only change how lines are compared; the
//! rows and the patch always carry the original text.

use std::time::{Duration, Instant};

use similar::{capture_diff_slices_deadline, Algorithm, DiffOp};

/// How long the line pass may run before `similar` falls back to a coarser
/// (still correct) script.
pub const LINE_DEADLINE: Duration = Duration::from_secs(2);

/// Lines longer than this (in chars) are not split into word spans.
pub const INLINE_CHAR_LIMIT: usize = 4_000;

/// Context lines kept around a change, for folded views and the patch.
pub const DEFAULT_CONTEXT: usize = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Compare lines with all whitespace removed (like `diff -w`).
    pub ignore_whitespace: bool,
    /// Compare lines case-insensitively.
    pub ignore_case: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    Equal,
    Delete,
    Insert,
}

/// A run of one line. `changed` marks the words that differ from the
/// paired line on the other side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub changed: bool,
}

/// One line on one side. `number` is 1-based.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub number: usize,
    pub tag: Tag,
    pub spans: Vec<Span>,
}

impl Cell {
    pub fn text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }
}

/// A side-by-side row. `Fold` stands for unchanged lines that are hidden
/// when the view keeps only the context around changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SideRow {
    Pair {
        left: Option<Cell>,
        right: Option<Cell>,
    },
    Fold {
        hidden: usize,
    },
}

/// A unified row. Equal lines carry both numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnifiedRow {
    Line {
        tag: Tag,
        old: Option<usize>,
        new: Option<usize>,
        spans: Vec<Span>,
    },
    Fold {
        hidden: usize,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub added: usize,
    pub removed: usize,
    /// Number of change blocks (hunks before context merging).
    pub changes: usize,
    pub old_lines: usize,
    pub new_lines: usize,
}

impl Summary {
    pub fn identical(&self) -> bool {
        self.added == 0 && self.removed == 0
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Comparison {
    pub summary: Summary,
    /// True when the texts are byte-for-byte equal (not only equal under
    /// the ignore options).
    pub exact: bool,
    pub side: Vec<SideRow>,
    pub unified: Vec<UnifiedRow>,
    /// Rows were cut at the cap. The summary still covers everything.
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// `Some(n)`: keep `n` unchanged lines around each change and fold the
    /// rest. `None`: show every line.
    pub context: Option<usize>,
    /// Maximum rows built for each view.
    pub row_cap: usize,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            context: None,
            row_cap: 5_000,
        }
    }
}

/// Split into lines without their terminators (`\n` or `\r\n`). An empty
/// text has no lines; a trailing newline does not open an extra line.
pub fn split_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let body = text.strip_suffix('\n').unwrap_or(text);
    body.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect()
}

fn line_key(line: &str, options: Options) -> String {
    let mut key: String = if options.ignore_whitespace {
        line.chars().filter(|c| !c.is_whitespace()).collect()
    } else {
        line.to_string()
    };
    if options.ignore_case {
        key = key.to_lowercase();
    }
    key
}

/// The line-level edit script.
pub fn line_ops(old: &[&str], new: &[&str], options: Options) -> Vec<DiffOp> {
    keyed_ops(old, new, options, false, false)
}

/// `*_open` marks a last line with no newline after it. The patch needs
/// that to differ from the same line with a newline, unless whitespace is
/// ignored.
fn keyed_ops(
    old: &[&str],
    new: &[&str],
    options: Options,
    old_open: bool,
    new_open: bool,
) -> Vec<DiffOp> {
    let keys = |lines: &[&str], open: bool| -> Vec<String> {
        let mut keys: Vec<String> = lines.iter().map(|line| line_key(line, options)).collect();
        if open && !options.ignore_whitespace {
            if let Some(last) = keys.last_mut() {
                last.push('\u{0}');
            }
        }
        keys
    };
    let old_keys = keys(old, old_open);
    let new_keys = keys(new, new_open);
    let deadline = Instant::now() + LINE_DEADLINE;
    capture_diff_slices_deadline(Algorithm::Myers, &old_keys, &new_keys, Some(deadline))
}

/// Tokens for the word pass: runs of letters, digits, and `_` (except
/// ideographs, which stand alone), runs of whitespace, and single symbols.
pub fn tokenize(line: &str) -> Vec<&str> {
    #[derive(PartialEq, Clone, Copy)]
    enum Class {
        Word,
        Space,
        Other,
    }
    fn class(c: char) -> Class {
        if c.is_whitespace() {
            Class::Space
        } else if (c.is_alphanumeric() || c == '_') && !is_ideograph(c) {
            Class::Word
        } else {
            Class::Other
        }
    }
    let mut tokens = Vec::new();
    let mut start = 0;
    let mut current: Option<Class> = None;
    for (index, c) in line.char_indices() {
        let next = class(c);
        match current {
            Some(previous) if previous == next && next != Class::Other => {}
            Some(_) => {
                tokens.push(&line[start..index]);
                start = index;
            }
            None => {}
        }
        current = Some(next);
    }
    if start < line.len() {
        tokens.push(&line[start..]);
    }
    tokens
}

fn is_ideograph(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF   // Hiragana, Katakana
        | 0x3400..=0x4DBF // CJK Ext A
        | 0x4E00..=0x9FFF // CJK Unified
        | 0xAC00..=0xD7AF // Hangul
        | 0xF900..=0xFAFF
        | 0x20000..=0x2FFFF)
}

fn plain(text: &str) -> Vec<Span> {
    vec![Span {
        text: text.to_string(),
        changed: false,
    }]
}

fn push_span(spans: &mut Vec<Span>, text: &str, changed: bool) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = spans.last_mut() {
        if last.changed == changed {
            last.text.push_str(text);
            return;
        }
    }
    spans.push(Span {
        text: text.to_string(),
        changed,
    });
}

/// Word spans for a removed line and the added line it pairs with. When
/// the lines share too little, nothing is marked inside them: the row
/// color already says the whole line changed.
pub fn inline_spans(old: &str, new: &str) -> (Vec<Span>, Vec<Span>) {
    if old.chars().count() > INLINE_CHAR_LIMIT || new.chars().count() > INLINE_CHAR_LIMIT {
        return (plain(old), plain(new));
    }
    let old_tokens = tokenize(old);
    let new_tokens = tokenize(new);
    let ops = similar::capture_diff_slices(Algorithm::Myers, &old_tokens, &new_tokens);
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut shared = 0usize;
    for op in &ops {
        match *op {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => {
                for k in 0..len {
                    let token = old_tokens[old_index + k];
                    if !token.trim().is_empty() {
                        shared += token.len();
                    }
                    push_span(&mut left, token, false);
                    push_span(&mut right, new_tokens[new_index + k], false);
                }
            }
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                for token in &old_tokens[old_index..old_index + old_len] {
                    push_span(&mut left, token, true);
                }
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                for token in &new_tokens[new_index..new_index + new_len] {
                    push_span(&mut right, token, true);
                }
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                for token in &old_tokens[old_index..old_index + old_len] {
                    push_span(&mut left, token, true);
                }
                for token in &new_tokens[new_index..new_index + new_len] {
                    push_span(&mut right, token, true);
                }
            }
        }
    }
    let longest = old.trim().len().max(new.trim().len()).max(1);
    if shared * 10 < longest * 3 {
        return (plain(old), plain(new));
    }
    if left.is_empty() {
        left = plain(old);
    }
    if right.is_empty() {
        right = plain(new);
    }
    (left, right)
}

/// One aligned step of the script, before folding.
enum Step<'a> {
    Equal {
        old_no: usize,
        new_no: usize,
        old: &'a str,
        new: &'a str,
    },
    Change {
        old_no: usize,
        new_no: usize,
        old: &'a [&'a str],
        new: &'a [&'a str],
    },
}

fn steps<'a>(old: &'a [&'a str], new: &'a [&'a str], ops: &[DiffOp]) -> Vec<Step<'a>> {
    let mut out = Vec::new();
    for op in ops {
        match *op {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => {
                for k in 0..len {
                    out.push(Step::Equal {
                        old_no: old_index + k + 1,
                        new_no: new_index + k + 1,
                        old: old[old_index + k],
                        new: new[new_index + k],
                    });
                }
            }
            DiffOp::Delete {
                old_index,
                old_len,
                new_index,
            } => out.push(Step::Change {
                old_no: old_index + 1,
                new_no: new_index + 1,
                old: &old[old_index..old_index + old_len],
                new: &new[new_index..new_index],
            }),
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => out.push(Step::Change {
                old_no: old_index + 1,
                new_no: new_index + 1,
                old: &old[old_index..old_index],
                new: &new[new_index..new_index + new_len],
            }),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => out.push(Step::Change {
                old_no: old_index + 1,
                new_no: new_index + 1,
                old: &old[old_index..old_index + old_len],
                new: &new[new_index..new_index + new_len],
            }),
        }
    }
    // `similar` can emit a delete next to an insert; merge them so the
    // lines pair up for the word pass.
    let mut merged: Vec<Step<'a>> = Vec::with_capacity(out.len());
    for step in out {
        if let (
            Some(Step::Change {
                old: prev_old,
                new: prev_new,
                old_no,
                new_no,
            }),
            Step::Change {
                old: next_old,
                new: next_new,
                ..
            },
        ) = (merged.last(), &step)
        {
            let old_end = *old_no - 1 + prev_old.len();
            let new_end = *new_no - 1 + prev_new.len();
            let (old_no, new_no) = (*old_no, *new_no);
            let joined_old = &old[old_no - 1..old_end + next_old.len()];
            let joined_new = &new[new_no - 1..new_end + next_new.len()];
            merged.pop();
            merged.push(Step::Change {
                old_no,
                new_no,
                old: joined_old,
                new: joined_new,
            });
            continue;
        }
        merged.push(step);
    }
    merged
}

/// Which equal lines a folded view keeps: those within `context` of a change.
fn visible_equals(steps: &[Step<'_>], context: Option<usize>) -> Vec<bool> {
    let Some(context) = context else {
        return vec![true; steps.len()];
    };
    let mut keep = vec![false; steps.len()];
    let changes: Vec<usize> = steps
        .iter()
        .enumerate()
        .filter(|(_, step)| matches!(step, Step::Change { .. }))
        .map(|(index, _)| index)
        .collect();
    for &index in &changes {
        keep[index] = true;
        let from = index.saturating_sub(context);
        let to = (index + context).min(steps.len() - 1);
        for slot in keep.iter_mut().take(to + 1).skip(from) {
            *slot = true;
        }
    }
    keep
}

/// Compare two texts and build both views.
pub fn compare(old_text: &str, new_text: &str, options: Options, layout: Layout) -> Comparison {
    let old = split_lines(old_text);
    let new = split_lines(new_text);
    let ops = line_ops(&old, &new, options);
    let steps = steps(&old, &new, &ops);

    let mut summary = Summary {
        old_lines: old.len(),
        new_lines: new.len(),
        ..Summary::default()
    };
    for step in &steps {
        if let Step::Change { old, new, .. } = step {
            summary.removed += old.len();
            summary.added += new.len();
            summary.changes += 1;
        }
    }

    let keep = visible_equals(&steps, layout.context);
    let cap = layout.row_cap.max(1);
    let mut side = Vec::new();
    let mut unified = Vec::new();
    let mut truncated = false;
    let mut hidden = 0usize;

    let flush = |hidden: &mut usize, side: &mut Vec<SideRow>, unified: &mut Vec<UnifiedRow>| {
        if *hidden > 0 {
            side.push(SideRow::Fold { hidden: *hidden });
            unified.push(UnifiedRow::Fold { hidden: *hidden });
            *hidden = 0;
        }
    };

    'outer: for (index, step) in steps.iter().enumerate() {
        if side.len() >= cap || unified.len() >= cap {
            truncated = true;
            break;
        }
        match step {
            Step::Equal {
                old_no,
                new_no,
                old,
                new,
            } => {
                if !keep[index] {
                    hidden += 1;
                    continue;
                }
                flush(&mut hidden, &mut side, &mut unified);
                side.push(SideRow::Pair {
                    left: Some(Cell {
                        number: *old_no,
                        tag: Tag::Equal,
                        spans: plain(old),
                    }),
                    right: Some(Cell {
                        number: *new_no,
                        tag: Tag::Equal,
                        spans: plain(new),
                    }),
                });
                unified.push(UnifiedRow::Line {
                    tag: Tag::Equal,
                    old: Some(*old_no),
                    new: Some(*new_no),
                    spans: plain(old),
                });
            }
            Step::Change {
                old_no,
                new_no,
                old,
                new,
            } => {
                flush(&mut hidden, &mut side, &mut unified);
                let pairs = old.len().min(new.len());
                let mut left_spans: Vec<Vec<Span>> = Vec::with_capacity(old.len());
                let mut right_spans: Vec<Vec<Span>> = Vec::with_capacity(new.len());
                for k in 0..pairs {
                    let (l, r) = inline_spans(old[k], new[k]);
                    left_spans.push(l);
                    right_spans.push(r);
                }
                left_spans.extend(old[pairs..].iter().map(|line| plain(line)));
                right_spans.extend(new[pairs..].iter().map(|line| plain(line)));

                for k in 0..old.len().max(new.len()) {
                    if side.len() >= cap {
                        truncated = true;
                        break;
                    }
                    side.push(SideRow::Pair {
                        left: left_spans.get(k).map(|spans| Cell {
                            number: old_no + k,
                            tag: Tag::Delete,
                            spans: spans.clone(),
                        }),
                        right: right_spans.get(k).map(|spans| Cell {
                            number: new_no + k,
                            tag: Tag::Insert,
                            spans: spans.clone(),
                        }),
                    });
                }
                for (k, spans) in left_spans.into_iter().enumerate() {
                    if unified.len() >= cap {
                        truncated = true;
                        break 'outer;
                    }
                    unified.push(UnifiedRow::Line {
                        tag: Tag::Delete,
                        old: Some(old_no + k),
                        new: None,
                        spans,
                    });
                }
                for (k, spans) in right_spans.into_iter().enumerate() {
                    if unified.len() >= cap {
                        truncated = true;
                        break 'outer;
                    }
                    unified.push(UnifiedRow::Line {
                        tag: Tag::Insert,
                        old: None,
                        new: Some(new_no + k),
                        spans,
                    });
                }
                if truncated {
                    break;
                }
            }
        }
    }
    if !truncated {
        flush(&mut hidden, &mut side, &mut unified);
    }

    Comparison {
        exact: old_text == new_text,
        summary,
        side,
        unified,
        truncated,
    }
}

fn hunk_range(start: usize, len: usize) -> String {
    // `start` is the 1-based first line; an empty range names the line before.
    let start = if len == 0 {
        start.saturating_sub(1)
    } else {
        start
    };
    if len == 1 {
        format!("{start}")
    } else {
        format!("{start},{len}")
    }
}

/// A unified patch (`diff -u` style). Empty when the texts compare equal
/// under `options`.
pub fn unified_patch(
    old_text: &str,
    new_text: &str,
    options: Options,
    old_name: &str,
    new_name: &str,
    context: usize,
) -> String {
    let old = split_lines(old_text);
    let new = split_lines(new_text);
    let old_missing_newline = !old_text.is_empty() && !old_text.ends_with('\n');
    let new_missing_newline = !new_text.is_empty() && !new_text.ends_with('\n');
    let ops = keyed_ops(
        &old,
        &new,
        options,
        old_missing_newline,
        new_missing_newline,
    );
    if ops.iter().all(|op| matches!(op, DiffOp::Equal { .. })) {
        return String::new();
    }

    let mut out = String::new();
    out.push_str(&format!("--- {old_name}\n+++ {new_name}\n"));
    for group in similar::group_diff_ops(ops, context) {
        let (Some(first), Some(last)) = (group.first(), group.last()) else {
            continue;
        };
        let old_start = first.old_range().start;
        let new_start = first.new_range().start;
        let old_len = last.old_range().end - old_start;
        let new_len = last.new_range().end - new_start;
        out.push_str(&format!(
            "@@ -{} +{} @@\n",
            hunk_range(old_start + 1, old_len),
            hunk_range(new_start + 1, new_len)
        ));
        for op in &group {
            let (old_range, new_range) = (op.old_range(), op.new_range());
            match op {
                DiffOp::Equal { .. } => {
                    for index in old_range {
                        push_patch_line(
                            &mut out,
                            ' ',
                            old[index],
                            index + 1 == old.len() && old_missing_newline,
                        );
                    }
                }
                DiffOp::Delete { .. } => {
                    for index in old_range {
                        push_patch_line(
                            &mut out,
                            '-',
                            old[index],
                            index + 1 == old.len() && old_missing_newline,
                        );
                    }
                }
                DiffOp::Insert { .. } => {
                    for index in new_range {
                        push_patch_line(
                            &mut out,
                            '+',
                            new[index],
                            index + 1 == new.len() && new_missing_newline,
                        );
                    }
                }
                DiffOp::Replace { .. } => {
                    for index in old_range {
                        push_patch_line(
                            &mut out,
                            '-',
                            old[index],
                            index + 1 == old.len() && old_missing_newline,
                        );
                    }
                    for index in new_range {
                        push_patch_line(
                            &mut out,
                            '+',
                            new[index],
                            index + 1 == new.len() && new_missing_newline,
                        );
                    }
                }
            }
        }
    }
    out
}

fn push_patch_line(out: &mut String, marker: char, line: &str, no_newline: bool) {
    out.push(marker);
    out.push_str(line);
    out.push('\n');
    if no_newline {
        out.push_str("\\ No newline at end of file\n");
    }
}
