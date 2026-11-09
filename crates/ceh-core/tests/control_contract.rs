use ceh_core::{control::*, model::*};
fn session() -> SessionId {
    SessionId::new("s").unwrap()
}
#[test]
fn pause_and_resume_revoke_prior_epochs() {
    let mut a = Authority::new(session());
    let original = a.snapshot();
    let paused = a.pause().unwrap();
    assert_eq!(paused.mode(), ControlMode::Paused);
    assert!(paused.epoch() > original.epoch());
    let resumed = a.resume().unwrap();
    assert_eq!(resumed.mode(), ControlMode::Automatic);
    assert!(resumed.epoch() > paused.epoch());
    assert_eq!(a.apply(&original), Err(ControlError::StaleUpdate));
}
#[test]
fn handoff_cannot_be_reversed_by_resume_or_newer_update() {
    let mut a = Authority::new(session());
    a.handoff().unwrap();
    assert_eq!(a.resume(), Err(ControlError::ManualOwnership));
    let mut other = Authority::new(session());
    other.pause().unwrap();
    other.resume().unwrap();
    assert_eq!(
        a.apply(&other.snapshot()),
        Err(ControlError::ManualOwnership)
    );
}
#[test]
fn control_update_must_belong_to_same_session() {
    let mut a = Authority::new(session());
    let foreign = Authority::new(SessionId::new("other").unwrap());
    assert_eq!(
        a.apply(&foreign.snapshot()),
        Err(ControlError::WrongSession)
    );
}
