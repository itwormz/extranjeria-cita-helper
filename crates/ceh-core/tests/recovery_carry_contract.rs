use ceh_core::{flow::*, model::*, recovery::RecoveryAction};
fn observation(kind: PageKind) -> Observation {
    Observation { document: DocumentId::new("doc").unwrap(), kind, method: RequestMethod::Get, operation_code: false, variant: String::new() }
}
fn engine() -> FlowEngine {
    FlowEngine::new(SessionId::new("session").unwrap(), SearchOptions::default())
}
/// One block on a fresh engine: returns the wait the core schedules, then runs the cooldown out.
fn blocked_wait(engine: &mut FlowEngine) -> u64 {
    let FlowEffect::WaitUntil(deadline) = engine.observe(observation(PageKind::Blocked), 0).unwrap()[0].clone() else { panic!("no wait") };
    let effects = engine.observe(observation(PageKind::Blocked), deadline).unwrap();
    assert!(matches!(effects[0], FlowEffect::Recover { action: RecoveryAction::Restart { clear_profile: true, reopen: false }, .. }));
    deadline
}

#[test]
fn repeated_blocks_across_new_sessions_wait_30_60_120_then_300_600() {
    let mut carry = RecoveryCarry::default();
    let mut waits = Vec::new();
    for _ in 0..6 {
        let mut next = engine();
        next.apply_carry(carry.clone());
        waits.push(blocked_wait(&mut next));
        carry = next.recovery_carry();
    }
    assert_eq!(waits, vec![30_000, 60_000, 120_000, 300_000, 600_000, 600_000]);
}

#[test]
fn without_the_carry_every_new_session_would_wait_the_first_pause() {
    let waits: Vec<u64> = (0..2).map(|_| blocked_wait(&mut engine())).collect();
    assert_eq!(waits, vec![30_000, 30_000]);
}

#[test]
fn a_progress_page_resets_the_blocked_streak_and_the_429_penalty_is_carried() {
    let mut first = engine();
    blocked_wait(&mut first);
    assert_eq!(first.recovery_carry().blocked_streak, 1);
    first.observe(observation(PageKind::Applicant), 31_000).unwrap();
    assert_eq!(first.recovery_carry().blocked_streak, 0);
    let carry = RecoveryCarry { blocked_streak: 2, applicant_reload_penalty: 60.0 };
    let mut second = engine();
    second.apply_carry(carry.clone());
    assert_eq!(second.recovery_carry(), carry);
    assert_eq!(second.applicant_reload_penalty(), 60.0);
}
