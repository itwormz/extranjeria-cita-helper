use ceh_core::{flow::*, model::*, recovery::*, timings::parse_timings};
use serde_json::json;
fn observation(kind: PageKind) -> Observation {
    Observation {
        document: DocumentId::new("same-document").unwrap(),
        kind,
        method: RequestMethod::Get,
        operation_code: false,
        variant: String::new(),
    }
}
fn engine() -> FlowEngine {
    FlowEngine::new(SessionId::new("s").unwrap(), SearchOptions::default())
}
fn facts() -> PageFacts {
    PageFacts {
        recovery: RecoveryFacts {
            official_site: true,
            ..Default::default()
        },
        ..Default::default()
    }
}
#[test]
fn changing_a_pending_recovery_recalculates_from_its_original_start() {
    let mut e = engine();
    let o = observation(PageKind::RateLimited);
    assert_eq!(
        e.observe_with_facts(o.clone(), facts(), 1000).unwrap(),
        vec![FlowEffect::WaitUntil(181000)]
    );
    e.apply_timings(
        parse_timings(&json!({"rate_limit_seconds":240})).unwrap(),
        30.,
    )
    .unwrap();
    assert_eq!(
        e.observe_with_facts(o.clone(), facts(), 10000).unwrap(),
        vec![FlowEffect::WaitUntil(241000)]
    );
    assert!(
        e.observe_with_facts(o.clone(), facts(), 240999)
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        e.observe_with_facts(o, facts(), 241000).unwrap().as_slice(),
        [FlowEffect::Recover {
            action: RecoveryAction::GetCurrent,
            ..
        }]
    ));
}
#[test]
fn proven_unsent_request_can_retry_after_the_configured_pause() {
    let mut e = engine();
    let o = observation(PageKind::RequestMenu);
    let a = e.observe_with_facts(o.clone(), facts(), 0).unwrap();
    let FlowEffect::Activate { guard, action } = a[0].clone() else {
        panic!()
    };
    e.acknowledge_dispatch(&guard, action, DispatchOutcome::NotDispatched)
        .unwrap();
    assert_eq!(
        e.observe_with_facts(o.clone(), facts(), 1).unwrap(),
        vec![FlowEffect::WaitUntil(2000)]
    );
    assert!(matches!(
        e.observe_with_facts(o, facts(), 2000).unwrap().as_slice(),
        [FlowEffect::Activate {
            action: ActionKind::Request,
            ..
        }]
    ));
}
#[test]
fn ambiguous_request_is_not_replayed_and_gets_a_safe_bounded_exit() {
    let mut e = engine();
    let o = observation(PageKind::RequestMenu);
    let a = e.observe_with_facts(o.clone(), facts(), 0).unwrap();
    let FlowEffect::Activate { guard, action } = a[0].clone() else {
        panic!()
    };
    e.acknowledge_dispatch(&guard, action, DispatchOutcome::Unknown)
        .unwrap();
    assert_eq!(
        e.observe_with_facts(o.clone(), facts(), 1).unwrap(),
        vec![FlowEffect::WaitUntil(5000)]
    );
    assert_eq!(
        e.observe_with_facts(o.clone(), facts(), 5000).unwrap(),
        vec![FlowEffect::WaitUntil(10000)]
    );
    assert!(matches!(
        e.observe_with_facts(o, facts(), 10000).unwrap().as_slice(),
        [FlowEffect::Recover {
            action: RecoveryAction::GetEntry,
            ..
        }]
    ));
}
#[test]
fn new_wait_samples_do_not_move_existing_recovery_but_next_document_uses_them() {
    let timings =
        parse_timings(&json!({"rate_limit_seconds":180,"recovery_jitter_seconds":[0,10]})).unwrap();
    let mut e = FlowEngine::with_timings(
        SessionId::new("s").unwrap(),
        SearchOptions::default(),
        timings,
        30.,
    )
    .unwrap();
    e.set_wait_samples(30., 2.).unwrap();
    let mut o = observation(PageKind::RateLimited);
    assert_eq!(
        e.observe_with_facts(o.clone(), facts(), 0).unwrap(),
        vec![FlowEffect::WaitUntil(182000)]
    );
    e.set_wait_samples(60., 9.).unwrap();
    assert!(
        e.observe_with_facts(o.clone(), facts(), 1000)
            .unwrap()
            .is_empty()
    );
    o.document = DocumentId::new("next").unwrap();
    assert!(
        e.observe_with_facts(o.clone(), facts(), 2000)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        e.observe_with_facts(o, facts(), 2001).unwrap(),
        vec![FlowEffect::WaitUntil(191001)]
    );
}
#[test]
fn pause_then_settings_change_keeps_original_recovery_elapsed_time() {
    let mut e = engine();
    let o = observation(PageKind::RateLimited);
    e.observe_with_facts(o.clone(), facts(), 0).unwrap();
    e.pause(1000).unwrap();
    e.resume(11000).unwrap();
    e.apply_timings(
        parse_timings(&json!({"rate_limit_seconds":240})).unwrap(),
        30.,
    )
    .unwrap();
    assert_eq!(
        e.observe_with_facts(o, facts(), 12000).unwrap(),
        vec![FlowEffect::WaitUntil(250000)]
    );
}
#[test]
fn contact_ambiguity_never_discards_the_current_form() {
    let mut e = engine();
    let o = observation(PageKind::Contact);
    let a = e.observe_with_facts(o.clone(), facts(), 0).unwrap();
    let FlowEffect::Prepare { guard, action } = a[0].clone() else {
        panic!()
    };
    e.complete_preparation(&guard, action, PreparationOutcome::Completed)
        .unwrap();
    e.observe_with_facts(o.clone(), facts(), 1).unwrap();
    e.acknowledge_dispatch(&guard, action, DispatchOutcome::Unknown)
        .unwrap();
    assert_eq!(
        e.observe_with_facts(o.clone(), facts(), 5001).unwrap(),
        vec![FlowEffect::Hold, FlowEffect::Alert]
    );
    assert!(e.observe_with_facts(o, facts(), 100000).unwrap().is_empty());
}
#[test]
fn post_dispatch_wait_starts_after_the_browser_ack_not_before_input() {
    let mut e = engine();
    let o = observation(PageKind::RequestMenu);
    let a = e.observe_with_facts(o.clone(), facts(), 0).unwrap();
    let FlowEffect::Activate { guard, action } = a[0].clone() else {
        panic!()
    };
    e.acknowledge_dispatch_at(&guard, action, DispatchOutcome::Dispatched, 10000)
        .unwrap();
    assert_eq!(
        e.observe_with_facts(o, facts(), 10001).unwrap(),
        vec![FlowEffect::WaitUntil(15000)]
    );
}
#[test]
fn unsent_retries_are_bounded_and_booking_cancels_the_timeout_exit() {
    let mut e = engine();
    let o = observation(PageKind::RequestMenu);
    for at in [0, 2000, 7000] {
        let a = e.observe_with_facts(o.clone(), facts(), at).unwrap();
        let FlowEffect::Activate { guard, action } = a[0].clone() else {
            panic!("expected bounded retry")
        };
        e.acknowledge_dispatch_at(&guard, action, DispatchOutcome::NotDispatched, at)
            .unwrap();
        let deadline = if at == 0 { 2000 } else { at + 5000 };
        assert_eq!(
            e.observe_with_facts(o.clone(), facts(), at + 1).unwrap(),
            vec![FlowEffect::WaitUntil(deadline)]
        );
    }
    assert_eq!(
        e.observe_with_facts(o.clone(), facts(), 12000).unwrap(),
        vec![FlowEffect::WaitUntil(17000)]
    );
    assert!(matches!(
        e.observe_with_facts(observation(PageKind::Booking), facts(), 16000)
            .unwrap()
            .as_slice(),
        [FlowEffect::Handoff, FlowEffect::Alert]
    ));
    assert!(e.observe_with_facts(o, facts(), 17000).unwrap().is_empty());
}
#[test]
fn a_late_old_document_receipt_does_not_retime_the_new_document() {
    let mut e = engine();
    let first = observation(PageKind::RequestMenu);
    let a = e.observe_with_facts(first, facts(), 0).unwrap();
    let FlowEffect::Activate { guard, action } = a[0].clone() else {
        panic!()
    };
    let mut next = observation(PageKind::RequestMenu);
    next.document = DocumentId::new("new-document").unwrap();
    e.observe_with_facts(next.clone(), facts(), 1).unwrap();
    e.acknowledge_dispatch_at(&guard, action, DispatchOutcome::NotDispatched, 2)
        .unwrap();
    assert_eq!(
        e.observe_with_facts(next, facts(), 3).unwrap(),
        vec![FlowEffect::Hold]
    );
}
#[test]
fn slow_failed_or_unknown_preparation_gets_a_full_wait_after_its_completion() {
    for outcome in [PreparationOutcome::Failed, PreparationOutcome::Unknown] {
        let mut e = engine();
        let o = observation(PageKind::Contact);
        let effects = e.observe_with_facts(o.clone(), facts(), 0).unwrap();
        let FlowEffect::Prepare { guard, action } = effects[0].clone() else {
            panic!()
        };
        e.complete_preparation_at(&guard, action, outcome, 45000)
            .unwrap();
        assert_eq!(
            e.observe_with_facts(o.clone(), facts(), 45001).unwrap(),
            vec![FlowEffect::WaitUntil(50000)]
        );
        assert!(
            e.observe_with_facts(o.clone(), facts(), 49999)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            e.observe_with_facts(o, facts(), 50000).unwrap(),
            vec![FlowEffect::Hold, FlowEffect::Alert]
        );
    }
}
