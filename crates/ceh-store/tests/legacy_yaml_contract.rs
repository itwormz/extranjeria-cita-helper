use ceh_store::legacy_yaml::*;
#[test]
fn scalar_style_preserves_python_legacy_schema() {
    let doc=parse_document(b"plain: yes\nquoted: 'yes'\noctal: 0123\ntext: '0123'\nfloat: 1.0\nexp_text: 2.5e3\nnone_value: null\n").unwrap();
    let root = doc.root();
    assert_eq!(
        root.get("plain").unwrap().scalar_text().as_deref(),
        Some("True")
    );
    assert_eq!(
        root.get("quoted").unwrap().scalar_text().as_deref(),
        Some("yes")
    );
    assert_eq!(
        root.get("octal").unwrap().scalar_text().as_deref(),
        Some("83")
    );
    assert_eq!(
        root.get("text").unwrap().scalar_text().as_deref(),
        Some("0123")
    );
    assert_eq!(
        root.get("float").unwrap().scalar_text().as_deref(),
        Some("1.0")
    );
    assert_eq!(
        root.get("exp_text").unwrap().scalar_text().as_deref(),
        Some("2.5e3")
    );
    assert_eq!(
        root.get("none_value").unwrap().scalar_text().as_deref(),
        Some("None")
    );
}
#[test]
fn anchors_and_merge_keys_keep_explicit_and_first_source_precedence() {
    let doc=parse_document(b"a: &a {key: first, other: base}\nb: &b {key: second}\nmerged: {<<: [*a, *b], other: explicit}\n").unwrap();
    let merged = doc.root().get("merged").unwrap();
    assert_eq!(
        merged.get("key").unwrap().scalar_text().as_deref(),
        Some("first")
    );
    assert_eq!(
        merged.get("other").unwrap().scalar_text().as_deref(),
        Some("explicit")
    );
}
#[test]
fn last_duplicate_key_matches_legacy_loader() {
    let doc = parse_document(b"key: old\nkey: new\n").unwrap();
    assert_eq!(
        doc.root().get("key").unwrap().scalar_text().as_deref(),
        Some("new")
    );
}
#[test]
fn malformed_tags_cycles_multiple_documents_and_limits_are_redacted_errors() {
    for source in [
        "token: !private SHOULD_NOT_LEAK",
        "a: &a [*a]",
        "a: [1,",
        "---\na: 1\n---\nb: 2",
    ] {
        let error = parse_document(source.as_bytes()).expect_err("unsafe/invalid YAML accepted");
        assert!(!format!("{error:?} {error}").contains("SHOULD_NOT_LEAK"));
    }
    assert!(parse_document(&vec![b'x'; 2 * 1024 * 1024 + 1]).is_err());
    let nested = format!("{}0{}", "[".repeat(65), "]".repeat(65));
    assert!(parse_document(nested.as_bytes()).is_err());
}
#[test]
fn raw_document_debug_never_prints_secret_values() {
    let doc = parse_document(b"token: NOT_PUBLIC_VALUE").unwrap();
    assert!(!format!("{doc:?}").contains("NOT_PUBLIC_VALUE"));
    assert!(!format!("{:?}", doc.root()).contains("NOT_PUBLIC_VALUE"));
}

#[test]
fn parsed_snapshot_can_cross_runtime_threads_without_exposing_values() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<LegacyDocument>();
}

#[test]
fn legacy_scalar_schema_matches_actual_python_yaml() {
    use serde_json::{Value, json};
    let data: Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/fixtures/yaml-scalars.json"
    ))
    .unwrap();
    for case in data["cases"].as_array().unwrap() {
        let actual = match parse_document(case["source"].as_str().unwrap().as_bytes()) {
            Ok(doc) => {
                let root = doc.root();
                let kind = match root {
                    RawValue::Null => "NoneType",
                    RawValue::Bool(_) => "bool",
                    RawValue::Integer(_) => "int",
                    RawValue::Float(_) => "float",
                    RawValue::String(_) => "str",
                    RawValue::Timestamp(s) if s.contains(' ') => "datetime",
                    RawValue::Timestamp(_) => "date",
                    _ => panic!("non-scalar fixture"),
                };
                json!({"kind":kind,"text":root.scalar_text().unwrap()})
            }
            Err(_) => json!({"error":true}),
        };
        assert_eq!(actual, case["expected"], "{}", case["id"]);
    }
}

#[test]
fn repeated_merges_have_a_total_key_copy_budget() {
    let source = format!(
        "base: &base {{{}: x}}\nmerged: {{<<: [{}]}}\n",
        "k".repeat(1024),
        vec!["*base"; 5000].join(",")
    );
    assert!(matches!(
        parse_document(source.as_bytes()),
        Err(ReadError::ResourceLimit)
    ));
}
