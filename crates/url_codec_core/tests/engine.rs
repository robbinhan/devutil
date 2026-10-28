use url_codec_core::{
    decode, encode, parse_query_string, parse_structured, UrlCodecEngine, UrlCodecMode,
};

#[test]
fn decode_percent_and_plus_as_form_urlencoded() {
    let result = UrlCodecEngine::new().convert("hello%20world%21", UrlCodecMode::Decode);
    assert_eq!(result.output.as_deref(), Some("hello world!"));
    assert_eq!(result.diagnostic, None);
    // Plain prose should not get a structure table.
    assert_eq!(result.structure, None);

    let plus = decode("a+b%2Bc").expect("decode");
    assert_eq!(plus, "a b+c");
}

#[test]
fn encode_spaces_as_plus_and_escapes_reserved() {
    let result = UrlCodecEngine::new().convert("hello world!", UrlCodecMode::Encode);
    assert_eq!(result.output.as_deref(), Some("hello+world%21"));
    assert_eq!(result.diagnostic, None);
    assert_eq!(result.structure, None);

    let encoded = encode("a b+c").expect("encode");
    assert_eq!(encoded, "a+b%2Bc");
}

#[test]
fn encode_decode_round_trip_preserves_utf8() {
    let samples = [
        "plain",
        "hello world!",
        "a=1&b=two",
        "中文测试",
        "emoji 😀",
        "path/with spaces?q=1",
        "already+%2Bencoded",
    ];
    for sample in samples {
        let encoded = encode(sample).expect("encode");
        let decoded = decode(&encoded).expect("decode");
        assert_eq!(decoded, sample, "round-trip failed for {sample:?}");
    }
}

#[test]
fn incomplete_and_invalid_escapes_return_diagnostics() {
    let engine = UrlCodecEngine::new();
    let incomplete = engine.convert("bad%2", UrlCodecMode::Decode);
    assert_eq!(
        incomplete.diagnostic.as_deref(),
        Some("Incomplete percent-escape sequence.")
    );
    assert_eq!(incomplete.output, None);
    assert_eq!(incomplete.structure, None);

    let invalid = engine.convert("bad%zz", UrlCodecMode::Decode);
    assert_eq!(
        invalid.diagnostic.as_deref(),
        Some("Invalid percent-escape sequence.")
    );
    assert_eq!(invalid.output, None);
    assert_eq!(invalid.structure, None);
}

#[test]
fn empty_input_returns_diagnostic_without_output() {
    let empty = UrlCodecEngine::new().convert("", UrlCodecMode::Decode);
    assert_eq!(
        empty.diagnostic.as_deref(),
        Some("Enter text to encode or decode.")
    );
    assert_eq!(empty.output, None);
    assert_eq!(empty.structure, None);
}

#[test]
fn lowercase_hex_escapes_decode() {
    assert_eq!(decode("%2f%2F").expect("decode"), "//");
}

#[test]
fn unreserved_characters_pass_through_on_encode() {
    assert_eq!(
        encode("ABC-xyz_123.~").expect("encode"),
        "ABC-xyz_123.~"
    );
}

#[test]
fn non_utf8_decoded_bytes_are_rejected() {
    let err = decode("%80").expect_err("must reject");
    assert_eq!(err.message, "Decoded bytes are not valid UTF-8.");
}

#[test]
fn decode_search_url_produces_query_param_table() {
    let encoded = concat!(
        "https%3A%2F%2Fexample.com%2Fsearch%3F",
        "q%3Dhello%20world%26",
        "page%3D2%26",
        "lang%3Den%26",
        "tag%3Dspring%2Bsale%26",
        "tag%3Dextra"
    );
    let result = UrlCodecEngine::new().convert(encoded, UrlCodecMode::Decode);
    assert!(result.diagnostic.is_none());
    let output = result.output.expect("decoded output");
    assert!(output.starts_with("https://example.com/search?"));

    let structure = result.structure.expect("structured view");
    assert_eq!(structure.scheme.as_deref(), Some("https"));
    assert_eq!(structure.host.as_deref(), Some("example.com"));
    assert_eq!(structure.path.as_deref(), Some("/search"));

    let keys: Vec<_> = structure
        .query_params
        .iter()
        .map(|p| (p.key.as_str(), p.value.as_str()))
        .collect();
    assert_eq!(
        keys,
        vec![
            ("q", "hello world"),
            ("page", "2"),
            ("lang", "en"),
            ("tag", "spring sale"),
            ("tag", "extra"),
        ]
    );
}

#[test]
fn parse_structured_bare_query_string() {
    let structure = parse_structured("q=hello&page=2&lang=").expect("query");
    assert_eq!(structure.scheme, None);
    assert_eq!(structure.host, None);
    assert_eq!(structure.path, None);
    assert_eq!(structure.query_params.len(), 3);
    assert_eq!(structure.query_params[0].key, "q");
    assert_eq!(structure.query_params[0].value, "hello");
    assert_eq!(structure.query_params[1].key, "page");
    assert_eq!(structure.query_params[1].value, "2");
    assert_eq!(structure.query_params[2].key, "lang");
    assert_eq!(structure.query_params[2].value, "");
}

