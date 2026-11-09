use ceh_browser::{BrowserError, applicant::ApplicantInput};
use ceh_core::profiles::ProfileDraft;
use serde_json::Value;
fn profile() -> ProfileDraft {
    let cases: Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/fixtures/profile-validation.json"
    ))
    .unwrap();
    serde_json::from_value(cases["cases"][0]["expected"]["normalized"].clone()).unwrap()
}
#[test]
fn applicant_data_respects_service_document_and_copy_contract() {
    let mut p = profile();
    assert!(ApplicantInput::from_profile(&p).is_ok());
    p.procedure = "regreso".into();
    p.nationality.clear();
    for kind in ["NIE", "DNI", "PASSPORT"] {
        p.document_type = kind.into();
        assert!(ApplicantInput::from_profile(&p).is_ok());
    }
    p.document_type = "OTHER".into();
    assert!(matches!(
        ApplicantInput::from_profile(&p),
        Err(BrowserError::Unsupported)
    ));
    p.fill_mode = "copy".into();
    p.access_mode = "certificate".into();
    p.document_number.clear();
    p.full_name.clear();
    assert!(ApplicantInput::from_profile(&p).is_ok());
    p.access_mode = "none".into();
    assert!(matches!(
        ApplicantInput::from_profile(&p),
        Err(BrowserError::Unsupported)
    ));
}
#[test]
fn malformed_manual_data_is_rejected_before_protocol_use() {
    let mut p = profile();
    p.fill_mode = "manual".into();
    p.document_number.clear();
    assert!(matches!(
        ApplicantInput::from_profile(&p),
        Err(BrowserError::Unsupported)
    ));
    p = profile();
    p.full_name = "x".repeat(1025);
    assert!(matches!(
        ApplicantInput::from_profile(&p),
        Err(BrowserError::Unsupported)
    ));
    p = profile();
    p.nationality.clear();
    assert!(matches!(
        ApplicantInput::from_profile(&p),
        Err(BrowserError::Unsupported)
    ));
}
