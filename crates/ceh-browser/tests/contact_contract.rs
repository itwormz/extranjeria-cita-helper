use ceh_browser::{BrowserError, contact::ContactInput};
use ceh_core::profiles::ProfileDraft;
use serde_json::Value;
fn profile() -> ProfileDraft {
    let cases: Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/fixtures/profile-validation.json"
    ))
    .unwrap();
    {
        let mut p: ProfileDraft =
            serde_json::from_value(cases["cases"][0]["expected"]["normalized"].clone()).unwrap();
        p.phone = "600123456".into();
        p.email = "alex@example.invalid".into();
        p
    }
}
#[test]
fn contact_data_is_bounded_before_protocol_use() {
    let mut p = profile();
    assert!(ContactInput::from_profile(&p).is_ok());
    for phone in ["", "600123456\n"] {
        p.phone = phone.into();
        assert!(matches!(
            ContactInput::from_profile(&p),
            Err(BrowserError::Unsupported)
        ));
    }
    p = profile();
    p.email = "x".repeat(255);
    assert!(matches!(
        ContactInput::from_profile(&p),
        Err(BrowserError::Unsupported)
    ));
    p.email = "alex@example.invalid\n".into();
    assert!(matches!(
        ContactInput::from_profile(&p),
        Err(BrowserError::Unsupported)
    ));
}
