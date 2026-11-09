//! Python 0.3.6 appointment-budget parity. Scenarios: tests/contracts/budget_reference.py.
#[path = "support/budget_replay.rs"]
mod budget_replay;

fn run_group(group: &str) {
    let scenarios = budget_replay::scenarios_in_group(group);
    assert!(!scenarios.is_empty(), "group {group} has no scenarios");
    let failures: Vec<String> = scenarios.iter().filter_map(|s| budget_replay::run_scenario(s).err()).collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn group_base() {
    run_group("base");
}
#[test]
fn group_floor_and_manual() {
    run_group("floor_and_manual");
}
#[test]
fn group_distribution() {
    run_group("distribution");
}
#[test]
fn group_try_now() {
    run_group("try_now");
}
#[test]
fn group_recalc() {
    run_group("recalc");
}
#[test]
fn group_large_target() {
    run_group("large_target");
}
