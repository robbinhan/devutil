use json_core::{AnalysisEngine, Disposition, InputKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExpectedDisposition {
    Accepted,
    Recovered,
    Inspect,
    Rejected,
}

struct FixtureCase {
    id: &'static str,
    input: &'static str,
    expected: Option<&'static str>,
    disposition: ExpectedDisposition,
}

const REPRESENTATIVE_CASES: &[FixtureCase] = &[
    FixtureCase {
        id: "A-001",
        input: r#"{"ok":true}"#,
        expected: Some(r#"{"ok":true}"#),
        disposition: ExpectedDisposition::Accepted,
    },
    FixtureCase {
        id: "A-002",
        input: r#"[{"id":1},{"id":2}]"#,
        expected: Some(r#"[{"id":1},{"id":2}]"#),
        disposition: ExpectedDisposition::Accepted,
    },
    FixtureCase {
        id: "B-001",
        input: r#""{\"ok\":true}""#,
        expected: Some(r#"{"ok":true}"#),
        disposition: ExpectedDisposition::Recovered,
    },
    FixtureCase {
        id: "B-002",
        input: r#""\"{\\\"ok\\\":true}\"""#,
        expected: Some(r#"{"ok":true}"#),
        disposition: ExpectedDisposition::Recovered,
    },
    FixtureCase {
        id: "B-004",
        input: "```json\n{\"ok\":true}\n```",
        expected: Some(r#"{"ok":true}"#),
        disposition: ExpectedDisposition::Recovered,
    },
    FixtureCase {
        id: "C-002",
        input: r#"{"items":[1,2,],}"#,
        expected: Some(r#"{"items":[1,2]}"#),
        disposition: ExpectedDisposition::Recovered,
    },
    FixtureCase {
        id: "C-006",
        input: "{\"id\":1}\n{\"id\":2}",
        expected: Some(r#"[{"id":1},{"id":2}]"#),
        disposition: ExpectedDisposition::Recovered,
    },
    FixtureCase {
        id: "D-001",
        input: "%7B%22ok%22%3Atrue%7D",
        expected: Some(r#"{"ok":true}"#),
        disposition: ExpectedDisposition::Inspect,
    },
    FixtureCase {
        id: "D-002",
        input: "eyJvayI6dHJ1ZX0=",
        expected: Some(r#"{"ok":true}"#),
        disposition: ExpectedDisposition::Inspect,
    },
    FixtureCase {
        id: "E-002",
        input: r#"{"a":"first" "b":"second"}"#,
        expected: None,
        disposition: ExpectedDisposition::Rejected,
    },
];

fn matches_disposition(result: &json_core::AnalysisResult, expected: ExpectedDisposition) -> bool {
    match (&result.disposition, expected) {
        (Disposition::Accepted, ExpectedDisposition::Accepted) => true,
        (Disposition::Recovered, ExpectedDisposition::Recovered) => true,
        (Disposition::Inspect(_), ExpectedDisposition::Inspect) => true,
        (Disposition::Rejected, ExpectedDisposition::Rejected) => true,
        _ => false,
    }
}

fn assert_case(case: &FixtureCase) {
    let result = AnalysisEngine::new().analyze(case.input);
    assert!(
        matches_disposition(&result, case.disposition),
        "{}: expected {:?}, got {:?}",
        case.id,
        case.disposition,
        result.disposition
    );
    match case.disposition {
        ExpectedDisposition::Accepted | ExpectedDisposition::Recovered => {
            assert_eq!(
                result.normalized.as_deref(),
                case.expected,
                "{} normalized mismatch",
                case.id
            );
        }
        ExpectedDisposition::Inspect => {
            assert!(result.normalized.is_none(), "{} should not mutate", case.id);
            if let Disposition::Inspect(candidates) = &result.disposition {
                if let Some(expected) = case.expected {
                    assert!(
                        candidates.iter().any(|c| c.text == expected),
                        "{} missing candidate {expected}, have {:?}",
                        case.id,
                        candidates.iter().map(|c| &c.text).collect::<Vec<_>>()
                    );
                }
            }
        }
        ExpectedDisposition::Rejected => {
            assert!(result.value.is_none(), "{} should have no value", case.id);
            assert!(
                !result.diagnostics.is_empty(),
                "{} should report a diagnostic",
                case.id
            );
        }
    }
}

#[test]
fn representative_fixture_cases_from_swift() {
    for case in REPRESENTATIVE_CASES {
        assert_case(case);
    }
}

struct MdCase {
    id: String,
    input: String,
    expected: Option<String>,
}

fn fixture_markdown_path() -> std::path::PathBuf {
    // crates/json_core/tests -> repo tests/fixtures
    PathBufFromWorkspace()
}

#[allow(non_snake_case)]
fn PathBufFromWorkspace() -> std::path::PathBuf {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("../../tests/fixtures/json-recovery-cases.md")
        .canonicalize()
        .unwrap_or_else(|_| manifest.join("../../tests/fixtures/json-recovery-cases.md"))
}

fn parse_markdown_cases(markdown: &str) -> Vec<MdCase> {
    let mut cases = Vec::new();
    let lines: Vec<&str> = markdown.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some(rest) = line.strip_prefix("### ") {
            let id = rest
                .split(|c: char| c.is_whitespace() || c == '—' || c == '–')
                .find(|part| !part.is_empty())
                .unwrap_or(rest)
                .trim()
                .to_string();
            i += 1;
            let mut input = None;
            let mut expected = None;
            while i < lines.len() && !lines[i].starts_with("### ") {
                let fence = if lines[i].starts_with("````") {
                    Some("````")
                } else if lines[i].starts_with("```") {
                    Some("```")
                } else {
                    None
                };
                if let Some(delim) = fence {
                    let lang = lines[i][delim.len()..].trim();
                    i += 1;
                    let mut body = Vec::new();
                    while i < lines.len() && !lines[i].starts_with(delim) {
                        body.push(lines[i]);
                        i += 1;
                    }
                    if i < lines.len() {
                        i += 1;
                    }
                    let text = body.join("\n");
                    if lang == "json" {
                        expected = Some(text);
                    } else if lang == "text" || lang.is_empty() {
                        if input.is_none() {
                            input = Some(text);
                        }
                    }
                    continue;
                }
                i += 1;
            }
            if let Some(input) = input {
                cases.push(MdCase {
                    id,
                    input,
                    expected,
                });
            }
            continue;
        }
        i += 1;
    }
    cases
}

fn expected_disposition_for_id(id: &str) -> ExpectedDisposition {
    match id.chars().next() {
        Some('A') => ExpectedDisposition::Accepted,
        Some('B') if id == "B-003" => ExpectedDisposition::Accepted,
        Some('B') => ExpectedDisposition::Recovered,
        Some('C') => ExpectedDisposition::Recovered,
        Some('D') => ExpectedDisposition::Inspect,
        Some('E') if id == "E-001" => ExpectedDisposition::Accepted,
        Some('E') => ExpectedDisposition::Rejected,
        _ => ExpectedDisposition::Rejected,
    }
}

#[test]
fn markdown_recovery_corpus() {
    let path = fixture_markdown_path();
    let markdown = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let cases = parse_markdown_cases(&markdown);
    assert!(
        cases.len() >= 15,
        "expected the full recovery corpus, found {}",
        cases.len()
    );

    let engine = AnalysisEngine::new();
    for case in cases {
        let result = engine.analyze(&case.input);
        let expected_disp = expected_disposition_for_id(&case.id);
        assert!(
            matches_disposition(&result, expected_disp),
            "{}: expected {:?}, got {:?}\ninput:\n{}",
            case.id,
            expected_disp,
            result.disposition,
            case.input
        );

        match expected_disp {
            ExpectedDisposition::Accepted | ExpectedDisposition::Recovered => {
                if let Some(expected) = &case.expected {
                    assert_eq!(
                        result.normalized.as_deref(),
                        Some(expected.as_str()),
                        "{} normalized mismatch",
                        case.id
                    );
                }
            }
            ExpectedDisposition::Inspect => {
                assert!(result.normalized.is_none(), "{} mutated source", case.id);
                if let (Some(expected), Disposition::Inspect(candidates)) =
                    (&case.expected, &result.disposition)
                {
                    assert!(
                        candidates.iter().any(|c| c.text == *expected),
                        "{} missing candidate {expected}",
                        case.id
                    );
                }
            }
            ExpectedDisposition::Rejected => {
                assert!(result.value.is_none());
                assert!(!result.diagnostics.is_empty(), "{} missing diagnostic", case.id);
            }
        }
    }
}

#[test]
fn inspect_kinds_cover_url_base64_and_log_extract() {
    let engine = AnalysisEngine::new();

    let url = engine.analyze("%7B%22user%22%3A%7B%22id%22%3A42%7D%7D");
    let Disposition::Inspect(url_cands) = url.disposition else {
        panic!("url should inspect");
    };
    assert!(url_cands.iter().any(|c| c.kind == InputKind::UrlEncoded));

    let b64 = engine.analyze("eyJvayI6dHJ1ZSwiY291bnQiOjJ9");
    let Disposition::Inspect(b64_cands) = b64.disposition else {
        panic!("base64 should inspect");
    };
    assert!(b64_cands.iter().any(|c| c.kind == InputKind::Base64Encoded));

    let log = engine.analyze(r#"2026-08-09T12:00:01Z INFO response={"ok":true,"count":2}"#);
    let Disposition::Inspect(log_cands) = log.disposition else {
        panic!("log extract should inspect, got {:?}", log.disposition);
    };
    assert!(log_cands
        .iter()
        .any(|c| c.kind == InputKind::EmbeddedJSON && c.text == r#"{"ok":true,"count":2}"#));
}
