use ceh_desktop::history::{HistoryRange, selection_bounds};
fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-10-25T12:00:00Z")
        .unwrap()
        .to_utc()
}
#[test]
fn range_is_inclusive_civil_days_and_handles_dst() {
    let range = HistoryRange::Range {
        start: "2026-10-25".into(),
        end: "2026-10-25".into(),
    };
    let bounds = selection_bounds(&range, now()).unwrap();
    assert_eq!(
        bounds.start.unwrap().to_rfc3339(),
        "2026-10-24T22:00:00+00:00"
    );
    assert_eq!(
        (bounds.end.unwrap() - bounds.start.unwrap()).num_hours(),
        25
    );
    let all = selection_bounds(&HistoryRange::All {}, now()).unwrap();
    assert!(all.start.is_none() && all.end.is_none());
    let today = selection_bounds(&HistoryRange::Today {}, now()).unwrap();
    assert_eq!(today.start, bounds.start);
    assert_eq!(today.end, bounds.end);
}
#[test]
fn invalid_dates_are_errors_not_implicit_all_history() {
    for (start, end) in [
        ("2026-02-30", "2026-03-01"),
        ("2026-10-26", "2026-10-25"),
        ("2026-1-1", "2026-01-02"),
        ("garbage", "2026-10-25"),
        ("9999-12-31", "9999-12-31"),
    ] {
        assert_eq!(
            selection_bounds(
                &HistoryRange::Range {
                    start: start.into(),
                    end: end.into()
                },
                now()
            )
            .err(),
            Some("invalid_period")
        );
    }
    assert!(
        serde_json::from_value::<HistoryRange>(
            serde_json::json!({"period":"all","start":"2026-10-25"})
        )
        .is_err()
    );
}
#[test]
fn history_is_missing_without_creating_storage_and_rejects_unknown_profile() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("tasks.yml"),
        "tasks:\n  - id: fixture\n    label: Fixture\n",
    )
    .unwrap();
    assert!(
        ceh_desktop::history::profile_history(directory.path(), "fixture", &HistoryRange::All {})
            .unwrap()
            .is_none()
    );
    assert!(!directory.path().join("data").exists());
    assert_eq!(
        ceh_desktop::history::profile_history(
            directory.path(),
            "../../other",
            &HistoryRange::All {}
        )
        .err(),
        Some("profile_not_found")
    );
}
