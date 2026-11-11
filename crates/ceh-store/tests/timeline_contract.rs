#[test]
fn serial_timeline_matches_python_without_mutating_facts() {
    let data: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/fixtures/timeline.json"
    ))
    .unwrap();
    for case in data["cases"].as_array().unwrap() {
        let input = case["input"].clone();
        let actual = ceh_store::timeline::build_timeline(
            input["rows"].as_array().unwrap(),
            input["sessions"].as_array().unwrap(),
        )
        .unwrap();
        if actual != case["expected"] {
            panic!(
                "{}: {}",
                case["id"],
                difference(&actual, &case["expected"], "root").unwrap()
            );
        }
        assert_eq!(input, case["input"]);
    }
}

fn difference(a: &serde_json::Value, b: &serde_json::Value, path: &str) -> Option<String> {
    if a == b {
        return None;
    }
    if let (Some(a), Some(b)) = (a.as_array(), b.as_array()) {
        if a.len() != b.len() {
            return Some(format!("{path}: length {} != {}", a.len(), b.len()));
        }
        for (i, (a, b)) in a.iter().zip(b).enumerate() {
            if let Some(diff) = difference(a, b, &format!("{path}[{i}]")) {
                return Some(diff);
            }
        }
    }
    if let (Some(a), Some(b)) = (a.as_object(), b.as_object()) {
        for key in a.keys().chain(b.keys()) {
            if let Some(diff) = difference(
                a.get(key).unwrap_or(&serde_json::Value::Null),
                b.get(key).unwrap_or(&serde_json::Value::Null),
                &format!("{path}.{key}"),
            ) {
                return Some(diff);
            }
        }
    }
    Some(format!("{path}: {a} != {b}"))
}
