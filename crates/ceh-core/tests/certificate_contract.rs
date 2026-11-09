use ceh_core::{flow::*, model::*};
fn engine() -> FlowEngine {
    FlowEngine::new(
        SessionId::new("certificate-test").unwrap(),
        SearchOptions::default(),
    )
}
fn page(kind: PageKind) -> Observation {
    Observation {
        document: DocumentId::new(if kind == PageKind::AuthWait {
            "provider"
        } else {
            "applicant"
        })
        .unwrap(),
        kind,
        method: RequestMethod::Get,
        operation_code: false,
        variant: String::new(),
    }
}
fn facts() -> PageFacts {
    PageFacts {
        certificate_method_available: true,
        ..Default::default()
    }
}
fn select(e: &mut FlowEngine) -> DocumentGuard {
    let effects = e
        .observe_with_facts(page(PageKind::AuthWait), facts(), 0)
        .unwrap();
    let Some(FlowEffect::SelectCertificate { guard }) = effects.first() else {
        panic!("certificate selection not requested")
    };
    guard.clone()
}
#[test]
fn certificate_alarm_starts_only_after_actual_publication_and_rings_once() {
    let mut e = engine();
    let guard = select(&mut e);
    assert_eq!(
        e.observe_with_facts(page(PageKind::AuthWait), facts(), 10000)
            .unwrap(),
        vec![FlowEffect::ReobserveAt(10500)]
    );
    e.certificate_published(&guard, 10000).unwrap();
    e.certificate_published(&guard, 12000).unwrap();
    assert_eq!(
        e.observe_with_facts(page(PageKind::AuthWait), facts(), 14999)
            .unwrap(),
        vec![FlowEffect::ReobserveAt(15499)]
    );
    assert_eq!(
        e.observe_with_facts(page(PageKind::AuthWait), facts(), 15000)
            .unwrap(),
        vec![FlowEffect::Alert, FlowEffect::ReobserveAt(15500)]
    );
    assert_eq!(
        e.observe_with_facts(page(PageKind::AuthWait), facts(), 15001)
            .unwrap(),
        vec![FlowEffect::ReobserveAt(15501)]
    );
    assert!(matches!(
        e.observe(page(PageKind::Applicant), 16000)
            .unwrap()
            .as_slice(),
        [FlowEffect::Silence, FlowEffect::Prepare { .. }]
    ));
    assert_eq!(
        e.certificate_published(&guard, 16001),
        Err(FlowError::StaleCompletion)
    );
}
#[test]
fn silent_sso_and_method_choice_do_not_ring() {
    let mut e = engine();
    for now in [0, 6000, 10000] {
        assert_eq!(
            e.observe(page(PageKind::AuthWait), now).unwrap(),
            vec![FlowEffect::ReobserveAt(now + 500)]
        );
    }
    assert!(matches!(
        e.observe(page(PageKind::Applicant), 10001)
            .unwrap()
            .as_slice(),
        [FlowEffect::Prepare { .. }]
    ));
}
#[test]
fn pause_preserves_remaining_certificate_wait() {
    let mut e = engine();
    let guard = select(&mut e);
    e.certificate_published(&guard, 0).unwrap();
    e.pause(2000).unwrap();
    assert!(
        e.observe_with_facts(page(PageKind::AuthWait), facts(), 10000)
            .unwrap()
            .is_empty()
    );
    e.resume(12000).unwrap();
    assert_eq!(
        e.observe_with_facts(page(PageKind::AuthWait), facts(), 14999)
            .unwrap(),
        vec![FlowEffect::ReobserveAt(15499)]
    );
    assert!(matches!(
        e.observe_with_facts(page(PageKind::AuthWait), facts(), 15000)
            .unwrap()
            .as_slice(),
        [FlowEffect::Alert, FlowEffect::ReobserveAt(_)]
    ));
}
#[test]
fn already_published_selection_can_be_recorded_after_pause_without_alerting() {
    let mut e = engine();
    let guard = select(&mut e);
    e.pause(1000).unwrap();
    e.certificate_published(&guard, 1500).unwrap();
    assert!(
        e.observe_with_facts(page(PageKind::AuthWait), facts(), 3000)
            .unwrap()
            .is_empty()
    );
    e.resume(11000).unwrap();
    assert_eq!(
        e.observe_with_facts(page(PageKind::AuthWait), facts(), 15999)
            .unwrap(),
        vec![FlowEffect::ReobserveAt(16499)]
    );
    assert!(matches!(
        e.observe_with_facts(page(PageKind::AuthWait), facts(), 16000)
            .unwrap()
            .as_slice(),
        [FlowEffect::Alert, FlowEffect::ReobserveAt(_)]
    ));
}
#[test]
fn proven_non_delivery_cancels_certificate_reminder() {
    let mut e = engine();
    let guard = select(&mut e);
    e.certificate_published(&guard, 0).unwrap();
    e.acknowledge_dispatch(&guard, ActionKind::Auth, DispatchOutcome::NotDispatched)
        .unwrap();
    assert_eq!(
        e.observe_with_facts(page(PageKind::AuthWait), facts(), 10000)
            .unwrap(),
        vec![FlowEffect::ReobserveAt(10500)]
    );
}
#[test]
fn clave_selection_cannot_start_certificate_timer() {
    let mut e = engine();
    let f = PageFacts {
        authentication: AuthenticationMode::Clave,
        clave_method_available: true,
        ..Default::default()
    };
    let effects = e
        .observe_with_facts(page(PageKind::AuthWait), f, 0)
        .unwrap();
    let FlowEffect::SelectClaveMethod { guard, .. } = &effects[0] else {
        panic!("missing clave effect")
    };
    assert_eq!(
        e.certificate_published(guard, 0),
        Err(FlowError::StaleCompletion)
    );
}
#[test]
fn certificate_deadline_overflow_does_not_mutate_clock_or_control() {
    let mut e = engine();
    let guard = select(&mut e);
    assert_eq!(
        e.certificate_published(&guard, u64::MAX - 1000),
        Err(FlowError::ClockOverflow)
    );
    assert_eq!(
        e.observe_with_facts(page(PageKind::AuthWait), facts(), 1)
            .unwrap(),
        vec![FlowEffect::ReobserveAt(501)]
    );
    e.certificate_published(&guard, u64::MAX - 6000).unwrap();
    e.pause(u64::MAX - 5000).unwrap();
    assert_eq!(e.resume(u64::MAX - 3000), Err(FlowError::ClockOverflow));
    assert_eq!(e.control().mode(), ceh_core::control::ControlMode::Paused);
    e.resume(u64::MAX - 4999).unwrap();
}

