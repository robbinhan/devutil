use json_core::{JSONValue, StrictJSONParser};

#[test]
fn parser_preserves_object_order_and_duplicate_keys() {
    let value = StrictJSONParser::new()
        .parse(r#"{"b":2,"a":1,"a":3}"#)
        .unwrap();
    let JSONValue::Object(members) = value else {
        panic!("expected object");
    };
    let keys: Vec<&str> = members.iter().map(|m| m.key.as_str()).collect();
    assert_eq!(keys, ["b", "a", "a"]);
    assert_eq!(
        members.iter().map(|m| m.value.clone()).collect::<Vec<_>>(),
        vec![
            JSONValue::Number { raw: "2".into() },
            JSONValue::Number { raw: "1".into() },
            JSONValue::Number { raw: "3".into() },
        ]
    );
}

#[test]
fn parser_reads_all_json_value_kinds() {
    let value = StrictJSONParser::new()
        .parse(r#"{"string":"hello","number":-1.25e+2,"bool":true,"empty":null,"array":[false]}"#)
        .unwrap();
    let JSONValue::Object(members) = value else {
        panic!("expected object");
    };
    assert_eq!(members.len(), 5);
}

#[test]
fn parser_rejects_leading_zero() {
    assert!(StrictJSONParser::new().parse(r#"{"value":01}"#).is_err());
}

#[test]
fn parser_reports_source_location() {
    let err = StrictJSONParser::new()
        .parse("{\n  \"value\": 01\n}")
        .unwrap_err();
    assert_eq!(err.line, 2);
    assert!(err.column > 1);
}
