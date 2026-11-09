use ceh_core::recognition::{PageSnapshot, recognize};
use serde_json::Value;

#[test]
fn rust_recognition_matches_real_python_priorities() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/fixtures/recognition.json"
    ))
    .unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let snapshot: PageSnapshot = serde_json::from_value(case["snapshot"].clone()).unwrap();
        let result = recognize(&snapshot).unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            case["expected"],
            "fixture {}",
            case["id"]
        );
    }
}

#[test]
fn empty_frame_list_is_rejected_instead_of_panicking() {
    let snapshot: PageSnapshot = serde_json::from_str(r#"{"visible":[],"frames":[]}"#).unwrap();
    assert!(recognize(&snapshot).is_err());
}
