use diff_core::{
    compare, inline_spans, split_lines, tokenize, unified_patch, Layout, Options, SideRow, Span,
    Tag, UnifiedRow,
};

fn full() -> Layout {
    Layout::default()
}

fn changed_text(spans: &[Span]) -> Vec<String> {
    spans
        .iter()
        .filter(|span| span.changed)
        .map(|span| span.text.clone())
        .collect()
}

#[test]
fn splits_lines_without_terminators() {
    assert!(split_lines("").is_empty());
    assert_eq!(split_lines("a"), vec!["a"]);
    assert_eq!(split_lines("a\n"), vec!["a"]);
    assert_eq!(split_lines("a\r\nb\r\n"), vec!["a", "b"]);
    assert_eq!(split_lines("a\n\nb"), vec!["a", "", "b"]);
}

#[test]
fn identical_texts() {
    let result = compare("alpha\nbeta\n", "alpha\nbeta\n", Options::default(), full());
    assert!(result.summary.identical());
    assert!(result.exact);
    assert_eq!(result.summary.changes, 0);
    assert_eq!(result.side.len(), 2);
    assert!(unified_patch("alpha\n", "alpha\n", Options::default(), "a", "b", 3).is_empty());
}

#[test]
fn counts_added_and_removed_lines() {
    let old = "one\ntwo\nthree\n";
    let new = "one\nthree\nfour\n";
    let result = compare(old, new, Options::default(), full());
    assert_eq!(result.summary.removed, 1);
    assert_eq!(result.summary.added, 1);
    assert_eq!(result.summary.changes, 2);
    assert!(!result.summary.identical());
    let tags: Vec<(Tag, Option<usize>, Option<usize>)> = result
        .unified
        .iter()
        .map(|row| match row {
            UnifiedRow::Line { tag, old, new, .. } => (*tag, *old, *new),
            UnifiedRow::Fold { .. } => panic!("no folds in a full layout"),
        })
        .collect();
    assert_eq!(
        tags,
        vec![
            (Tag::Equal, Some(1), Some(1)),
            (Tag::Delete, Some(2), None),
            (Tag::Equal, Some(3), Some(2)),
            (Tag::Insert, None, Some(3)),
        ]
    );
}

#[test]
fn replaced_lines_pair_side_by_side_with_word_spans() {
    let old = "let colour = \"red\";\nkeep\n";
    let new = "let colour = \"blue\";\nkeep\n";
    let result = compare(old, new, Options::default(), full());
    let SideRow::Pair {
        left: Some(left),
        right: Some(right),
    } = &result.side[0]
    else {
        panic!("first row pairs both sides");
    };
    assert_eq!(left.tag, Tag::Delete);
    assert_eq!(right.tag, Tag::Insert);
    assert_eq!(left.number, 1);
    assert_eq!(right.number, 1);
    assert_eq!(changed_text(&left.spans), vec!["red"]);
    assert_eq!(changed_text(&right.spans), vec!["blue"]);
    assert_eq!(left.text(), "let colour = \"red\";");
}

#[test]
fn uneven_replace_leaves_blank_cells() {
    let result = compare("a\nb\n", "x\n", Options::default(), full());
    assert_eq!(result.side.len(), 2);
    match &result.side[1] {
        SideRow::Pair {
            left: Some(left),
            right: None,
        } => assert_eq!(left.number, 2),
        other => panic!("unexpected row {other:?}"),
    }
}

#[test]
fn unrelated_lines_are_not_split_into_words() {
    let (left, right) = inline_spans("completely different", "zzz qqq www");
    assert!(changed_text(&left).is_empty());
    assert!(changed_text(&right).is_empty());
}

#[test]
fn tokenizer_keeps_words_and_splits_ideographs() {
    assert_eq!(
        tokenize("foo_bar(1, 2)"),
        vec!["foo_bar", "(", "1", ",", " ", "2", ")"]
    );
    assert_eq!(tokenize("示例文本"), vec!["示", "例", "文", "本"]);
    assert_eq!(tokenize("a  b"), vec!["a", "  ", "b"]);
}

