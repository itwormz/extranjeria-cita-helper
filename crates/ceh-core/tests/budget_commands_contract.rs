use ceh_core::appointment_budget::{AppointmentBudget, AppointmentPolicy, BudgetError, TargetSource};
use ceh_core::budget_commands::{BudgetCommand, apply_budget_command};
use chrono::{DateTime, Duration, Utc};

fn utc(text: &str) -> DateTime<Utc> {
    text.parse().unwrap()
}

#[test]
fn apply_policy_switching_adaptive_off_clears_manual_target() {
    let mut policy = AppointmentPolicy::default();
    let mut budget = AppointmentBudget::default();
    let now = utc("2026-11-03T11:00:00Z");
    apply_budget_command(&mut budget, &mut policy, BudgetCommand::SetDailyTarget(Some(20)), now).unwrap();
    assert_eq!(budget.summary(now, &policy, false).target_source, TargetSource::Manual);
    let next = AppointmentPolicy { adaptive_target: false, daily_target: 8, ..policy.clone() };
    apply_budget_command(&mut budget, &mut policy, BudgetCommand::ApplyPolicy(next), now).unwrap();
    let summary = budget.summary(now, &policy, false);
    assert_eq!((summary.manual_target, summary.target, summary.target_source), (None, 8, TargetSource::Base));
}

#[test]
fn apply_policy_keeping_adaptive_preserves_manual_target() {
    let mut policy = AppointmentPolicy::default();
    let mut budget = AppointmentBudget::default();
    let now = utc("2026-11-03T11:00:00Z");
    apply_budget_command(&mut budget, &mut policy, BudgetCommand::SetDailyTarget(Some(20)), now).unwrap();
    let next = AppointmentPolicy { daily_target: 9, ..policy.clone() };
    apply_budget_command(&mut budget, &mut policy, BudgetCommand::ApplyPolicy(next), now).unwrap();
    assert_eq!(budget.summary(now, &policy, false).target, 20);
}

#[test]
fn a_cod_oper_answer_is_recorded_for_statistics_but_adds_no_pause() {
    // Owner decision (2026-11-11): the extra pauses after `cod. oper` are gone. The answer still counts and
    // still marks a possible restriction; only the ordinary minimum pause between attempts applies.
    let policy = AppointmentPolicy::default();
    let mut budget = AppointmentBudget::default();
    let start = utc("2026-11-03T11:00:00Z");
    budget.submitted(start, &policy);
    budget.cod_oper(start + Duration::minutes(1), &policy);
    assert!(budget.restricted(), "the possible restriction stays visible");
    assert_eq!(budget.restriction_until(), None, "no pause is scheduled");
    let summary = budget.summary(start + Duration::minutes(2), &policy, false);
    assert_eq!(summary.cod_oper_count, 1);
    assert!(summary.restricted);
    assert_ne!(summary.wait_reason, Some(ceh_core::appointment_budget::WaitReason::CodOper));
    assert_eq!(budget.next_submission(start + Duration::minutes(2), &policy, false), start + Duration::minutes(15));
    // A further attempt does not start a longer pause either.
    budget.submitted(start + Duration::minutes(15), &policy);
    assert_eq!(budget.restriction_until(), None);
    assert_eq!(budget.next_submission(start + Duration::minutes(16), &policy, false), start + Duration::minutes(30));
}

#[test]
fn apply_policy_changing_distribution_resets_todays_choice() {
    let mut policy = AppointmentPolicy::default();
    let mut budget = AppointmentBudget::default();
    let now = utc("2026-11-03T11:00:00Z");
    apply_budget_command(&mut budget, &mut policy, BudgetCommand::SetDistribution { disabled: true }, now).unwrap();
    assert!(!budget.distributes(&policy));
    let next = AppointmentPolicy { distribute_attempts: true, ..policy.clone() };
    apply_budget_command(&mut budget, &mut policy, BudgetCommand::ApplyPolicy(next), now).unwrap();
    assert!(budget.distributes(&policy), "profile choice replaces the temporary disable");
}

#[test]
fn set_daily_target_invalid_is_rejected_without_change() {
    let mut policy = AppointmentPolicy::default();
    let mut budget = AppointmentBudget::default();
    let now = utc("2026-11-03T11:00:00Z");
    apply_budget_command(&mut budget, &mut policy, BudgetCommand::SetDailyTarget(Some(20)), now).unwrap();
    for bad in [0, i32::MAX as u32 + 1] {
        assert_eq!(
            apply_budget_command(&mut budget, &mut policy, BudgetCommand::SetDailyTarget(Some(bad)), now),
            Err(BudgetError::InvalidTarget)
        );
    }
    assert_eq!(budget.summary(now, &policy, false).manual_target, Some(20));
}

#[test]
fn try_now_command_allows_an_immediate_attempt_keeping_minimum_pause() {
    let mut policy = AppointmentPolicy::default();
    let mut budget = AppointmentBudget::default();
    let start = utc("2026-11-03T11:00:00Z");
    budget.submitted(start, &policy);
    budget.cod_oper(start + Duration::minutes(1), &policy);
    apply_budget_command(&mut budget, &mut policy, BudgetCommand::TryNow, start + Duration::minutes(2)).unwrap();
    assert_eq!(budget.next_submission(start + Duration::minutes(2), &policy, false), start + Duration::minutes(15));
}

#[test]
fn binding_a_copied_document_keeps_todays_manual_target_and_distribution_choice() {
    let policy = AppointmentPolicy::default();
    let now = utc("2026-11-03T11:00:00Z");
    let mut before = AppointmentBudget::default();
    before.set_daily_target(Some(12), now, &policy).unwrap();
    before.set_distribution(false, now, &policy);
    // The document's own saved budget has real usage but none of today's explicit choices.
    let mut bound = AppointmentBudget::default();
    bound.submitted(now - Duration::hours(1), &policy);
    bound.carry_today_choices_from(&before, now, &policy);
    let summary = bound.summary(now, &policy, false);
    assert_eq!((summary.sent, summary.manual_target, summary.disabled), (1, Some(12), false));
}

#[test]
fn choices_from_another_day_are_not_carried_to_a_bound_document() {
    let policy = AppointmentPolicy::default();
    let yesterday = utc("2026-11-02T11:00:00Z");
    let now = utc("2026-11-03T11:00:00Z");
    let mut before = AppointmentBudget::default();
    before.set_daily_target(Some(12), yesterday, &policy).unwrap();
    let mut bound = AppointmentBudget::default();
    bound.carry_today_choices_from(&before, now, &policy);
    assert_eq!(bound.summary(now, &policy, false).manual_target, None);
}
