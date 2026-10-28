use json_core::{
    AnalysisEngine, CanonicalSerializer, Disposition, InputKind, JSONValue, StrictJSONParser,
};

#[test]
fn valid_json_is_accepted_and_canonicalized() {
    let result = AnalysisEngine::new().analyze(" { \"ok\" : true } ");
    assert!(matches!(result.disposition, Disposition::Accepted));
    assert_eq!(result.normalized.as_deref(), Some(r#"{"ok":true}"#));
    assert!(result.steps.is_empty());
}

#[test]
fn escaped_json_is_recovered() {
    let result = AnalysisEngine::new().analyze(r#""{\"ok\":true}""#);
    assert!(matches!(result.disposition, Disposition::Recovered));
    assert_eq!(result.normalized.as_deref(), Some(r#"{"ok":true}"#));
}

#[test]
fn raw_escaped_object_with_nested_json_string_is_recovered() {
    let input = r#"{\\"sdk_log\\":\\"{\\\\"msg\\\\":{\\\\"ok\\\\":true}}\\"}"#;
    let result = AnalysisEngine::new().analyze(input);

    let Disposition::Recovered = result.disposition else {
        panic!("expected recovered result, got {:?}", result.disposition);
    };
    let Some(JSONValue::Object(members)) = result.value else {
        panic!("expected recovered outer object");
    };
    let sdk_log = members
        .iter()
        .find(|m| m.key == "sdk_log")
        .expect("sdk_log");
    let JSONValue::String(nested_json) = &sdk_log.value else {
        panic!("expected sdk_log string");
    };
    let nested_value = StrictJSONParser::new().parse(nested_json).unwrap();
    let JSONValue::Object(nested_members) = nested_value else {
        panic!("expected nested object");
    };
    let JSONValue::Object(msg_members) = &nested_members.first().unwrap().value else {
        panic!("expected msg object");
    };
    assert_eq!(msg_members.first().unwrap().value, JSONValue::Bool(true));
}

#[test]
fn encoded_json_is_inspect_only() {
    let result = AnalysisEngine::new().analyze("%7B%22ok%22%3Atrue%7D");
    let Disposition::Inspect(candidates) = result.disposition else {
        panic!("expected inspect result");
    };
    assert!(candidates.iter().any(|c| c.kind == InputKind::UrlEncoded));
    assert!(result.normalized.is_none());
}

#[test]
fn ambiguous_input_is_rejected() {
    let result = AnalysisEngine::new().analyze(r#"{"a":"first" "b":"second"}"#);
    assert!(matches!(result.disposition, Disposition::Rejected));
    assert!(!result.diagnostics.is_empty());
    assert!(result.value.is_none());
}

#[test]
fn pretty_serializer_uses_two_space_indent() {
    let value = StrictJSONParser::new()
        .parse(r#"{"items":[1,true,null]}"#)
        .unwrap();
    assert_eq!(
        CanonicalSerializer::pretty(&value),
        "{\n  \"items\": [\n    1,\n    true,\n    null\n  ]\n}"
    );
}

#[test]
fn package_can_represent_null() {
    assert_eq!(JSONValue::Null, JSONValue::Null);
}