#[test]
fn ignore_whitespace_and_case() {
    let old = "Hello   World\nfoo\n";
    let new = "hello world\nFOO\n";
    assert!(!compare(old, new, Options::default(), full())
        .summary
        .identical());
    let ws = Options {
        ignore_whitespace: true,
        ..Options::default()
    };
    assert!(!compare(old, new, ws, full()).summary.identical());
    let both = Options {
        ignore_whitespace: true,
        ignore_case: true,
    };
    let result = compare(old, new, both, full());
    assert!(result.summary.identical());
    assert!(!result.exact);
    // Equal rows still show each side's own text.
    let SideRow::Pair {
        left: Some(left),
        right: Some(right),
    } = &result.side[0]
    else {
        panic!("pair");
    };
    assert_eq!(left.text(), "Hello   World");
    assert_eq!(right.text(), "hello world");
}

#[test]
fn folds_unchanged_runs_outside_context() {
    let old: String = (1..=20).map(|n| format!("line {n}\n")).collect();
    let new = old.replace("line 10\n", "line ten\n");
    let layout = Layout {
        context: Some(2),
        row_cap: 5_000,
    };
    let result = compare(&old, &new, Options::default(), layout);
    let folds: Vec<usize> = result
        .side
        .iter()
        .filter_map(|row| match row {
            SideRow::Fold { hidden } => Some(*hidden),
            _ => None,
        })
        .collect();
    assert_eq!(folds, vec![7, 8]);
    // 2 context + 1 change + 2 context, plus two folds.
    assert_eq!(result.side.len(), 7);
}

#[test]
fn caps_rows_but_keeps_the_full_summary() {
    let old: String = (0..100).map(|n| format!("old {n}\n")).collect();
    let new: String = (0..100).map(|n| format!("new {n} extra\n")).collect();
    let layout = Layout {
        context: None,
        row_cap: 10,
    };
    let result = compare(&old, &new, Options::default(), layout);
    assert!(result.truncated);
    assert!(result.side.len() <= 10);
    assert!(result.unified.len() <= 10);
    assert_eq!(result.summary.removed, 100);
    assert_eq!(result.summary.added, 100);
}

#[test]
fn unified_patch_format() {
    let old = "one\ntwo\nthree\nfour\n";
    let new = "one\n2\nthree\nfour\nfive\n";
    let patch = unified_patch(
        old,
        new,
        Options::default(),
        "a/sample.txt",
        "b/sample.txt",
        1,
    );
    assert_eq!(
        patch,
        "--- a/sample.txt\n+++ b/sample.txt\n@@ -1,4 +1,5 @@\n one\n-two\n+2\n three\n four\n+five\n"
    );
}

#[test]
fn unified_patch_splits_distant_hunks() {
    let old: String = (1..=30).map(|n| format!("row {n}\n")).collect();
    let new = old
        .replace("row 2\n", "row two\n")
        .replace("row 28\n", "row twenty-eight\n");
    let patch = unified_patch(&old, &new, Options::default(), "a", "b", 3);
    let headers: Vec<&str> = patch
        .lines()
        .filter(|line| line.starts_with("@@"))
        .collect();
    assert_eq!(headers, vec!["@@ -1,5 +1,5 @@", "@@ -25,6 +25,6 @@"]);
}

#[test]
fn unified_patch_marks_missing_newline() {
    let patch = unified_patch("a\nb", "a\nb\n", Options::default(), "a", "b", 3);
    assert_eq!(
        patch,
        "--- a\n+++ b\n@@ -1,2 +1,2 @@\n a\n-b\n\\ No newline at end of file\n+b\n"
    );
}

#[test]
fn unified_patch_from_empty() {
    let patch = unified_patch("", "x\ny\n", Options::default(), "a", "b", 3);
    assert_eq!(patch, "--- a\n+++ b\n@@ -0,0 +1,2 @@\n+x\n+y\n");
    let patch = unified_patch("x\n", "", Options::default(), "a", "b", 3);
    assert_eq!(patch, "--- a\n+++ b\n@@ -1 +0,0 @@\n-x\n");
}

#[test]
fn unified_patch_respects_ignore_options() {
    let options = Options {
        ignore_whitespace: false,
        ignore_case: true,
    };
    assert!(unified_patch("ABC\n", "abc\n", options, "a", "b", 3).is_empty());
}
