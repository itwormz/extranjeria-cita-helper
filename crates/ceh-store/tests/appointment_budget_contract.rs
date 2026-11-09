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
