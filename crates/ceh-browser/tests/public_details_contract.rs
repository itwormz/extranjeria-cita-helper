use ceh_browser::{
    BrowserError,
    public_details::{OfficeFacts, decode_offers},
};
use serde_json::json;
#[test]
fn offices_accept_only_bounded_public_labels_and_selected_member() {
    let facts = OfficeFacts::decode(
        &json!({"offices":[" Office A ","Office A","Office B"],"selected":"Office B"}).to_string(),
    )
    .unwrap();
    assert_eq!(facts.offices, ["Office A", "Office A", "Office B"]);
    assert_eq!(facts.selected.as_deref(), Some("Office B"));
    for raw in [
        json!({"offices":["Office A"],"selected":"UNKNOWN"}),
        json!({"offices":["Office A"],"selected":null,"token":"PRIVATE"}),
        json!({"offices":vec!["Office";129],"selected":null}),
        json!({"offices":["X".repeat(321)],"selected":null}),
    ] {
        assert_eq!(
            OfficeFacts::decode(&raw.to_string()),
            Err(BrowserError::NoObservation)
        );
    }
}
#[test]
fn offers_are_explicit_labels_not_filters_or_private_fields() {
    let raw = json!([
        "Día: 02/11/2026 Hora: 09:30",
        "Día: 02/11/2026 Hora: 09:30",
        "09:00–14:00",
        "Día: 31/02/2026 Hora: 09:30"
    ]);
    let details = decode_offers(&raw.to_string(), Some("Office A")).unwrap();
    assert_eq!(details.offers.len(), 1);
    assert_eq!(details.office.as_deref(), Some("Office A"));
    assert!(
        decode_offers("[]", Some("Office A"))
            .unwrap()
            .office
            .is_none()
    );
    for raw in [
        json!({"token":"PRIVATE"}),
        json!(vec!["label"; 33]),
        json!(["x".repeat(241)]),
        json!([{"confirmation":"PRIVATE"}]),
    ] {
        assert_eq!(
            decode_offers(&raw.to_string(), None),
            Err(BrowserError::NoObservation)
        );
    }
}
