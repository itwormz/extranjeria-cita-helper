use ceh_desktop::{bootstrap, trusted_origin};
#[test]
fn missing_preview_profile_is_empty_without_creating_files() {
    let directory = tempfile::tempdir().unwrap();
    let result = bootstrap(directory.path(), "ru").unwrap();
    assert!(result.profiles.is_empty());
    assert!(result.selected_profile.is_none());
    assert!(!result.browser_connected);
    assert!(
        std::fs::read_dir(directory.path())
            .unwrap()
            .next()
            .is_none()
    );
}
#[test]
fn malformed_profile_error_has_no_file_content() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("tasks.yml"),
        "tasks: [SECRET-DONT-ECHO",
    )
    .unwrap();
    let error = bootstrap(directory.path(), "ru").err().unwrap();
    assert_eq!(error, "profiles_unavailable");
}
#[test]
fn only_local_main_window_may_invoke() {
    for url in [
        "tauri://localhost/index.html",
        "http://tauri.localhost/",
        "https://tauri.localhost/",
        "http://127.0.0.1:1420/",
    ] {
        assert!(trusted_origin("main", url), "{url}");
        assert!(!trusted_origin("site", url));
    }
    for url in [
        "https://icp.administracionelectronica.gob.es/",
        "https://localhost/",
        "file:///tmp/ui.html",
        "http://127.0.0.1:1421/",
        "http://127.0.0.1:1420@evil.example/",
        "data:text/html,test",
    ] {
        assert!(!trusted_origin("main", url), "{url}");
    }
}

#[test]
fn bootstrap_summaries_exclude_applicant_contact_and_secrets() {
    let directory = tempfile::tempdir().unwrap();
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/fixtures/legacy-profiles.json"
    ))
    .unwrap();
    let case = corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == "huellas_defaults")
        .unwrap();
    std::fs::write(
        directory.path().join("tasks.yml"),
        case["source"].as_str().unwrap(),
    )
    .unwrap();
    let value = serde_json::to_string(&bootstrap(directory.path(), "ru").unwrap()).unwrap();
    for private in [
        "X0000000T",
        "FICTITIOUS PERSON",
        "600000000",
        "fixture@example.test",
        "notifications_ref",
    ] {
        assert!(!value.contains(private));
    }
    assert!(value.contains("fixture"));
}
#[test]
fn editor_defaults_and_selected_profile_are_lazy_and_scoped() {
    let directory = tempfile::tempdir().unwrap();
    let editor = ceh_desktop::profile_editor(directory.path(), None).unwrap();
    assert_eq!(editor.draft.procedure, "huellas");
    assert_eq!(editor.draft.no_slots_mode, "salir");
    assert_eq!(editor.draft.applicant_no_slots_mode, "wait");
    assert!(editor.draft.document_number.is_empty());
    assert!(!editor.provinces.is_empty());
    assert_eq!(
        ceh_desktop::profile_editor(directory.path(), Some("missing"))
            .err()
            .unwrap(),
        "profile_missing"
    );
    assert!(
        std::fs::read_dir(directory.path())
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn editor_exposes_validated_default_timings_without_runtime_access() {
    let directory = tempfile::tempdir().unwrap();
    let editor = ceh_desktop::profile_editor(directory.path(), None).unwrap();
    assert_eq!(editor.timings.reload_seconds(), [30., 60.]);
    assert_eq!(
        serde_json::to_value(editor).unwrap()["timings"]["blocked_seconds"][0],
        30.
    );
}

#[test]
fn diagnostics_folder_is_scoped_private_and_does_not_create_profiles() {
    let root = tempfile::tempdir().unwrap();
    let folder = ceh_desktop::diagnostics_directory(root.path()).unwrap();
    assert_eq!(folder, root.path().join("diagnostics"));
    assert!(folder.is_dir());
    assert!(!root.path().join("tasks.yml").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&folder).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    assert_eq!(
        ceh_desktop::diagnostics_directory(root.path()).unwrap(),
        folder
    );
}
#[cfg(unix)]
#[test]
fn diagnostics_folder_refuses_a_link_to_another_directory() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(external.path(), root.path().join("diagnostics")).unwrap();
    assert!(ceh_desktop::diagnostics_directory(root.path()).is_err());
    assert_eq!(std::fs::read_dir(external.path()).unwrap().count(), 0);
}

#[test]
fn new_profile_inherits_validated_global_timezone() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("tasks.yml"),
        "settings: {timezone: Atlantic/Canary}\ntasks: []\n",
    )
    .unwrap();
    let editor = ceh_desktop::profile_editor(root.path(), None).unwrap();
    assert_eq!(editor.draft.timezone, "Atlantic/Canary");
}

