use ceh_browser::{BrowserError, DispatchGate, DispatchOutcome};
use ceh_core::{control::Authority, model::*};
fn session() -> SessionId {
    SessionId::new("phase-session").unwrap()
}
fn guard() -> DocumentGuard {
    DocumentGuard {
        session: session(),
        epoch: 0,
        document: DocumentId::new("page-one").unwrap(),
        expected_page: PageKind::Province,
    }
}
fn page() -> Observation {
    Observation {
        document: guard().document,
        kind: PageKind::Province,
        method: RequestMethod::Get,
        operation_code: false,
        variant: String::new(),
    }
}
#[test]
fn compound_phase_rechecks_control_between_select_and_submit() {
    let gate = DispatchGate::new(session());
    let mut phase = gate.begin(&guard(), &page(), ActionKind::Province).unwrap();
    assert!(matches!(
        gate.begin(&guard(), &page(), ActionKind::Province),
        Err(BrowserError::DuplicateCommand)
    ));
    assert_eq!(
        phase.send(&page(), || DispatchOutcome::Unknown).unwrap(),
        DispatchOutcome::Unknown
    );
    phase.complete_step(DispatchOutcome::Dispatched).unwrap();
    let mut authority = Authority::new(session());
    gate.apply_control(&authority.pause().unwrap()).unwrap();
    assert_eq!(
        phase.send(&page(), || panic!("paused submit")),
        Err(BrowserError::Paused)
    );
    assert_eq!(phase.finish(), DispatchOutcome::Dispatched);
    gate.apply_control(&authority.resume().unwrap()).unwrap();
    let mut fresh = guard();
    fresh.epoch = authority.snapshot().epoch();
    assert!(matches!(
        gate.begin(&fresh, &page(), ActionKind::Province),
        Err(BrowserError::DuplicateCommand)
    ));
}
#[test]
fn one_phase_can_select_then_submit_without_bypassing_guard() {
    let gate = DispatchGate::new(session());
    let mut phase = gate.begin(&guard(), &page(), ActionKind::Province).unwrap();
    for _ in 0..2 {
        phase.send(&page(), || DispatchOutcome::Unknown).unwrap();
        phase.complete_step(DispatchOutcome::Dispatched).unwrap();
    }
    assert_eq!(phase.finish(), DispatchOutcome::Dispatched);
    assert!(matches!(
        gate.begin(&guard(), &page(), ActionKind::Province),
        Err(BrowserError::DuplicateCommand)
    ));
}
#[test]
fn navigation_and_unknown_reply_prevent_remaining_steps() {
    let gate = DispatchGate::new(session());
    let mut phase = gate.begin(&guard(), &page(), ActionKind::Province).unwrap();
    phase.send(&page(), || DispatchOutcome::Unknown).unwrap();
    assert_eq!(
        phase.send(&page(), || panic!("outstanding reply")),
        Err(BrowserError::InvalidControl)
    );
    phase.complete_step(DispatchOutcome::Dispatched).unwrap();
    let mut changed = page();
    changed.document = DocumentId::new("page-two").unwrap();
    assert_eq!(
        phase.send(&changed, || panic!("new document")),
        Err(BrowserError::StaleDocument)
    );
    assert_eq!(phase.finish(), DispatchOutcome::Dispatched);
    let other = DispatchGate::new(session());
    let mut phase = other
        .begin(&guard(), &page(), ActionKind::Province)
        .unwrap();
    phase.send(&page(), || DispatchOutcome::Unknown).unwrap();
    phase.complete_step(DispatchOutcome::Unknown).unwrap();
    assert_eq!(
        phase.send(&page(), || panic!("ambiguous retry")),
        Err(BrowserError::DuplicateCommand)
    );
    assert_eq!(phase.finish(), DispatchOutcome::Unknown);
}
#[test]
fn only_wholly_proven_non_delivery_releases_reservation() {
    let gate = DispatchGate::new(session());
    {
        let _never_sent = gate.begin(&guard(), &page(), ActionKind::Province).unwrap();
    }
    let mut phase = gate.begin(&guard(), &page(), ActionKind::Province).unwrap();
    phase.send(&page(), || DispatchOutcome::Unknown).unwrap();
    phase.complete_step(DispatchOutcome::NotDispatched).unwrap();
    assert_eq!(phase.finish(), DispatchOutcome::NotDispatched);
    let mut phase = gate.begin(&guard(), &page(), ActionKind::Province).unwrap();
    phase.send(&page(), || DispatchOutcome::Unknown).unwrap();
    phase.complete_step(DispatchOutcome::Dispatched).unwrap();
    phase.send(&page(), || DispatchOutcome::Unknown).unwrap();
    phase.complete_step(DispatchOutcome::NotDispatched).unwrap();
    assert_eq!(phase.finish(), DispatchOutcome::Dispatched);
    assert!(matches!(
        gate.begin(&guard(), &page(), ActionKind::Province),
        Err(BrowserError::DuplicateCommand)
    ));
}

