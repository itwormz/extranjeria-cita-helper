//! Reading and editing today's budget while no search runs (Python `current_daily_budget` / `edit_daily_budget`).
use ceh_desktop::{appointment_budget_view, set_idle_daily_target};
use ceh_store::appointment_budget::BudgetStore;
use chrono::{DateTime, Utc};
use std::fs;

fn at(text: &str) -> DateTime<Utc> {
    text.parse().unwrap()
}
fn runtime() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("tasks.yml"), "tasks:\n  - id: fixture\n    label: Fixture\n").unwrap();
    root
}
fn profile(root: &std::path::Path) -> ceh_core::profiles::ProfileDraft {
    ceh_desktop::profile_editor(root, Some("fixture")).unwrap().draft
}

#[test]
fn a_profile_without_a_budget_file_shows_the_base_budget_and_creates_nothing() {
    let root = runtime();
    let summary = appointment_budget_view(root.path(), "fixture", at("2026-11-10T11:00:00Z")).unwrap().unwrap();
    assert_eq!((summary.target, summary.sent, summary.remaining), (5, 0, 5));
    assert!(!root.path().join("data").exists(), "viewing writes nothing");
    assert_eq!(appointment_budget_view(root.path(), "../other", at("2026-11-10T11:00:00Z")).err(), Some("profile_not_found"));
}

#[test]
fn editing_sets_todays_target_and_keeps_the_other_fields() {
    let root = runtime();
    let data = root.path().join("data");
    let store = BudgetStore::open(&data, &profile(root.path())).unwrap();
    let mut budget = store.load().unwrap();
    let now = at("2026-11-10T11:00:00Z");
    let policy = ceh_core::appointment_budget::AppointmentPolicy::default();
    budget.submitted(now, &policy);
    budget.banners = 4;
    store.save(&budget).unwrap();
    drop(store);
    let changed = set_idle_daily_target(root.path(), "fixture", Some(12), now).unwrap();
    assert_eq!((changed.target, changed.sent, changed.banners, changed.manual_target), (12, 1, 4, Some(12)));
    let again = appointment_budget_view(root.path(), "fixture", now).unwrap().unwrap();
    assert_eq!((again.target, again.target_source), (12, ceh_core::appointment_budget::TargetSource::Manual));
    let cleared = set_idle_daily_target(root.path(), "fixture", None, now).unwrap();
    assert_eq!((cleared.target, cleared.manual_target, cleared.sent), (5, None, 1));
}

#[test]
fn invalid_targets_are_rejected_without_touching_the_file() {
    let root = runtime();
    let now = at("2026-11-10T11:00:00Z");
    set_idle_daily_target(root.path(), "fixture", Some(7), now).unwrap();
    let before = fs::read_dir(root.path().join("data/appointment-budget")).unwrap().flatten().map(|e| (e.file_name(), fs::read(e.path()).unwrap())).collect::<Vec<_>>();
    for bad in [0, i32::MAX as u32 + 1, u32::MAX] {
        assert_eq!(set_idle_daily_target(root.path(), "fixture", Some(bad), now).err(), Some("invalid_target"));
    }
    let mut after = fs::read_dir(root.path().join("data/appointment-budget")).unwrap().flatten().map(|e| (e.file_name(), fs::read(e.path()).unwrap())).collect::<Vec<_>>();
    let mut before = before;
    before.sort();
    after.sort();
    assert_eq!(before, after);
}

#[test]
fn a_budget_used_by_another_search_cannot_be_edited() {
    let root = runtime();
    let held = BudgetStore::open_saved(&root.path().join("data"), &profile(root.path())).unwrap();
    assert_eq!(set_idle_daily_target(root.path(), "fixture", Some(9), at("2026-11-10T11:00:00Z")).err(), Some("budget_in_use"));
    drop(held);
    assert!(set_idle_daily_target(root.path(), "fixture", Some(9), at("2026-11-10T11:00:00Z")).is_ok());
}

#[test]
fn viewing_on_a_new_day_resets_the_counters_and_saves_the_new_day_for_an_existing_file() {
    let root = runtime();
    let data = root.path().join("data");
    let yesterday = at("2026-11-09T11:00:00Z");
    let store = BudgetStore::open(&data, &profile(root.path())).unwrap();
    let mut budget = store.load().unwrap();
    for _ in 0..3 {
        budget.submitted(yesterday, &ceh_core::appointment_budget::AppointmentPolicy::default());
    }
    store.save(&budget).unwrap();
    drop(store);
    let today = at("2026-11-10T11:00:00Z");
    let summary = appointment_budget_view(root.path(), "fixture", today).unwrap().unwrap();
    assert_eq!(summary.sent, 0);
    let saved = BudgetStore::load_profile(&data, "fixture").unwrap().unwrap();
    assert_eq!(saved.sent(), 0, "the reset day is stored");
}