#[test]
fn statistics_requires_an_owned_profile_and_does_not_create_missing_history() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("tasks.yml"),
        "tasks:\n  - id: fixture\n    label: Fixture\n",
    )
    .unwrap();
    assert!(
        ceh_desktop::profile_statistics(directory.path(), "fixture")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        ceh_desktop::profile_statistics(directory.path(), "../../other").err(),
        Some("profile_not_found")
    );
    assert!(!directory.path().join("data").exists());
    std::fs::create_dir(directory.path().join("data")).unwrap();
    let path = directory.path().join("data/profile-statistics.sqlite3");
    std::fs::write(&path, "SENSITIVE-CORRUPT-CONTENT").unwrap();
    assert_eq!(
        ceh_desktop::profile_statistics(directory.path(), "fixture").err(),
        Some("statistics_unavailable")
    );
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "SENSITIVE-CORRUPT-CONTENT"
    );
}

#[test]
fn statistics_counts_achievements_by_profile_and_madrid_day_without_private_fields() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("tasks.yml"),
        "tasks:\n  - id: fixture\n    label: Fixture\n",
    )
    .unwrap();
    std::fs::create_dir(directory.path().join("data")).unwrap();
    let private = directory.path().join("data/achievements.json");
    std::fs::write(&private, serde_json::json!({"fixture":[
        {"recorded_at":"2026-10-24T21:59:59Z","appointment":{"confirmation":"PRIVATE-DO-NOT-EXPORT"}},
        {"recorded_at":"2026-10-24T22:00:00Z"},
        {"recorded_at":"2026-10-25T22:59:59Z"},
        {"recorded_at":"2026-10-25T23:00:00Z"}
    ],"other":[{"recorded_at":"2026-10-25T10:00:00Z"}]}).to_string()).unwrap();
    let before = std::fs::read(&private).unwrap();
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-25T12:00:00Z")
        .unwrap()
        .to_utc();
    let result = ceh_desktop::profile_statistics_at(directory.path(), "fixture", now)
        .unwrap()
        .unwrap();
    assert_eq!(result.all["successes"], 4);
    assert_eq!(result.today["successes"], 2);
    assert!(result.all.get("checks").is_none());
    assert!(!serde_json::to_string(&result).unwrap().contains("PRIVATE"));
    assert_eq!(std::fs::read(&private).unwrap(), before);
    assert!(
        !directory
            .path()
            .join("data/profile-statistics.sqlite3")
            .exists()
    );
    std::fs::write(
        &private,
        r#"{"fixture":[{"recorded_at":"invalid SECRET"}]}"#,
    )
    .unwrap();
    assert_eq!(
        ceh_desktop::profile_statistics_at(directory.path(), "fixture", now).err(),
        Some("statistics_unavailable")
    );
}

#[test]
fn preview_runtime_selects_only_explicit_owned_legacy_copies() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(
        ceh_desktop::preview_runtime_root(root.path(), None)
            .unwrap()
            .0,
        root.path().join("rust-preview")
    );
    assert!(ceh_desktop::preview_runtime_root(root.path(), Some("../../outside")).is_err());
    assert!(ceh_desktop::preview_runtime_root(root.path(), Some("stable")).is_err());
    let copy = root.path().join("legacy-compat/stable/rust");
    std::fs::create_dir_all(&copy).unwrap();
    std::fs::write(copy.join("tasks.yml"), "tasks: []").unwrap();
    let selected = ceh_desktop::preview_runtime_root(root.path(), Some("stable")).unwrap();
    assert_eq!(selected.0, copy.canonicalize().unwrap());
    assert_eq!(selected.1, "legacy_copy");
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        let link = root.path().join("legacy-compat/historical/rust");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        assert!(ceh_desktop::preview_runtime_root(root.path(), Some("historical")).is_err());
    }
}

#[test]
fn statistics_today_budget_uses_python_summary_keys() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("tasks.yml"),
        "tasks:\n  - id: fixture\n    label: Fixture\n",
    )
    .unwrap();
    let budgets = directory.path().join("data/appointment-budget");
    std::fs::create_dir_all(&budgets).unwrap();
    let key = "a".repeat(64);
    std::fs::write(budgets.join("profile-fixture.json"), format!(r#"{{"key":"{key}"}}"#)).unwrap();
    std::fs::write(budgets.join(format!("{key}.json")), r#"{"day":"2026-10-25","sent":2,"banners":1}"#).unwrap();
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-25T12:00:00Z").unwrap().to_utc();
    let result = ceh_desktop::profile_statistics_at(directory.path(), "fixture", now).unwrap().unwrap();
    let budget = &result.today["appointment_budget"];
    for key in ["target_source", "episode_history", "wait_reason", "remaining", "ready", "banners", "restricted"] {
        assert!(budget.get(key).is_some(), "missing {key}");
    }
    assert!(budget.get("history").is_none());
    assert_eq!(budget["sent"], 2);
    assert_eq!(budget["target"], 5);
}