#[test]
fn certificate_cancellation_requires_proven_unpublished_input_and_revoked_epoch() {
    let mut e = engine();
    let guard = select(&mut e);
    assert_eq!(
        e.cancel_unpublished_pre_action(&guard, ActionKind::Auth),
        Err(FlowError::StaleCompletion)
    );
    e.pause(1).unwrap();
    e.cancel_unpublished_pre_action(&guard, ActionKind::Auth)
        .unwrap();
    e.resume(2).unwrap();
    assert!(matches!(
        e.observe_with_facts(page(PageKind::AuthWait), facts(), 3)
            .unwrap()
            .as_slice(),
        [FlowEffect::SelectCertificate { .. }]
    ));
    let mut e = engine();
    let guard = select(&mut e);
    e.certificate_published(&guard, 1).unwrap();
    e.pause(2).unwrap();
    assert_eq!(
        e.cancel_unpublished_pre_action(&guard, ActionKind::Auth),
        Err(FlowError::UnexpectedCompletion)
    );
}

#[test]
fn certificate_wait_alerts_once_when_browser_reports_loading() {
    let mut e = engine();
    let _ = select(&mut e);
    assert!(!e.observe(page(PageKind::Loading), 1).unwrap().contains(&FlowEffect::Alert));
    let mut e = engine();
    let guard = select(&mut e);
    e.certificate_published(&guard, 0).unwrap();
    assert_eq!(e.observe(page(PageKind::Loading), 4999).unwrap(), vec![FlowEffect::ReobserveAt(5499)]);
    assert_eq!(e.observe(page(PageKind::Loading), 5000).unwrap(), vec![FlowEffect::Alert, FlowEffect::ReobserveAt(5500)]);
    assert_eq!(e.observe(page(PageKind::Loading), 5500).unwrap(), vec![FlowEffect::ReobserveAt(6000)]);
    assert!(e.observe(page(PageKind::Applicant), 6000).unwrap().contains(&FlowEffect::Silence));
}
