use ceh_browser::{BrowserError, BrowserPort, ReplayBrowser};
use ceh_core::model::*;
use std::{
    future::Future,
    pin::pin,
    task::{Context, Poll, Waker},
};

fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = pin!(future);
    let waker = Waker::noop();
    match future.as_mut().poll(&mut Context::from_waker(waker)) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("replay unexpectedly requires an async runtime or external I/O"),
    }
}
fn observation(document: &str) -> Observation {
    Observation {
        document: DocumentId::new(document).unwrap(),
        kind: PageKind::Applicant,
        method: RequestMethod::Get,
        operation_code: false,
        variant: String::new(),
    }
}
fn guard(session: &str, document: &str) -> DocumentGuard {
    DocumentGuard {
        session: SessionId::new(session).unwrap(),
        epoch: 0,
        document: DocumentId::new(document).unwrap(),
        expected_page: PageKind::Applicant,
    }
}

#[test]
fn queued_action_must_match_current_session_and_document() {
    let mut browser = ReplayBrowser::new(
        SessionId::new("session-1").unwrap(),
        vec![observation("doc-1"), observation("doc-2")],
    );
    assert_eq!(ready(browser.observe()).unwrap(), observation("doc-1"));
    ready(browser.execute(&guard("session-1", "doc-1"), ActionKind::Applicant)).unwrap();
    assert_eq!(ready(browser.observe()).unwrap(), observation("doc-2"));
    assert_eq!(
        ready(browser.execute(&guard("session-1", "doc-1"), ActionKind::Applicant)),
        Err(BrowserError::StaleDocument)
    );
    assert_eq!(
        ready(browser.execute(&guard("old-session", "doc-2"), ActionKind::Applicant)),
        Err(BrowserError::WrongSession)
    );
    assert_eq!(browser.actions().len(), 1);
}

#[test]
fn handoff_is_permanent_across_navigation_and_errors() {
    let mut browser = ReplayBrowser::new(
        SessionId::new("session-1").unwrap(),
        vec![observation("doc-1"), observation("doc-2")],
    );
    ready(browser.observe()).unwrap();
    ready(browser.handoff(&guard("session-1", "doc-1"))).unwrap();
    ready(browser.observe()).unwrap();
    assert_eq!(
        ready(browser.execute(&guard("session-1", "doc-2"), ActionKind::Contact)),
        Err(BrowserError::ManualOwnership)
    );
    assert_eq!(browser.ownership(), Ownership::Manual);
    assert!(browser.actions().is_empty());
}

#[test]
fn cannot_act_without_observation_or_consume_invented_observations() {
    let mut browser = ReplayBrowser::new(SessionId::new("session-1").unwrap(), vec![]);
    assert_eq!(
        ready(browser.execute(&guard("session-1", "doc-1"), ActionKind::Applicant)),
        Err(BrowserError::NoObservation)
    );
    assert_eq!(ready(browser.observe()), Err(BrowserError::EndOfReplay));
}

#[test]
fn stale_handoff_does_not_claim_the_new_page() {
    let mut browser = ReplayBrowser::new(
        SessionId::new("session-1").unwrap(),
        vec![observation("doc-2")],
    );
    ready(browser.observe()).unwrap();
    assert_eq!(
        ready(browser.handoff(&guard("session-1", "doc-1"))),
        Err(BrowserError::StaleDocument)
    );
    assert_eq!(browser.ownership(), Ownership::Automatic);
}

#[test]
fn same_document_stage_change_rejects_queued_action() {
    let mut next = observation("doc-1");
    next.kind = PageKind::Booking;
    let mut browser = ReplayBrowser::new(
        SessionId::new("session-1").unwrap(),
        vec![observation("doc-1"), next],
    );
    ready(browser.observe()).unwrap();
    let queued = guard("session-1", "doc-1");
    ready(browser.observe()).unwrap();
    assert_eq!(
        ready(browser.execute(&queued, ActionKind::Applicant)),
        Err(BrowserError::StaleDocument)
    );
    assert!(browser.actions().is_empty());
}

