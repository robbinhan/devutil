use json_core::{CandidateNormalizer, InputKind};

#[test]
fn escaped_json_string_produces_automatic_candidate() {
    let input = r#""{\"user\":{\"id\":42}}""#;
    let candidates = CandidateNormalizer::new().generate_candidates(input);
    assert!(candidates.iter().any(|c| {
        c.kind == InputKind::QuotedJSON
            && c.text == r#"{"user":{"id":42}}"#
            && !c.requires_user_action
    }));
}

#[test]
fn raw_multi_escaped_object_produces_automatic_candidate() {
    let input = r#"{\\"a\\":\\"b\\"}"#;
    let candidates = CandidateNormalizer::new().generate_candidates(input);
    assert!(candidates.iter().any(|c| {
        c.kind == InputKind::EscapedJSON && c.text == r#"{"a":"b"}"# && !c.requires_user_action
    }));
}

#[test]
fn markdown_fence_produces_automatic_candidate() {
    let input = "```json\n{\"ok\":true}\n```";
    let candidates = CandidateNormalizer::new().generate_candidates(input);
    assert!(candidates
        .iter()
        .any(|c| c.kind == InputKind::MarkdownFence && c.text == r#"{"ok":true}"#));
}

#[test]
fn url_and_base64_candidates_require_user_action() {
    let url_candidates = CandidateNormalizer::new().generate_candidates("%7B%22ok%22%3Atrue%7D");
    assert!(url_candidates
        .iter()
        .any(|c| c.kind == InputKind::UrlEncoded && c.requires_user_action));

    let base64_candidates = CandidateNormalizer::new().generate_candidates("eyJvayI6dHJ1ZX0=");
    assert!(base64_candidates
        .iter()
        .any(|c| c.kind == InputKind::Base64Encoded && c.requires_user_action));
}

#[test]
fn newline_delimited_json_produces_array_candidate() {
    let input = "{\"id\":1}\n{\"id\":2}";
    let candidates = CandidateNormalizer::new().generate_candidates(input);
    assert!(candidates
        .iter()
        .any(|c| c.kind == InputKind::Ndjson && c.text == r#"[{"id":1},{"id":2}]"#));
}