#[test]
fn parse_structured_keeps_duplicate_keys() {
    let params = parse_query_string("tag=a&tag=b&tag=c");
    assert_eq!(params.len(), 3);
    assert!(params.iter().all(|p| p.key == "tag"));
    assert_eq!(
        params.iter().map(|p| p.value.as_str()).collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
}

#[test]
fn parse_structured_rejects_plain_prose() {
    assert_eq!(parse_structured("hello world!"), None);
    assert_eq!(parse_structured("just some notes"), None);
}

#[test]
fn decode_failure_has_no_structure() {
    let result = UrlCodecEngine::new().convert("nope%", UrlCodecMode::Decode);
    assert!(result.diagnostic.is_some());
    assert_eq!(result.output, None);
    assert_eq!(result.structure, None);
}

fn keys_of(structure: &url_codec_core::UrlStructure) -> Vec<&str> {
    structure.query_params.iter().map(|p| p.key.as_str()).collect()
}

fn value_of<'a>(structure: &'a url_codec_core::UrlStructure, key: &str) -> Option<&'a str> {
    structure
        .query_params
        .iter()
        .find(|p| p.key == key)
        .map(|p| p.value.as_str())
}

#[test]
fn parse_structured_path_only_relative_url() {
    let structure =
        parse_structured("/search?q=hello&page=2&lang=&sort=asc").expect("relative url");
    assert_eq!(structure.scheme, None);
    assert_eq!(structure.host, None);
    assert_eq!(structure.path.as_deref(), Some("/search"));
    assert_eq!(keys_of(&structure), vec!["q", "page", "lang", "sort"]);
    assert_eq!(value_of(&structure, "q"), Some("hello"));
    assert_eq!(value_of(&structure, "lang"), Some(""));
}

#[test]
fn parse_structured_relative_without_leading_slash_and_query_only() {
    let rel = parse_structured("api/v2/items?id=7&x=").expect("relative");
    assert_eq!(rel.path.as_deref(), Some("api/v2/items"));
    assert_eq!(keys_of(&rel), vec!["id", "x"]);

    let q = parse_structured("?a=1&b=2").expect("query only");
    assert_eq!(q.path, None);
    assert_eq!(keys_of(&q), vec!["a", "b"]);

    let proto_rel = parse_structured("//cdn.example.com/a.js?v=3").expect("protocol-relative");
    assert_eq!(proto_rel.scheme, None);
    assert_eq!(proto_rel.host.as_deref(), Some("cdn.example.com"));
    assert_eq!(proto_rel.path.as_deref(), Some("/a.js"));
    assert_eq!(keys_of(&proto_rel), vec!["v"]);

    let path_only = parse_structured("/usr/local/bin").expect("path only");
    assert_eq!(path_only.path.as_deref(), Some("/usr/local/bin"));
    assert!(path_only.query_params.is_empty());
}

#[test]
fn parse_structured_full_url() {
    let structure =
        parse_structured("https://user@example.com:8443/a/b?x=1&y=two%20words#frag").expect("url");
    assert_eq!(structure.scheme.as_deref(), Some("https"));
    assert_eq!(structure.host.as_deref(), Some("example.com:8443"));
    assert_eq!(structure.path.as_deref(), Some("/a/b"));
    assert_eq!(keys_of(&structure), vec!["x", "y"]);
    assert_eq!(value_of(&structure, "y"), Some("two words"));
}

#[test]
fn parse_structured_bare_query_string_has_no_path() {
    let structure = parse_structured("q=hello&page=2&lang=en").expect("query");
    assert_eq!(structure.path, None);
    assert_eq!(keys_of(&structure), vec!["q", "page", "lang"]);
}

#[test]
fn parse_structured_nested_url_in_value_splits_on_first_question_mark() {
    let relative = parse_structured(
        "/search?q=hello&next=https://example.com/search?lang=en&page=2",
    )
    .expect("relative");
    assert_eq!(relative.path.as_deref(), Some("/search"));
    assert_eq!(keys_of(&relative), vec!["q", "next", "page"]);
    assert_eq!(
        value_of(&relative, "next"),
        Some("https://example.com/search?lang=en")
    );

    let absolute =
        parse_structured("https://a.example/p?cb=https://b.example/q?x=1&y=2").expect("absolute");
    assert_eq!(absolute.path.as_deref(), Some("/p"));
    assert_eq!(keys_of(&absolute), vec!["cb", "y"]);
    assert_eq!(value_of(&absolute, "cb"), Some("https://b.example/q?x=1"));

    let bare = parse_structured("a=1&cb=https://b.example/q?x=1&z=3").expect("bare");
    assert_eq!(bare.path, None);
    assert_eq!(keys_of(&bare), vec!["a", "cb", "z"]);
    assert_eq!(value_of(&bare, "cb"), Some("https://b.example/q?x=1"));
}

#[test]
fn decode_prefers_raw_split_so_encoded_nested_params_stay_in_value() {
    let input = "/v1/x?cb=https%3A%2F%2Fh.example%2Fcb%3Fu%3D1%26v%3D2&os=0&ua=Mozilla%2F5.0+(iPhone)";
    let result = UrlCodecEngine::new().convert(input, UrlCodecMode::Decode);
    let structure = result.structure.expect("structure");
    assert_eq!(structure.path.as_deref(), Some("/v1/x"));
    assert_eq!(keys_of(&structure), vec!["cb", "os", "ua"]);
    assert_eq!(value_of(&structure, "cb"), Some("https://h.example/cb?u=1&v=2"));
    assert_eq!(value_of(&structure, "ua"), Some("Mozilla/5.0 (iPhone)"));
}

#[test]
fn decode_fully_encoded_relative_url_uses_decoded_structure() {
    let result = UrlCodecEngine::new().convert("%2Fv1%2Fx%3Fa%3D1%26b%3D2", UrlCodecMode::Decode);
    let structure = result.structure.expect("structure");
    assert_eq!(structure.path.as_deref(), Some("/v1/x"));
    assert_eq!(keys_of(&structure), vec!["a", "b"]);
}

#[test]
fn parse_structured_rejects_prose_with_question_mark() {
    assert_eq!(parse_structured("Hello?"), None);
    assert_eq!(parse_structured("what is this? not a url"), None);
}
