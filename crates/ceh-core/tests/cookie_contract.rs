use ceh_core::{flow::*, model::*};
fn engine() -> FlowEngine {
    FlowEngine::new(
        SessionId::new("cookie-test").unwrap(),
        SearchOptions::default(),
    )
}
fn page(kind: PageKind) -> Observation {
    Observation {
        document: DocumentId::new("cookie-document").unwrap(),
        kind,
        method: RequestMethod::Get,
        operation_code: false,
        variant: String::new(),
    }
}
fn banner() -> PageFacts {
    PageFacts {
        cookie_accept_available: true,
        ..Default::default()
    }
}
#[test]
fn cookie_precedes_form_actions_and_is_not_repeated_while_still_visible() {
    let mut e = engine();
    let effects = e
        .observe_with_facts(page(PageKind::Province), banner(), 0)
        .unwrap();
    let [FlowEffect::AcceptCookies { guard }] = effects.as_slice() else {
        panic!("cookie must precede preparation")
    };
    let guard = guard.clone();
    e.acknowledge_dispatch(&guard, ActionKind::Cookie, DispatchOutcome::Unknown)
        .unwrap();
    assert_eq!(
        e.observe_with_facts(page(PageKind::Province), banner(), 1)
            .unwrap(),
        vec![FlowEffect::ReobserveAt(501)]
    );
    assert!(matches!(
        e.observe(page(PageKind::Province), 2).unwrap().as_slice(),
        [FlowEffect::Prepare {
            action: ActionKind::Province,
            ..
        }]
    ));
}
#[test]
fn cookie_does_not_mutate_paused_or_manual_pages() {
    let mut e = engine();
    e.pause(0).unwrap();
    assert!(
        e.observe_with_facts(page(PageKind::Province), banner(), 1)
            .unwrap()
            .is_empty()
    );
    let mut e = engine();
    assert_eq!(
        e.observe_with_facts(page(PageKind::Booking), banner(), 0)
            .unwrap(),
        vec![FlowEffect::Handoff, FlowEffect::Alert]
    );
    assert!(
        e.observe_with_facts(page(PageKind::Province), banner(), 1)
            .unwrap()
            .is_empty()
    );
    let mut e = engine();
    let mut facts = banner();
    facts.office_count = 2;
    assert_eq!(
        e.observe_with_facts(page(PageKind::Office), facts, 0)
            .unwrap(),
        vec![FlowEffect::Alert]
    );
}
#[test]
fn cookie_is_not_an_authentication_or_error_recovery_action() {
    for kind in [
        PageKind::AuthWait,
        PageKind::Unknown,
        PageKind::Loading,
        PageKind::Expired,
        PageKind::Blocked,
        PageKind::RateLimited,
        PageKind::ServerError,
    ] {
        assert!(
            !engine()
                .observe_with_facts(page(kind), banner(), 0)
                .unwrap()
                .iter()
                .any(|e| matches!(e, FlowEffect::AcceptCookies { .. }))
        );
    }
}

#[test]
fn proven_unsent_pre_action_can_resume_but_unknown_delivery_cannot() {
    for unknown in [false, true] {
        let mut e = engine();
        let effects = e
            .observe_with_facts(page(PageKind::Province), banner(), 0)
            .unwrap();
        let [FlowEffect::AcceptCookies { guard }] = effects.as_slice() else {
            panic!("missing cookie")
        };
        let guard = guard.clone();
        if unknown {
            e.acknowledge_dispatch(&guard, ActionKind::Cookie, DispatchOutcome::Unknown)
                .unwrap();
        }
        e.pause(1).unwrap();
        let result = e.cancel_unpublished_pre_action(&guard, ActionKind::Cookie);
        if unknown {
            assert_eq!(result, Err(FlowError::UnexpectedCompletion));
        } else {
            result.unwrap();
            e.cancel_unpublished_pre_action(&guard, ActionKind::Cookie)
                .unwrap();
        }
        assert!(
            e.observe_with_facts(page(PageKind::Province), banner(), 2)
                .unwrap()
                .is_empty()
        );
        e.resume(3).unwrap();
        let effects = e
            .observe_with_facts(page(PageKind::Province), banner(), 4)
            .unwrap();
        assert_eq!(
            effects
                .iter()
                .any(|e| matches!(e, FlowEffect::AcceptCookies { .. })),
            !unknown
        );
        if !unknown {
            // A duplicate old cancellation cannot clear a new pending intent.
            e.cancel_unpublished_pre_action(&guard, ActionKind::Cookie)
                .unwrap();
            assert!(
                !e.observe_with_facts(page(PageKind::Province), banner(), 5)
                    .unwrap()
                    .iter()
                    .any(|e| matches!(e, FlowEffect::AcceptCookies { .. }))
            );
        }
    }
}
