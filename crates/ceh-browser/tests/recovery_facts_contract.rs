use ceh_browser::{BrowserError, recovery_controls::RecoveryRegion};
use ceh_core::model::PageKind;
use serde_json::json;
#[test]
fn recovery_scope_is_the_smallest_message_region_not_the_surrounding_form() {
    let rows = json!([
    {"index":0,"size":20,"text":"Datos del solicitante. Sesión caducada","label":"Aceptar"},
    {"index":3,"size":4,"text":"Sesión caducada","label":"Volver"}
    ]);
    let scope = RecoveryRegion::decode(&rows.to_string(), PageKind::Expired)
        .unwrap()
        .unwrap();
    assert_eq!(scope.label(), "Volver");
    assert!(
        RecoveryRegion::decode(&rows.to_string(), PageKind::ServerError)
            .unwrap()
            .is_none()
    );
}
#[test]
fn recovery_regions_are_bounded_and_reject_unknown_data() {
    for raw in [
        json!([{"index":0,"size":1,"text":"Sesión caducada","label":"Enviar"}]),
        json!([{"index":0,"size":1,"text":"Sesión caducada","label":"Aceptar","token":"PRIVATE"}]),
        json!([{"index":5000,"size":1,"text":"Sesión caducada","label":"Aceptar"}]),
        json!([{"index":0,"size":1,"text":"x".repeat(65537),"label":"Aceptar"}]),
    ] {
        assert!(matches!(
            RecoveryRegion::decode(&raw.to_string(), PageKind::Expired),
            Err(BrowserError::NoObservation)
        ));
    }
}
