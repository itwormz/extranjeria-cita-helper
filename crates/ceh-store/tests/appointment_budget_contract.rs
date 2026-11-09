use ceh_core::{appointment_budget::AppointmentPolicy, profiles::ProfileDraft};
use ceh_store::appointment_budget::BudgetStore;
use chrono::{DateTime, Utc};

#[test]
fn persisted_budget_survives_restart_and_profiles_with_same_document_share_lock() {
    let cases: serde_json::Value = serde_json::from_str(include_str!("../../../tests/contracts/fixtures/profile-validation.json")).unwrap();
    let mut profile: ProfileDraft = serde_json::from_value(cases["cases"][0]["expected"]["normalized"].clone()).unwrap();
    profile.fill_mode = "manual".into();
    profile.document_type = "NIE".into();
    profile.document_number = "X1234567L".into();
    let data = tempfile::tempdir().unwrap();
    let store = BudgetStore::open(data.path(), &profile).unwrap();
    let mut budget = store.load().unwrap();
    let at: DateTime<Utc> = "2026-11-04T11:00:00Z".parse().unwrap();
    budget.submitted(at, &AppointmentPolicy::default());
    store.save(&budget).unwrap();
    profile.id = "another-profile".into();
    assert!(BudgetStore::open(data.path(), &profile).is_err());
    drop(store);
    let reopened = BudgetStore::open(data.path(), &profile).unwrap();
    assert_eq!(reopened.load().unwrap().sent(), 1);
    let directory = data.path().join("appointment-budget");
    for entry in std::fs::read_dir(directory).unwrap().flatten() {
        if entry.path().extension().is_some_and(|extension| extension == "json") {
            assert!(!std::fs::read_to_string(entry.path()).unwrap().contains("X1234567L"));
        }
    }
}

fn stored_profile(budget: &serde_json::Value) -> tempfile::TempDir {
    let data = tempfile::tempdir().unwrap();
    let root = data.path().join("appointment-budget");
    std::fs::create_dir_all(&root).unwrap();
    let key = "b".repeat(64);
    std::fs::write(root.join("profile-demo.json"), format!(r#"{{"key":"{key}"}}"#)).unwrap();
    std::fs::write(root.join(format!("{key}.json")), budget.to_string()).unwrap();
    data
}

#[test]
fn python_0_3_6_budget_file_loads_without_losing_state() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../tests/contracts/fixtures/budget-file-0.3.6.json")).unwrap();
    let data = stored_profile(&fixture["file"]);
    let mut budget = BudgetStore::load_profile(data.path(), "demo").unwrap().unwrap();
    let at: DateTime<Utc> = fixture["at"].as_str().unwrap().parse().unwrap();
    let summary = serde_json::to_value(budget.summary(at, &AppointmentPolicy::default(), false)).unwrap();
    for key in ["sent", "target", "manual_target", "cod_oper_count", "restricted", "next_target", "target_source", "remaining", "wait_reason"] {
        assert_eq!(summary[key], fixture["summary"][key], "{key}");
    }
    assert_eq!(summary["episode_history"].as_array().unwrap().len(), fixture["summary"]["episode_history"].as_array().unwrap().len());
    assert_eq!(summary["episode_history"][0]["offices"][0]["names"], fixture["summary"]["episode_history"][0]["offices"][0]["names"]);
}

#[test]
fn old_target_override_below_five_is_clamped_and_windows_dropped() {
    let data = stored_profile(&serde_json::json!({
        "day": "2026-11-03", "sent": 1, "target_override": 1, "next_target": 2,
        "windows": [["2026-11-03T12:00:00+01:00", "2026-11-03T12:30:00+01:00"]]
    }));
    let budget = BudgetStore::load_profile(data.path(), "demo").unwrap().unwrap();
    let stored = serde_json::to_value(&budget).unwrap();
    assert_eq!(stored["target_override"], 5);
    assert_eq!(stored["next_target"], 5);
    assert_eq!(stored["windows"], serde_json::json!([]));
    assert_eq!(stored["sent"], 1);
}

#[test]
fn saved_file_keeps_exactly_the_python_field_names() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../tests/contracts/fixtures/budget-file-0.3.6.json")).unwrap();
    let data = stored_profile(&fixture["file"]);
    let budget = BudgetStore::load_profile(data.path(), "demo").unwrap().unwrap();
    let mut profile: ProfileDraft = serde_json::from_value(
        serde_json::from_str::<serde_json::Value>(include_str!("../../../tests/contracts/fixtures/profile-validation.json")).unwrap()["cases"][0]["expected"]["normalized"].clone(),
    )
    .unwrap();
    profile.fill_mode = "manual".into();
    profile.document_type = "NIE".into();
    profile.document_number = "X1234567L".into();
    let store = BudgetStore::open(data.path(), &profile).unwrap();
    store.save(&budget).unwrap();
    let saved = std::fs::read_dir(data.path().join("appointment-budget")).unwrap().flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|e| e == "json") && !path.file_name().unwrap().to_string_lossy().starts_with("profile-") && !path.file_name().unwrap().to_string_lossy().starts_with("bbbb"))
        .unwrap();
    let written: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(saved).unwrap()).unwrap();
    let mut python_keys: Vec<_> = fixture["file"].as_object().unwrap().keys().cloned().collect();
    let mut rust_keys: Vec<_> = written.as_object().unwrap().keys().cloned().collect();
    python_keys.sort();
    rust_keys.sort();
    assert_eq!(rust_keys, python_keys);
}

#[test]
fn copied_document_budget_is_locked_against_another_profile_and_never_stores_the_document() {
    let cases: serde_json::Value = serde_json::from_str(include_str!("../../../tests/contracts/fixtures/profile-validation.json")).unwrap();
    let mut profile: ProfileDraft = serde_json::from_value(cases["cases"][0]["expected"]["normalized"].clone()).unwrap();
    profile.fill_mode = "copy".into();
    profile.id = "copy-a".into();
    let data = tempfile::tempdir().unwrap();
    let first = BudgetStore::open_for_document(data.path(), &profile, Some(("X1234567L", "NIE"))).unwrap();
    let mut budget = first.load().unwrap();
    budget.submitted("2026-11-04T11:00:00Z".parse().unwrap(), &AppointmentPolicy::default());
    first.save(&budget).unwrap();
    profile.id = "copy-b".into();
    assert!(
        BudgetStore::open_for_document(data.path(), &profile, Some(("x1234567l", "nie"))).is_err(),
        "the same document (any case) is in use by another profile"
    );
    drop(first);
    let reopened = BudgetStore::open_for_document(data.path(), &profile, Some(("X1234567L", "NIE"))).unwrap();
    assert_eq!(reopened.load().unwrap().sent(), 1);
    for entry in std::fs::read_dir(data.path().join("appointment-budget")).unwrap().flatten() {
        if entry.path().extension().is_some_and(|e| e == "json") {
            assert!(!std::fs::read_to_string(entry.path()).unwrap().to_uppercase().contains("X1234567L"));
        }
    }
}
