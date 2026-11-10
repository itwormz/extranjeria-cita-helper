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

fn kind_of(text: &str, status: Option<u16>, matches: bool) -> ceh_core::model::PageKind {
    let snapshot: PageSnapshot = serde_json::from_value(serde_json::json!({
        "visible": ["body", "#form"],
        "frames": [{"text": text, "status": status, "response_matches": matches}]
    }))
    .unwrap();
    recognize(&snapshot).unwrap().kind
}

#[test]
fn http_403_is_a_block_even_without_block_text_unless_the_page_shows_a_captcha() {
    use ceh_core::model::PageKind;
    // Python: a 403 entry response raises HTTP_BLOCKED after the challenge check (captcha text wins).
    assert_eq!(kind_of("Servicio de cita previa", Some(403), true), PageKind::Blocked);
    assert_eq!(kind_of("Introduzca el código de la imagen", Some(403), true), PageKind::Captcha);
    // A status that belongs to another navigation, or any other status, never blocks by itself.
    assert_ne!(kind_of("Servicio de cita previa", Some(403), false), PageKind::Blocked);
    assert_ne!(kind_of("Servicio de cita previa", Some(200), true), PageKind::Blocked);
    assert_ne!(kind_of("Servicio de cita previa", None, true), PageKind::Blocked);
}

#[test]
fn an_empty_403_entry_response_is_blocked_and_only_the_entry_frame_counts() {
    use ceh_core::model::PageKind;
    // Nothing is visible (an empty 403 body has no height): still a block, not "loading".
    let empty: PageSnapshot = serde_json::from_value(serde_json::json!({
        "visible": [], "frames": [{"text": "", "status": 403, "response_matches": true}]
    }))
    .unwrap();
    assert_eq!(recognize(&empty).unwrap().kind, PageKind::Blocked);
    // A genuinely loading page without any status stays loading.
    let loading: PageSnapshot = serde_json::from_value(serde_json::json!({
        "visible": [], "frames": [{"text": ""}]
    }))
    .unwrap();
    assert_eq!(recognize(&loading).unwrap().kind, PageKind::Loading);
    // Python checks the entry response only: a 403 in a child frame does not block the page.
    let child: PageSnapshot = serde_json::from_value(serde_json::json!({
        "visible": ["body", "#form"],
        "frames": [{"text": "Servicio de cita previa", "status": 200}, {"text": "", "status": 403}]
    }))
    .unwrap();
    assert_ne!(recognize(&child).unwrap().kind, PageKind::Blocked);
}
