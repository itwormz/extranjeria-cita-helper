use ceh_browser::{BrowserError, DispatchGate, DispatchOutcome};
use ceh_core::{control::Authority, model::*};

fn session() -> SessionId {
    SessionId::new("owned-session").unwrap()
}
fn guard() -> DocumentGuard {
    DocumentGuard {
        session: session(),
        epoch: 0,
        document: DocumentId::new("realm-one").unwrap(),
        expected_page: PageKind::Applicant,
    }
}
fn page() -> Observation {
    Observation {
        document: guard().document,
        kind: PageKind::Applicant,
        method: RequestMethod::Post,
        operation_code: false,
        variant: String::new(),
    }
}

#[test]
fn control_revokes_prepared_dispatch_without_waiting_for_browser_reply() {
    let gate = DispatchGate::new(session());
    let control = gate.clone();
    let mut authority = Authority::new(session());
    control.apply_control(&authority.pause().unwrap()).unwrap();
    let mut calls = 0;
    assert_eq!(
        gate.dispatch(&guard(), &page(), ActionKind::Applicant, || {
            calls += 1;
            DispatchOutcome::Dispatched
        }),
        Err(BrowserError::Paused)
    );
    control.apply_control(&authority.resume().unwrap()).unwrap();
    assert_eq!(
        gate.dispatch(&guard(), &page(), ActionKind::Applicant, || {
            calls += 1;
            DispatchOutcome::Dispatched
        }),
        Err(BrowserError::StaleCommand)
    );
    assert_eq!(calls, 0);
}

#[test]
fn lost_reply_reserves_submission_but_explicit_non_dispatch_can_retry() {
    let gate = DispatchGate::new(session());
    assert_eq!(
        gate.dispatch(&guard(), &page(), ActionKind::Applicant, || {
            DispatchOutcome::Unknown
        }),
        Ok(DispatchOutcome::Unknown)
    );
    let mut calls = 0;
    assert_eq!(
        gate.dispatch(&guard(), &page(), ActionKind::Applicant, || {
            calls += 1;
            DispatchOutcome::Dispatched
        }),
        Err(BrowserError::DuplicateCommand)
    );
    assert_eq!(calls, 0);
    let fresh = DispatchGate::new(session());
    assert_eq!(
        fresh.dispatch(&guard(), &page(), ActionKind::Applicant, || {
            DispatchOutcome::NotDispatched
        }),
        Ok(DispatchOutcome::NotDispatched)
    );
    assert_eq!(
        fresh.dispatch(&guard(), &page(), ActionKind::Applicant, || {
            DispatchOutcome::Dispatched
        }),
        Ok(DispatchOutcome::Dispatched)
    );
}

#[test]
fn changed_document_or_stage_never_reaches_transport() {
    let gate = DispatchGate::new(session());
    let mut changed = page();
    changed.document = DocumentId::new("realm-two").unwrap();
    assert_eq!(
        gate.dispatch(&guard(), &changed, ActionKind::Applicant, || panic!(
            "stale transport"
        )),
        Err(BrowserError::StaleDocument)
    );
    changed = page();
    changed.kind = PageKind::Booking;
    assert_eq!(
        gate.dispatch(&guard(), &changed, ActionKind::Applicant, || panic!(
            "changed stage"
        )),
        Err(BrowserError::StaleDocument)
    );
    let mut authority = Authority::new(session());
    gate.apply_control(&authority.handoff().unwrap()).unwrap();
    assert_eq!(
        gate.dispatch(&guard(), &page(), ActionKind::Applicant, || panic!(
            "manual page"
        )),
        Err(BrowserError::ManualOwnership)
    );
}
