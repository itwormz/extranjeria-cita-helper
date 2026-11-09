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
fn apply_policy_changing_cod_pause_recalculates_from_original_start() {
    let mut policy = AppointmentPolicy::default();
    let mut budget = AppointmentBudget::default();
    let start = utc("2026-11-03T11:00:00Z");
    budget.submitted(start, &policy);
    budget.cod_oper(start + Duration::minutes(1), &policy);
    assert_eq!(budget.restriction_until(), Some(start + Duration::minutes(31)));
    let next = AppointmentPolicy { first_cod_pause_minutes: 45, ..policy.clone() };
    apply_budget_command(&mut budget, &mut policy, BudgetCommand::ApplyPolicy(next), start + Duration::minutes(2)).unwrap();
    assert_eq!(budget.restriction_until(), Some(start + Duration::minutes(46)));
    assert_eq!(policy.first_cod_pause_minutes, 45);
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