#[test]
fn acknowledged_preparation_can_resume_but_unknown_preparation_cannot_replay() {
    let gate = DispatchGate::new(session());
    let mut phase = gate
        .begin_preparation(&guard(), &page(), ActionKind::Province)
        .unwrap();
    phase.send(&page(), || DispatchOutcome::Unknown).unwrap();
    phase.complete_step(DispatchOutcome::Dispatched).unwrap();
    let mut authority = Authority::new(session());
    gate.apply_control(&authority.pause().unwrap()).unwrap();
    drop(phase);
    gate.apply_control(&authority.resume().unwrap()).unwrap();
    let mut fresh = guard();
    fresh.epoch = authority.snapshot().epoch();
    let mut phase = gate
        .begin_preparation(&fresh, &page(), ActionKind::Province)
        .unwrap();
    phase.send(&page(), || DispatchOutcome::Unknown).unwrap();
    phase.complete_step(DispatchOutcome::Unknown).unwrap();
    drop(phase);
    assert!(matches!(
        gate.begin_preparation(&fresh, &page(), ActionKind::Province),
        Err(BrowserError::DuplicateCommand)
    ));
    // A preparation reservation never serves as proof that the final submit was sent.
    let submit = gate.begin(&fresh, &page(), ActionKind::Province).unwrap();
    assert_eq!(submit.finish(), DispatchOutcome::NotDispatched);
}

#[test]
fn ui_interrupt_revokes_between_steps_without_racing_the_core_epoch() {
    let gate = DispatchGate::new(session());
    let mut phase = gate.begin(&guard(), &page(), ActionKind::Province).unwrap();
    phase.send(&page(), || DispatchOutcome::Unknown).unwrap();
    phase.complete_step(DispatchOutcome::Dispatched).unwrap();
    let ticket = gate.interrupt().unwrap();
    assert_eq!(
        phase.send(&page(), || panic!("interrupted write")),
        Err(BrowserError::Paused)
    );
    let mut authority = Authority::new(session());
    let paused = authority.pause().unwrap();
    gate.apply_control(&paused).unwrap();
    assert!(gate.clear_interrupt(&paused, ticket).is_err());
    let resumed = authority.resume().unwrap();
    gate.apply_control(&resumed).unwrap();
    gate.clear_interrupt(&resumed, ticket).unwrap();
    assert!(gate.clear_interrupt(&paused, ticket).is_err());
}

#[test]
fn stale_resume_cannot_clear_a_newer_ui_interrupt() {
    let gate = DispatchGate::new(session());
    let first = gate.interrupt().unwrap();
    let mut authority = Authority::new(session());
    let paused = authority.pause().unwrap();
    gate.apply_control(&paused).unwrap();
    let resumed = authority.resume().unwrap();
    gate.apply_control(&resumed).unwrap();
    let second = gate.interrupt().unwrap();
    assert!(gate.clear_interrupt(&resumed, first).is_err());
    assert!(matches!(
        gate.begin(&guard(), &page(), ActionKind::Province),
        Err(BrowserError::Paused)
    ));
    gate.clear_interrupt(&resumed, second).unwrap();
}
