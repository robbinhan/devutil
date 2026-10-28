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
    pub capped: bool,
}

/// The prefix a text view is allowed to show. Not capped when `full` fits.
pub fn field_text(full: &str) -> FieldPreview {
    let end = preview_end(full);
    FieldPreview {
        capped: end < full.len(),
        text: full[..end].to_string(),
    }
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
        });
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
        eprintln!(
            "large json: source {} bytes, analyze {analyzed:?}, pretty {} bytes in {pretty_elapsed:?}, preview {} bytes, tree rows {}, VmHWM {:?} KiB",
            json.len(),
            pretty.len(),
            shown.text.len(),
            rows.len(),
            vm_hwm_kb()
        );
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
