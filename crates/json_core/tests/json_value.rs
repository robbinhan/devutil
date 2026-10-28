use json_core::{JSONMember, JSONValue};

#[test]
fn object_members_preserve_order_and_duplicate_keys() {
    let value = JSONValue::Object(vec![
        JSONMember::new("b", JSONValue::Number { raw: "2".into() }),
        JSONMember::new("a", JSONValue::Number { raw: "1".into() }),
        JSONMember::new("a", JSONValue::Number { raw: "3".into() }),
    ]);

    let JSONValue::Object(members) = value else {
        panic!("expected object");
    };
    let keys: Vec<&str> = members.iter().map(|m| m.key.as_str()).collect();
    assert_eq!(keys, ["b", "a", "a"]);
}
