use json_core::RecoveryParser;

#[test]
fn single_quotes_and_unquoted_keys() {
    let input = "{name: 'Alice', roles: ['admin', 'viewer']}";
    let result = RecoveryParser::new().repair(input).unwrap();
    assert_eq!(
        result.repaired_text,
        r#"{"name":"Alice","roles":["admin","viewer"]}"#
    );
}

#[test]
fn trailing_commas_and_comments() {
    let input = "{\n // user id\n \"id\": 42,\n /* state */\n \"active\": true,\n}";
    let result = RecoveryParser::new().repair(input).unwrap();
    assert_eq!(result.repaired_text, r#"{"id":42,"active":true}"#);
}

#[test]
fn jsonp_and_truncated_containers() {
    let jsonp = RecoveryParser::new()
        .repair("callback({\"ok\":true});")
        .unwrap();
    assert_eq!(jsonp.repaired_text, r#"{"ok":true}"#);

    let truncated = RecoveryParser::new()
        .repair(r#"{"users":[{"id":1,"name":"A"},{"id":2,"name":"B"}"#)
        .unwrap();
    assert_eq!(
        truncated.repaired_text,
        r#"{"users":[{"id":1,"name":"A"},{"id":2,"name":"B"}]}"#
    );
}

#[test]
fn ndjson_becomes_an_array() {
    let result = RecoveryParser::new()
        .repair("{\"id\":1}\n{\"id\":2}")
        .unwrap();
    assert_eq!(result.repaired_text, r#"[{"id":1},{"id":2}]"#);
}

#[test]
fn ambiguous_input_is_rejected() {
    assert!(RecoveryParser::new()
        .repair(r#"{"a":"first" "b":"second"}"#)
        .is_err());
    assert!(RecoveryParser::new().repair(r#"{"value":01}"#).is_err());
    assert!(RecoveryParser::new()
        .repair(r#"{"message":"this text was cut off"#)
        .is_err());
}
