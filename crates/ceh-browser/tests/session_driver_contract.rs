use ceh_browser::{BrowserError, DispatchGate, session_driver::SessionDriver};
use ceh_core::{flow::FlowEffect, model::*, profiles::ProfileDraft, timings::AssistTimings};
fn profile() -> ProfileDraft {
    let v: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/fixtures/profile-validation.json"
    ))
    .unwrap();
    serde_json::from_value(v["cases"][0]["expected"]["normalized"].clone()).unwrap()
}
#[test]
fn driver_uses_profile_policies_and_rejects_pause_resume_after_permanent_handoff() {
    let session = SessionId::new("driver").unwrap();
    let gate = DispatchGate::new(session.clone());
    let mut driver = SessionDriver::new(
        "owned-context",
        session,
        profile(),
        AssistTimings::default(),
        gate,
    )
    .unwrap();
    let paused = driver.pause(10).unwrap();
    assert_eq!(paused.mode(), ceh_core::control::ControlMode::Paused);
    assert!(driver.resume(9).is_err());
    assert_eq!(driver.resume(20).unwrap().epoch(), 2);
    let booking = Observation {
        document: DocumentId::new("booking").unwrap(),
        kind: PageKind::Booking,
        method: RequestMethod::Get,
        operation_code: false,
        variant: String::new(),
    };
    let effects = driver
        .accept_observation(booking, Default::default(), 30)
        .unwrap();
    assert!(effects.contains(&FlowEffect::Handoff));
    assert!(matches!(
        driver.resume(40),
        Err(BrowserError::ManualOwnership)
    ));
}
