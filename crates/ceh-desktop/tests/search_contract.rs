#![cfg(feature = "shell")]
use ceh_desktop::search::{Control, SearchController};
#[test]
fn idle_controller_is_read_only_and_rejects_controls_without_owned_browser() {
    let controller = SearchController::default();
    let state = controller.snapshot().unwrap();
    assert!(!state.active);
    assert!(state.browser_pid.is_none());
    assert!(state.rows.is_empty());
    for command in [Control::Pause, Control::Resume, Control::Stop] {
        assert!(controller.control(command).is_err());
    }
    assert!(!controller.snapshot().unwrap().active);
}

#[test]
fn idle_controller_rejects_budget_commands_and_settings_refresh() {
    use ceh_core::{appointment_budget::AppointmentPolicy, budget_commands::BudgetCommand};
    let controller = SearchController::default();
    for command in [
        BudgetCommand::TryNow,
        BudgetCommand::SetDailyTarget(Some(7)),
        BudgetCommand::SetDistribution { disabled: true },
        BudgetCommand::ApplyPolicy(AppointmentPolicy::default()),
    ] {
        assert_eq!(controller.budget_command(command), Err("search_not_running"));
    }
    assert_eq!(controller.refresh_settings(std::path::Path::new("/nonexistent")), Err("search_not_running"));
    assert!(!controller.snapshot().unwrap().active);
}