#[test]
fn pause_revokes_queued_command_even_after_resume_on_same_page() {
    use ceh_core::control::Authority;
    let mut browser = ReplayBrowser::new(
        SessionId::new("session-1").unwrap(),
        vec![observation("doc-1")],
    );
    ready(browser.observe()).unwrap();
    let old = guard("session-1", "doc-1");
    let mut authority = Authority::new(SessionId::new("session-1").unwrap());
    browser.apply_control(&authority.pause().unwrap()).unwrap();
    assert_eq!(
        ready(browser.execute(&old, ActionKind::Applicant)),
        Err(BrowserError::Paused)
    );
    let resumed = authority.resume().unwrap();
    browser.apply_control(&resumed).unwrap();
    assert_eq!(
        ready(browser.execute(&old, ActionKind::Applicant)),
        Err(BrowserError::StaleCommand)
    );
    let mut fresh = old;
    fresh.epoch = resumed.epoch();
    ready(browser.execute(&fresh, ActionKind::Applicant)).unwrap();
    assert_eq!(browser.actions().len(), 1);
}

#[test]
fn flow_and_browser_keep_lost_ack_request_single_after_pause() {
    use ceh_core::flow::{FlowEffect, FlowEngine};
    let session = SessionId::new("session-1").unwrap();
    let mut o = observation("doc-1");
    o.kind = PageKind::RequestMenu;
    let mut browser = ReplayBrowser::new(session.clone(), vec![o.clone(), o]);
    let mut flow = FlowEngine::new(session, SearchOptions::default());
    let current = ready(browser.observe()).unwrap();
    let effects = flow.observe(current, 0).unwrap();
    let FlowEffect::Activate { guard, action } = &effects[0] else {
        panic!("missing request")
    };
    ready(browser.execute(guard, *action)).unwrap(); // acknowledgement is deliberately lost
    browser.apply_control(&flow.pause(1).unwrap()).unwrap();
    browser.apply_control(&flow.resume(2).unwrap()).unwrap();
    let current = ready(browser.observe()).unwrap();
    assert_eq!(flow.observe(current, 3).unwrap(), vec![FlowEffect::Hold]);
    assert_eq!(
        ready(browser.execute(guard, *action)),
        Err(BrowserError::StaleCommand)
    );
    assert_eq!(browser.actions().len(), 1);
}
#[test]
fn handoff_snapshot_revokes_queued_actions_before_any_details() {
    use ceh_core::{
        control::ControlMode,
        flow::{FlowEffect, FlowEngine},
    };
    let session = SessionId::new("session-1").unwrap();
    let mut o = observation("doc-1");
    o.kind = PageKind::Booking;
    let mut browser = ReplayBrowser::new(session.clone(), vec![o]);
    let mut flow = FlowEngine::new(session, SearchOptions::default());
    let effects = flow.observe(ready(browser.observe()).unwrap(), 0).unwrap();
    assert_eq!(effects, vec![FlowEffect::Handoff, FlowEffect::Alert]);
    assert_eq!(flow.control().mode(), ControlMode::Manual);
    browser.apply_control(&flow.control()).unwrap();
    assert_eq!(
        ready(browser.execute(&guard("session-1", "doc-1"), ActionKind::Applicant)),
        Err(BrowserError::ManualOwnership)
    );
    assert!(browser.actions().is_empty());
}

#[test]
fn duplicate_transport_delivery_cannot_submit_same_document_twice() {
    use ceh_core::control::Authority;
    let session = SessionId::new("session-1").unwrap();
    let mut browser = ReplayBrowser::new(session.clone(), vec![observation("doc-1")]);
    ready(browser.observe()).unwrap();
    let mut command = guard("session-1", "doc-1");
    ready(browser.execute(&command, ActionKind::Applicant)).unwrap();
    assert_eq!(
        ready(browser.execute(&command, ActionKind::Applicant)),
        Err(BrowserError::DuplicateCommand)
    );
    let mut authority = Authority::new(session);
    browser.apply_control(&authority.pause().unwrap()).unwrap();
    let resumed = authority.resume().unwrap();
    browser.apply_control(&resumed).unwrap();
    command.epoch = resumed.epoch();
    assert_eq!(
        ready(browser.execute(&command, ActionKind::Applicant)),
        Err(BrowserError::DuplicateCommand)
    );
    assert_eq!(browser.actions().len(), 1);
}
