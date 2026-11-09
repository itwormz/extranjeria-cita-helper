use ceh_desktop::appointment::{AppointmentDraft, AppointmentError};
fn draft() -> AppointmentDraft {
    serde_json::from_value(serde_json::json!({"date":"2028-02-29","time":"09:05","office":" Fixture office ","address":" Example address ","confirmation":" PRIVATE-FIXTURE-123 "})).unwrap()
}
#[test]
fn validated_appointment_has_only_local_fields_and_server_timezone() {
    let data = draft().validated("Europe/Madrid").unwrap();
    assert_eq!(
        data,
        serde_json::json!({"date":"2028-02-29","time":"09:05","office":"Fixture office","address":"Example address","confirmation":"PRIVATE-FIXTURE-123","timezone":"Europe/Madrid"})
    );
    assert!(
        serde_json::from_value::<AppointmentDraft>(
            serde_json::json!({"date":"2028-02-29","time":"09:05","timezone":"UTC"})
        )
        .is_err()
    );
}
#[test]
fn invalid_calendar_dates_and_clock_values_are_rejected_without_echoing_input() {
    for value in [
        "2026-02-29",
        "2026-04-31",
        "2026-1-01",
        "0000-01-01",
        "SECRET",
    ] {
        let mut data = draft();
        data.date = value.into();
        assert_eq!(
            data.validated("Europe/Madrid").unwrap_err(),
            AppointmentError::Date
        );
    }
    for value in ["24:00", "09:60", "9:05", "0905", "SECRET"] {
        let mut data = draft();
        data.time = value.into();
        assert_eq!(
            data.validated("Europe/Madrid").unwrap_err(),
            AppointmentError::Time
        );
    }
}
#[test]
fn optional_fields_are_empty_and_controls_or_oversized_values_never_reach_storage() {
    let blank: AppointmentDraft =
        serde_json::from_value(serde_json::json!({"date":"2026-10-31","time":"00:00"})).unwrap();
    assert_eq!(
        blank.validated("Atlantic/Canary").unwrap()["confirmation"],
        ""
    );
    let mut data = draft();
    data.confirmation = "a".repeat(513);
    assert_eq!(
        data.validated("Europe/Madrid").unwrap_err(),
        AppointmentError::Field
    );
    data.confirmation = "line\nsecond".into();
    assert_eq!(
        data.validated("Europe/Madrid").unwrap_err(),
        AppointmentError::Field
    );
    assert_eq!(
        draft().validated("invalid-zone").unwrap_err(),
        AppointmentError::Timezone
    );
}

#[test]
fn record_uses_trusted_profile_and_immutable_statistics_snapshot() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("tasks.yml"), "tasks:\n  - id: fixture\n    label: Fixture\n    schedule:\n      timezone: Atlantic/Canary\n").unwrap();
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-31T15:00:00Z")
        .unwrap()
        .to_utc();
    let result =
        ceh_desktop::appointment::record_at(root.path(), "fixture", &draft(), now).unwrap();
    assert_eq!(result["appointment"]["timezone"], "Atlantic/Canary");
    assert_eq!(result["appointment"]["confirmation"], "PRIVATE-FIXTURE-123");
    assert!(result["id"].as_str().is_some());
    assert!(result["statistics"].is_object());
    assert!(
        result["statistics_provenance"]["unknown_fields"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("checks"))
    );
    let public = ceh_services::exports::public_card(&result, "en");
    assert_eq!(public["rows"][0][1], "—");
    assert_eq!(public["rows"][1][1], "—");
    assert_eq!(public["rows"][10][1], "—");

    assert!(!root.path().join("data/profile-statistics.sqlite3").exists());
    let path = root.path().join("data/achievements.json");
    let before = std::fs::read(&path).unwrap();
    assert_eq!(
        ceh_desktop::appointment::record_at(root.path(), "../other", &draft(), now).unwrap_err(),
        "profile_not_found"
    );
    let mut invalid = draft();
    invalid.time = "24:01".into();
    assert_eq!(
        ceh_desktop::appointment::record_at(root.path(), "fixture", &invalid, now).unwrap_err(),
        "appointment_time_invalid"
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let read = ceh_services::exports::AchievementStore::new(&path)
        .list("fixture")
        .unwrap();
    let public_reopened = ceh_services::exports::public_card(&read[0], "en");
    assert_eq!(public_reopened["rows"][0][1], "—");
    assert_eq!(public_reopened["rows"][1][1], "—");
    assert_eq!(read, vec![result]);
}

fn fixture_root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("tasks.yml"),
        "tasks:\n  - id: fixture\n    label: Fixture\n",
    )
    .unwrap();
    root
}
#[test]
fn context_is_read_only_and_edit_preserves_original_statistics_and_identity() {
    let root = fixture_root();
    let at = chrono::DateTime::parse_from_rfc3339("2026-10-31T15:00:00Z")
        .unwrap()
        .to_utc();
    let empty = ceh_desktop::appointment::context_at(root.path(), "fixture", at).unwrap();
    assert!(empty.records.is_empty());
    assert!(empty.offices.is_empty());
    assert_eq!(empty.today, "2026-10-31");
    assert!(!root.path().join("data").exists());
    let record = ceh_desktop::appointment::record_at(root.path(), "fixture", &draft(), at).unwrap();
    let id = record["id"].as_str().unwrap();
    let mut edited = draft();
    edited.time = "10:15".into();
    let updated = ceh_desktop::appointment::update(root.path(), "fixture", id, &edited).unwrap();
    assert_eq!(updated["statistics"], record["statistics"]);
    assert_eq!(updated["recorded_at"], record["recorded_at"]);
    assert_eq!(updated["id"], record["id"]);
    assert_eq!(updated["appointment"]["time"], "10:15");
    let reopened = ceh_desktop::appointment::context_at(root.path(), "fixture", at).unwrap();
    assert_eq!(reopened.records, vec![updated.clone()]);
    assert_eq!(
        ceh_desktop::appointment::private_field(
            root.path(),
            "fixture",
            id,
            ceh_desktop::appointment::PrivateField::Confirmation
        )
        .unwrap(),
        "PRIVATE-FIXTURE-123"
    );
    let path = root.path().join("data/achievements.json");
    let before = std::fs::read(&path).unwrap();
    assert_eq!(
        ceh_desktop::appointment::update(root.path(), "fixture", "unknown", &draft()).unwrap_err(),
        "achievement_not_found"
    );
    assert_eq!(std::fs::read(path).unwrap(), before);
}
#[test]
fn malformed_achievements_are_not_overwritten_or_returned_as_empty() {
    let root = fixture_root();
    std::fs::create_dir(root.path().join("data")).unwrap();
    let path = root.path().join("data/achievements.json");
    std::fs::write(&path, "PRIVATE BROKEN").unwrap();
    let at = chrono::DateTime::parse_from_rfc3339("2026-10-31T15:00:00Z")
        .unwrap()
        .to_utc();
    assert!(ceh_desktop::appointment::context_at(root.path(), "fixture", at).is_err());
    assert!(ceh_desktop::appointment::record_at(root.path(), "fixture", &draft(), at).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "PRIVATE BROKEN");
    assert_eq!(
        ceh_desktop::appointment::context_at(root.path(), "foreign", at).err(),
        Some("profile_not_found")
    );
}
