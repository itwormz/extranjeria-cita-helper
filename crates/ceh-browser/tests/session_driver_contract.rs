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

use ceh_browser::session_driver::recovery_memory;
use ceh_core::{model::{ActionKind, DocumentId}, recovery::RecoveryAction};
#[test]
fn a_reload_keeps_the_action_it_repeats_and_a_recovery_get_forgets_it() {
    let doc = DocumentId::new("doc").unwrap();
    let before = (Some(ActionKind::Request), Some((doc.clone(), ActionKind::Request)));
    // Python: the repeated availability search stays a "request" for the next retry step.
    assert_eq!(recovery_memory(RecoveryAction::Reload, before.0, before.1.clone()), (Some(ActionKind::Request), None));
    let pending_only = (None, Some((doc.clone(), ActionKind::Request)));
    assert_eq!(recovery_memory(RecoveryAction::Reload, pending_only.0, pending_only.1), (Some(ActionKind::Request), None));
    // `last_action = pending_action = None` after the recovery GET.
    for action in [RecoveryAction::GetCurrent, RecoveryAction::GetEntry] {
        assert_eq!(recovery_memory(action, before.0, before.1.clone()), (None, None));
    }
}
