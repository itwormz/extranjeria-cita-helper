//! User and settings commands that change a running search's appointment budget.
use crate::appointment_budget::{AppointmentBudget, AppointmentPolicy, BudgetError};
use chrono::{DateTime, Utc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BudgetCommand {
    /// Today's manual budget; `None` restores the adaptive/base budget.
    SetDailyTarget(Option<u32>),
    /// Today-only choice for window distribution.
    SetDistribution { disabled: bool },
    /// Bring the next attempt forward once (keeps the minimum pause).
    TryNow,
    /// The profile's policy changed while the search runs.
    ApplyPolicy(AppointmentPolicy),
}

/// Apply one command; on `Err` neither the budget nor the policy changed.
pub fn apply_budget_command(
    budget: &mut AppointmentBudget,
    policy: &mut AppointmentPolicy,
    command: BudgetCommand,
    now: DateTime<Utc>,
) -> Result<(), BudgetError> {
    match command {
        BudgetCommand::SetDailyTarget(target) => budget.set_daily_target(target, now, policy),
        BudgetCommand::SetDistribution { disabled } => {
            budget.set_distribution(disabled, now, policy);
            Ok(())
        }
        BudgetCommand::TryNow => {
            budget.try_now(now, policy);
            Ok(())
        }
        BudgetCommand::ApplyPolicy(new) => {
            budget.policy_changed(policy, &new);
            *policy = new;
            Ok(())
        }
    }
}
