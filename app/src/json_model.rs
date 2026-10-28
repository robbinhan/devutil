//! Pure JSON workbench helpers. The UI calls these; the parser stays in `json_core`.

use json_core::{
    AnalysisResult, CanonicalSerializer, DiagnosticSeverity, JSONValue, StrictJSONParser,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputMode {
    Pretty,
    Compact,
    Source,
}

pub fn output_text(result: &AnalysisResult, mode: OutputMode) -> Option<String> {
    match mode {
        OutputMode::Pretty => result.value.as_ref().map(CanonicalSerializer::pretty),
        OutputMode::Compact => result.normalized.clone(),
        OutputMode::Source => Some(result.source.clone()),
    }
}

/// UTF-16 units a multiline field may hold.
///
/// NSH 0.1.1 measures a multiline `input` with the platform text engine and
/// `max_lines == 0`, so the whole string is typeset. On macOS the view is an
/// `NSTextView` with `setVerticallyResizable(true)` and a text-container
/// height of `CGFloat::MAX`, which lays out every line to learn its height.
/// A few megabytes, in particular one long line, makes that allocation grow
/// without settling. `max_length` is the same unit count, so a paste is
/// clipped before it lands in the view.
pub const FIELD_LIMIT: usize = 32 * 1024;

/// Lines a multiline field may hold. A pretty document hits this before the
/// byte cap; the rest stays in memory for Copy.
pub const FIELD_LINE_LIMIT: usize = 200;

/// Children listed for one expanded array or object. The rest is a single
/// "more" row, so opening a 20,000-element array does not build 20,000 rows.
pub const TREE_CHILD_LIMIT: usize = 1000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldPreview {
    pub text: String,
    /// Byte offset of `text` in the full document. The default view is the prefix.
    pub byte_start: usize,
    pub capped: bool,
}

/// The prefix a text view is allowed to show. Not capped when `full` fits.
pub fn field_text(full: &str) -> FieldPreview {
    let end = preview_end(full);
    FieldPreview {
        byte_start: 0,
        capped: end < full.len(),
        text: full[..end].to_string(),
    }
}

/// Where a hit landed, and the slice the text view should show so the hit
/// is near the top. NSH selects UTF-16 units of that slice; it does not
/// scroll a selection into view, so the slice itself is the scroll.
pub struct Reveal {
    pub preview: FieldPreview,
    pub utf16_start: u32,
    pub utf16_end: u32,
    pub line: u32,
}

pub fn reveal_match(full: &str, start: usize, end: usize) -> Reveal {
    let start = floor_char(full, start.min(full.len()));
    let end = floor_char(full, end.clamp(start, full.len()));
    let line = line_number(full, start);
    let whole = field_text(full);
    if !whole.capped {
        let (utf16_start, utf16_end) = crate::find::utf16_span(full, start, end);
        return Reveal {
            preview: whole,
            utf16_start,
            utf16_end,
            line,
        };
    }
    let origin = window_origin(full, start, end);
    let window_end = take_window(full, origin);
    let shown_end = window_end.max(origin);
    let text = full[origin..shown_end].to_string();
    let local_end = end.min(shown_end).saturating_sub(origin);
    let local_start = start.saturating_sub(origin).min(local_end);
    let (utf16_start, utf16_end) = crate::find::utf16_span(&text, local_start, local_end);
    Reveal {
        preview: FieldPreview {
            text,
            byte_start: origin,
            capped: origin > 0 || shown_end < full.len(),
        },
        utf16_start,
        utf16_end,
        line,
    }
}

/// Replace the slice a window was showing. `None` when the offsets are not
/// on character boundaries, so a bad edit cannot tear the document.
pub fn splice_window(full: &str, origin: usize, old_len: usize, edited: &str) -> Option<String> {
    if !full.is_char_boundary(origin) {
        return None;
    }
    let end = origin.checked_add(old_len)?;
    if end > full.len() || !full.is_char_boundary(end) {
        return None;
    }
    let mut out = String::with_capacity(full.len() - old_len + edited.len());
    out.push_str(&full[..origin]);
    out.push_str(edited);
    out.push_str(&full[end..]);
    Some(out)
}

pub fn window_message(full_bytes: usize, line: u32) -> String {
    format!(
        "Showing a window around the match on line {line}. The text view cannot hold the whole document ({}). Search uses the full text.",
        format_bytes(full_bytes)
    )
}

fn window_origin(full: &str, start: usize, end: usize) -> usize {
    let line = line_start(full, start);
    let with_context = retreat_lines(full, line, 2);
    if take_window(full, with_context) >= end {
        return with_context;
    }
    if take_window(full, line) >= end {
        return line;
    }
    start
}

fn retreat_lines(full: &str, mut at: usize, lines: usize) -> usize {
    for _ in 0..lines {
        if at == 0 {
            break;
        }
        at = line_start(full, at - 1);
    }
    at
}

fn line_start(full: &str, at: usize) -> usize {
    full[..at].rfind('\n').map(|index| index + 1).unwrap_or(0)
}

fn line_number(full: &str, at: usize) -> u32 {
    (full[..at].bytes().filter(|byte| *byte == b'\n').count() + 1) as u32
}

fn take_window(full: &str, origin: usize) -> usize {
    let mut units = 0usize;
    let mut lines = 1usize;
    for (index, ch) in full[origin..].char_indices() {
        if ch == '\n' && lines >= FIELD_LINE_LIMIT {
            return origin + index;
        }
        let add = ch.len_utf16();
        if units + add > FIELD_LIMIT {
            return origin + index;
        }
        units += add;
        if ch == '\n' {
            lines += 1;
        }
    }
    full.len()
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

fn preview_end(full: &str) -> usize {
    let mut units = 0usize;
    let mut lines = 1usize;
    for (index, ch) in full.char_indices() {
        if ch == '\n' && lines >= FIELD_LINE_LIMIT {
            return index;
        }
        let add = ch.len_utf16();
        if units + add > FIELD_LIMIT {
            return index;
        }
        units += add;
        if ch == '\n' {
            lines += 1;
        }
    }
    full.len()
}

pub fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

pub fn format_bytes(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

pub fn cap_message(full_bytes: usize, preview_bytes: usize) -> String {
    format!(
        "Showing the first {} of {}. The text view stops there so it stays responsive. Analyze and Copy use the full text.",
        format_bytes(preview_bytes),
        format_bytes(full_bytes)
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeRow {
    pub id: String,
    pub depth: u32,
    pub label: String,
    pub expandable: bool,
    pub expanded: bool,
    /// The row's own key or value matched. Ancestors stay visible without this.
    pub matched: bool,
}

/// Visible outline of `value`. `expanded` holds the ids that are open.
/// The root id is `"root"`. Child ids are `{parent}/{index}` so duplicate
/// object keys stay distinct.
pub fn tree_rows(value: &JSONValue, expanded: &[String]) -> Vec<TreeRow> {
    let mut rows = Vec::new();
    walk(value, "root", "root", 0, expanded, &mut rows);
    rows
}

fn is_open(expanded: &[String], id: &str) -> bool {
    expanded.iter().any(|item| item == id)
}

fn walk(
    value: &JSONValue,
    id: &str,
    label: &str,
    depth: u32,
    expanded: &[String],
    rows: &mut Vec<TreeRow>,
) {
    match value {
        JSONValue::Object(members) => {
            let open = is_open(expanded, id);
            rows.push(TreeRow {
                id: id.to_string(),
                depth,
                label: format!(
                    "{label}  {{ {} {} }}",
                    members.len(),
                    if members.len() == 1 { "field" } else { "fields" }
                ),
                expandable: true,
                expanded: open,
                matched: false,
            });
            if open {
                push_children(
                    id,
                    depth,
                    expanded,
                    rows,
                    members.len(),
                    members
                        .iter()
                        .take(TREE_CHILD_LIMIT)
                        .map(|member| (&member.value, member.key.clone())),
                );
            }
        }
        JSONValue::Array(values) => {
            let open = is_open(expanded, id);
            rows.push(TreeRow {
                id: id.to_string(),
                depth,
                label: format!(
                    "{label}  [ {} {} ]",
                    values.len(),
                    if values.len() == 1 { "item" } else { "items" }
                ),
                expandable: true,
                expanded: open,
                matched: false,
            });
            if open {
                push_children(
                    id,
                    depth,
                    expanded,
                    rows,
                    values.len(),
                    values.iter().take(TREE_CHILD_LIMIT).enumerate().map(|(index, child)| {
                        (child, format!("[{index}]"))
                    }),
                );
            }
        }
        other => {
            let display = match other {
                JSONValue::String(value) => format!("\"{value}\""),
                JSONValue::Number { raw } => raw.clone(),
                JSONValue::Bool(true) => "true".to_string(),
                JSONValue::Bool(false) => "false".to_string(),
                JSONValue::Null => "null".to_string(),
                JSONValue::Object(_) | JSONValue::Array(_) => String::new(),
            };
            let nested = match other {
                JSONValue::String(value) => nested_json_value(value),
                _ => None,
            };
            let open = nested.is_some() && is_open(expanded, id);
            rows.push(TreeRow {
                id: id.to_string(),
                depth,
                label: format!("{label}: {display}"),
                expandable: nested.is_some(),
                expanded: open,
                matched: false,
            });
            if open {
                if let Some(nested) = nested {
                    let child = format!("{id}/nested");
                    walk(&nested, &child, "Nested JSON", depth + 1, expanded, rows);
                }
            }
        }
    }
}

fn push_children<'a>(
    id: &str,
    depth: u32,
    expanded: &[String],
    rows: &mut Vec<TreeRow>,
    total: usize,
    children: impl Iterator<Item = (&'a JSONValue, String)>,
) {
    for (index, (child, label)) in children.enumerate() {
        let child_id = format!("{id}/{index}");
        walk(child, &child_id, &label, depth + 1, expanded, rows);
    }
    if total > TREE_CHILD_LIMIT {
        rows.push(TreeRow {
            id: format!("{id}/more"),
            depth: depth + 1,
            label: format!("… {} more not shown", total - TREE_CHILD_LIMIT),
            expandable: false,
            expanded: false,
            matched: false,
        });
    }
}

/// Rows whose key or value matches `matches`, plus the ancestors of those
/// rows. Each collection lists at most [`TREE_CHILD_LIMIT`] matching
/// children; the rest is one "more" row. Non-matching siblings are omitted,
/// so a hit past the first 1,000 children can still appear.
pub fn filtered_tree_rows(value: &JSONValue, matches: impl Fn(&str) -> bool) -> Vec<TreeRow> {
    let mut rows = Vec::new();
    if walk_filtered(value, "root", "root", 0, &matches, &mut rows, 0) {
        rows
    } else {
        Vec::new()
    }
}

fn walk_filtered(
    value: &JSONValue,
    id: &str,
    label: &str,
    depth: u32,
    matches: &impl Fn(&str) -> bool,
    rows: &mut Vec<TreeRow>,
    depth_left: u32,
) -> bool {
    if depth_left > 64 {
        return false;
    }
    let depth_left = depth_left + 1;
    match value {
        JSONValue::Object(members) => {
            let self_match = matches(label);
            let mut interesting = Vec::new();
            for (index, member) in members.iter().enumerate() {
                if contains_match(&member.value, &member.key, matches, depth_left) {
                    interesting.push((index, member));
                }
            }
            if !self_match && interesting.is_empty() {
                return false;
            }
            let shown = interesting.len().min(TREE_CHILD_LIMIT);
            let hidden = interesting.len().saturating_sub(TREE_CHILD_LIMIT);
            rows.push(TreeRow {
                id: id.to_string(),
                depth,
                label: format!(
                    "{label}  {{ {} {} }}",
                    members.len(),
                    if members.len() == 1 { "field" } else { "fields" }
                ),
                expandable: true,
                expanded: shown > 0,
                matched: self_match,
            });
            for (index, member) in interesting.into_iter().take(shown) {
                let child_id = format!("{id}/{index}");
                walk_filtered(
                    &member.value,
                    &child_id,
                    &member.key,
                    depth + 1,
                    matches,
                    rows,
                    depth_left,
                );
            }
            if hidden > 0 {
                rows.push(more_row(id, depth, hidden));
            }
            true
        }
        JSONValue::Array(values) => {
            let self_match = matches(label);
            let mut interesting = Vec::new();
            for (index, child) in values.iter().enumerate() {
                let child_label = format!("[{index}]");
                if contains_match(child, &child_label, matches, depth_left) {
                    interesting.push((index, child_label, child));
                }
            }
            if !self_match && interesting.is_empty() {
                return false;
            }
            let shown = interesting.len().min(TREE_CHILD_LIMIT);
            let hidden = interesting.len().saturating_sub(TREE_CHILD_LIMIT);
            rows.push(TreeRow {
                id: id.to_string(),
                depth,
                label: format!(
                    "{label}  [ {} {} ]",
                    values.len(),
                    if values.len() == 1 { "item" } else { "items" }
                ),
                expandable: true,
                expanded: shown > 0,
                matched: self_match,
            });
            for (index, child_label, child) in interesting.into_iter().take(shown) {
                let child_id = format!("{id}/{index}");
                walk_filtered(child, &child_id, &child_label, depth + 1, matches, rows, depth_left);
            }
            if hidden > 0 {
                rows.push(more_row(id, depth, hidden));
            }
            true
        }
        other => {
            let raw = scalar_display(other);
            let display = match other {
                JSONValue::String(_) => format!("\"{raw}\""),
                _ => raw.to_string(),
            };
            let self_match = matches(label) || matches(raw) || matches(&display);
            let nested = match other {
                JSONValue::String(value) if value.len() <= 64 * 1024 => nested_json_value(value),
                _ => None,
            };
            let nested_match = nested
                .as_ref()
                .is_some_and(|nested| contains_match(nested, "Nested JSON", matches, depth_left));
            if !self_match && !nested_match {
                return false;
            }
            let child_id = format!("{id}/nested");
            rows.push(TreeRow {
                id: id.to_string(),
                depth,
                label: format!("{label}: {display}"),
                expandable: nested_match,
                expanded: nested_match,
                matched: self_match,
            });
            if nested_match {
                if let Some(nested) = nested {
                    walk_filtered(
                        &nested,
                        &child_id,
                        "Nested JSON",
                        depth + 1,
                        matches,
                        rows,
                        depth_left,
                    );
                }
            }
            true
        }
    }
}

fn contains_match(
    value: &JSONValue,
    label: &str,
    matches: &impl Fn(&str) -> bool,
    depth_left: u32,
) -> bool {
    if depth_left > 64 {
        return false;
    }
    if matches(label) {
        return true;
    }
    match value {
        JSONValue::Object(members) => members
            .iter()
            .any(|member| contains_match(&member.value, &member.key, matches, depth_left + 1)),
        JSONValue::Array(values) => values.iter().enumerate().any(|(index, child)| {
            let child_label = format!("[{index}]");
            contains_match(child, &child_label, matches, depth_left + 1)
        }),
        other => {
            if matches(scalar_display(other)) {
                return true;
            }
            match other {
                JSONValue::String(value) if value.len() <= 64 * 1024 => nested_json_value(value)
                    .is_some_and(|nested| contains_match(&nested, "Nested JSON", matches, depth_left + 1)),
                _ => false,
            }
        }
    }
}

fn scalar_display(value: &JSONValue) -> &str {
    match value {
        JSONValue::String(value) => value,
        JSONValue::Number { raw } => raw,
        JSONValue::Bool(true) => "true",
        JSONValue::Bool(false) => "false",
        JSONValue::Null => "null",
        JSONValue::Object(_) | JSONValue::Array(_) => "",
    }
}

fn more_row(id: &str, depth: u32, hidden: usize) -> TreeRow {
    TreeRow {
        id: format!("{id}/more"),
        depth: depth + 1,
        label: format!(
            "… {hidden} more {}",
            if hidden == 1 { "match" } else { "matches" }
        ),
        expandable: false,
        expanded: false,
        matched: false,
    }
}

fn nested_json_value(string: &str) -> Option<JSONValue> {
    match StrictJSONParser::new().parse(string).ok()? {
        value @ (JSONValue::Object(_) | JSONValue::Array(_)) => Some(value),
        _ => None,
    }
}

pub fn severity_name(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => "Error",
        DiagnosticSeverity::Warning => "Warning",
        DiagnosticSeverity::Info => "Info",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use json_core::AnalysisEngine;

    #[test]
    fn pretty_and_compact_follow_the_engine() {
        let result = AnalysisEngine::new().analyze(r#"{"b":1,"a":[true]}"#);
        let pretty = output_text(&result, OutputMode::Pretty).unwrap();
        assert!(pretty.contains('\n'));
        assert!(pretty.contains("\"b\""));
        let compact = output_text(&result, OutputMode::Compact).unwrap();
        assert!(!compact.contains('\n'));
        assert_eq!(
            output_text(&result, OutputMode::Source).as_deref(),
            Some(r#"{"b":1,"a":[true]}"#)
        );
    }

    #[test]
    fn tree_starts_closed_except_ids_listed_open() {
        let value = StrictJSONParser::new()
            .parse(r#"{"name":"DevUtil","tags":["a","b"]}"#)
            .unwrap();
        let closed = tree_rows(&value, &[]);
        assert_eq!(closed.len(), 1);
        assert!(closed[0].expandable);
        assert!(!closed[0].expanded);

        let open = tree_rows(&value, &["root".into()]);
        assert_eq!(open.len(), 3);
        assert_eq!(open[1].label, "name: \"DevUtil\"");
        assert!(open[2].label.starts_with("tags"));
        assert!(!open[2].expanded);

        let tags = tree_rows(&value, &["root".into(), "root/1".into()]);
        assert_eq!(tags.len(), 5);
        assert_eq!(tags[3].label, "[0]: \"a\"");
        assert_eq!(tags[4].label, "[1]: \"b\"");
    }

    #[test]
    fn duplicate_keys_get_distinct_ids() {
        let value = StrictJSONParser::new()
            .parse(r#"{"k":1,"k":2}"#)
            .unwrap();
        let rows = tree_rows(&value, &["root".into()]);
        assert_eq!(rows[1].id, "root/0");
        assert_eq!(rows[2].id, "root/1");
        assert!(rows[1].label.starts_with("k:"));
        assert!(rows[2].label.starts_with("k:"));
    }

    #[test]
    fn a_string_of_json_can_expand() {
        let value = StrictJSONParser::new()
            .parse(r#"{"blob":"{\"n\":1}"}"#)
            .unwrap();
        let closed = tree_rows(&value, &["root".into(), "root/0".into()]);
        assert!(closed.iter().any(|row| row.label.starts_with("Nested JSON")));
        assert!(!closed.iter().any(|row| row.label.starts_with("n:")));
        let rows = tree_rows(&value, &["root".into(), "root/0".into(), "root/0/nested".into()]);
        assert!(rows.iter().any(|row| row.label.starts_with("n:")));
    }

    #[test]
    fn the_field_preview_stops_at_the_text_view_limit() {
        let short = "hello";
        let small = field_text(short);
        assert!(!small.capped);
        assert_eq!(small.text, short);

        let long = "x".repeat(FIELD_LIMIT + 32);
        let capped = field_text(&long);
        assert!(capped.capped);
        assert_eq!(capped.text.len(), FIELD_LIMIT);
        assert!(long.starts_with(&capped.text));

        // A pair is not split, matching the platform's UTF-16 max_length.
        let mut emoji = "a".repeat(FIELD_LIMIT - 1);
        emoji.push('😀');
        emoji.push('b');
        let clipped = field_text(&emoji);
        assert!(clipped.capped);
        assert!(clipped.text.ends_with('a'));
        assert!(!clipped.text.contains('😀'));

        let lines = "x\n".repeat(FIELD_LINE_LIMIT + 40);
        let preview = field_text(&lines);
        assert!(preview.capped);
        assert_eq!(preview.text.matches('\n').count(), FIELD_LINE_LIMIT - 1);
        assert!(preview.text.len() < FIELD_LIMIT);
    }

    #[test]
    fn a_wide_array_lists_a_window_of_children() {
        let extra = 50;
        let total = TREE_CHILD_LIMIT + extra;
        let json = format!(
            "[{}]",
            (0..total)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
        let value = StrictJSONParser::new().parse(&json).unwrap();
        let rows = tree_rows(&value, &["root".into()]);
        assert_eq!(rows.len(), 1 + TREE_CHILD_LIMIT + 1);
        assert_eq!(rows[1].label, "[0]: 0");
        assert!(rows.last().unwrap().label.contains(&extra.to_string()));
        assert!(!rows.last().unwrap().expandable);
    }

    #[test]
    fn twenty_thousand_objects_analyze_format_and_preview_stay_bounded() {
        let json = sample_objects(20_000);
        let started = std::time::Instant::now();
        let result = AnalysisEngine::new().analyze(&json);
        let analyzed = started.elapsed();
        assert!(
            matches!(result.disposition, json_core::Disposition::Accepted),
            "{:?}",
            result.disposition
        );

        let pretty_started = std::time::Instant::now();
        let pretty = output_text(&result, OutputMode::Pretty).expect("pretty");
        let pretty_elapsed = pretty_started.elapsed();
        let shown = field_text(&pretty);
        assert!(shown.capped);
        assert!(pretty.starts_with(&shown.text));
        assert!(shown.text.len() <= FIELD_LIMIT);
        assert!(pretty.contains("\"id\": 19999"));

        let rows = tree_rows(result.value.as_ref().unwrap(), &["root".into()]);
        assert_eq!(rows.len(), 1 + TREE_CHILD_LIMIT + 1);

        assert!(
            analyzed.as_secs() < 8,
            "analyze took {analyzed:?} for {} bytes",
            json.len()
        );
        assert!(
            pretty_elapsed.as_secs() < 8,
            "pretty took {pretty_elapsed:?} for {} bytes",
            pretty.len()
        );
        if let Some(hwm_kb) = vm_hwm_kb() {
            assert!(
                hwm_kb < 512 * 1024,
                "VmHWM {hwm_kb} KiB after analyzing {} bytes into {} pretty bytes",
                json.len(),
                pretty.len()
            );
        }
        let find_started = std::time::Instant::now();
        let outcome = crate::find::search(&json, "user19999", true, false);
        let find_elapsed = find_started.elapsed();
        assert_eq!(outcome.hits.len(), 1, "{outcome:?}");
        let hit = outcome.hits[0];
        assert!(hit.start > FIELD_LIMIT, "the hit must sit past the text-view prefix");
        let revealed = reveal_match(&json, hit.start, hit.end);
        assert!(revealed.preview.byte_start > 0);
        assert!(revealed.preview.text.contains("user19999"));
        assert!(revealed.preview.text.len() <= FIELD_LIMIT);
        assert_eq!(
            crate::find::slice_utf16(&revealed.preview.text, revealed.utf16_start, revealed.utf16_end),
            "user19999"
        );

        let needle = crate::find::Needle::compile("Shanghai", true, false).unwrap();
        let filter_started = std::time::Instant::now();
        let filtered = filtered_tree_rows(result.value.as_ref().unwrap(), |text| needle.matches(text));
        let filter_elapsed = filter_started.elapsed();
        assert!(
            filtered.iter().any(|row| row.matched && row.label.contains("Shanghai")),
            "a matching city should be highlighted"
        );
        assert!(filtered.iter().any(|row| row.label.contains("more match")));
        assert!(filtered.len() < 8_000, "the child cap must bound the filtered tree");
        assert!(
            find_elapsed.as_secs() < 2,
            "find took {find_elapsed:?} in {} bytes",
            json.len()
        );
        assert!(
            filter_elapsed.as_secs() < 8,
            "tree filter took {filter_elapsed:?}"
        );

        eprintln!(
            "large json: source {} bytes, analyze {analyzed:?}, pretty {} bytes in {pretty_elapsed:?}, preview {} bytes, tree rows {}, find {find_elapsed:?}, filter {filter_elapsed:?} ({} rows), VmHWM {:?} KiB",
            json.len(),
            pretty.len(),
            shown.text.len(),
            rows.len(),
            filtered.len(),
            vm_hwm_kb()
        );
    }

    #[test]
    fn a_window_around_a_late_line_stays_inside_the_field_limits() {
        let mut text = String::new();
        for line in 0..300 {
            text.push_str(&format!("line {line} ----\n"));
        }
        let marker = "line 250";
        let start = text.find(marker).unwrap();
        let revealed = reveal_match(&text, start, start + marker.len());
        assert!(revealed.preview.byte_start > 0);
        assert!(revealed.preview.text.contains(marker));
        assert!(revealed.preview.text.len() <= FIELD_LIMIT);
        assert!(revealed.preview.text.matches('\n').count() < FIELD_LINE_LIMIT);
        assert!(revealed.line >= 250);
        assert_eq!(
            crate::find::slice_utf16(&revealed.preview.text, revealed.utf16_start, revealed.utf16_end),
            marker
        );
    }

    #[test]
    fn a_match_past_the_child_cap_is_still_listed() {
        let total = TREE_CHILD_LIMIT + 10;
        let json = format!(
            "[{}]",
            (0..total)
                .map(|index| format!(r#"{{"n":{index}}}"#))
                .collect::<Vec<_>>()
                .join(",")
        );
        let value = StrictJSONParser::new().parse(&json).unwrap();
        let needle = format!("{}", total - 1);
        let rows = filtered_tree_rows(&value, |text| text.contains(&needle));
        assert!(rows.iter().any(|row| row.matched && row.label.contains(&needle)));
        assert!(rows.len() < 20, "only the matching branch should be listed");
        assert!(!rows.iter().any(|row| row.label.contains("more")));
    }

    #[test]
    fn splice_window_replaces_only_the_visible_slice() {
        let full = "abcdef";
        assert_eq!(splice_window(full, 2, 2, "XY").as_deref(), Some("abXYef"));
        assert_eq!(splice_window(full, 1, 10, "Z"), None);
    }

    fn sample_objects(count: usize) -> String {
        let mut out = String::with_capacity(count * 180);
        out.push('[');
        for i in 0..count {
            if i > 0 {
                out.push(',');
            }
            let age = 18 + (i % 63);
            let _ = std::fmt::Write::write_fmt(
                &mut out,
                format_args!(
                    r#"{{"id":{i},"name":"user{i}","email":"u{i}@example.com","tags":["a","b","c"],"profile":{{"age":{age},"score":0.5,"addr":{{"city":"Shanghai","zip":"200000"}}}}}}"#
                ),
            );
        }
        out.push(']');
        out
    }

    fn vm_hwm_kb() -> Option<u64> {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        for line in status.lines() {
            let Some(rest) = line.strip_prefix("VmHWM:") else {
                continue;
            };
            return rest.split_whitespace().next()?.parse().ok();
        }
        None
    }
}
