use ceh_core::control::ControlMode;
use ceh_services::diagnostics::{CapturePolicy, ScriptArchive, script_reference, script_url};
#[test]
fn resource_identity_removes_query_from_metadata_but_not_reference() {
    let raw = "https://icp.administracionelectronica.gob.es/icpplus/js/app.js;jsessionid=FICTITIOUS?v=1#top";
    assert_eq!(
        script_url(raw).unwrap(),
        "https://icp.administracionelectronica.gob.es/icpplus/js/app.js"
    );
    assert_ne!(
        script_reference(raw),
        script_reference(&raw.replace("v=1", "v=2"))
    );
    assert!(script_url("https://external.example.test/code.js").is_none());
}
#[test]
fn screenshot_policy_never_acquires_manual_or_paused_page() {
    for mode in [ControlMode::Manual, ControlMode::Paused] {
        let policy = CapturePolicy {
            enabled: true,
            mode,
            screen_locked: false,
            first_party: true,
        };
        assert!(!policy.screenshot());
        assert!(!policy.dom_snapshot());
        assert!(policy.passive_response());
    }
}
#[test]
fn resource_archive_deduplicates_content_and_counts_loads() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/builds/work/diagnostics-tests");
    std::fs::create_dir_all(&root).unwrap();
    let directory = tempfile::tempdir_in(root).unwrap();
    let archive = ScriptArchive::new(directory.path());
    let session = archive
        .prepare_session("fixture-session", 14, 1000.0, false)
        .unwrap();
    let url = "https://icp.administracionelectronica.gob.es/icpplus/js/test.js?v=1";
    let first = archive
        .save_script(
            &session,
            url,
            b"function fictitious() {}",
            200,
            "application/javascript",
            1000.0,
        )
        .unwrap()
        .unwrap();
    let second = archive
        .save_script(
            &session,
            url,
            b"function fictitious() {}",
            200,
            "text/javascript",
            1001.0,
        )
        .unwrap()
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(
        std::fs::read_dir(directory.path().join("resources"))
            .unwrap()
            .count(),
        1
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(session.join("resources.json")).unwrap()).unwrap();
    assert_eq!(manifest[0]["load_count"], 2);
    assert!(!manifest.to_string().contains("?v="));
    assert!(
        archive
            .save_script(
                &session,
                url,
                b"<html>request rejected</html>",
                200,
                "text/javascript",
                1002.0
            )
            .unwrap()
            .is_none()
    );
}

#[test]
fn startup_report_is_private_bounded_and_scrubbed_again_on_copy() {
    use ceh_services::diagnostics::{Sanitizer, StartupReports};
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/builds/work/diagnostics-tests");
    std::fs::create_dir_all(&base).unwrap();
    let directory = tempfile::tempdir_in(base).unwrap();
    let mut sanitizer = Sanitizer::default();
    sanitizer.register(["FICTITIOUS_PRIVATE".into()]);
    let reports = StartupReports::new(directory.path(), sanitizer, "/Users/Fictional");
    let at = chrono::DateTime::parse_from_rfc3339("2026-10-30T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let mut last = String::new();
    for _ in 0..12 {
        last=reports.save(serde_json::json!({"message":"FICTITIOUS_PRIVATE password=demo /Users/Fictional/app user@example.test","phase":"browser_launch","profile":"never copy raw profile","environment":{"token":"never copy"}}),at).unwrap();
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 10);
    let report = reports.read(&last).unwrap().unwrap();
    let text = reports.clipboard(&last, report).unwrap();
    for private in [
        "FICTITIOUS_PRIVATE",
        "password=demo",
        "user@example.test",
        "/Users/Fictional",
        "never copy",
    ] {
        assert!(!text.contains(private));
    }
    assert!(reports.read("../../invalid").unwrap().is_none());
}
#[test]
fn malformed_kept_manifest_does_not_delete_objects() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/builds/work/diagnostics-tests");
    std::fs::create_dir_all(&root).unwrap();
    let dir = tempfile::tempdir_in(root).unwrap();
    let archive = ScriptArchive::new(dir.path());
    let kept = archive.prepare_session("kept", 14, 1000.0, false).unwrap();
    std::fs::create_dir_all(dir.path().join("resources")).unwrap();
    let object = dir.path().join("resources/unreferenced.js.gz");
    std::fs::write(&object, b"fictitious").unwrap();
    std::fs::write(kept.join("resources.json"), b"not JSON").unwrap();
    archive
        .prepare_session("current", 14, 1001.0, true)
        .unwrap();
    assert!(object.exists());
}

#[test]
fn response_budget_is_bounded_and_released_without_browser_access() {
    use ceh_services::diagnostics::{ResponseBudget, ResponseKind};
    let budget = ResponseBudget::default();
    let permits = (0..8)
        .map(|_| budget.acquire(ResponseKind::Script).unwrap())
        .collect::<Vec<_>>();
    assert!(budget.acquire(ResponseKind::Script).is_none());
    assert!(budget.acquire(ResponseKind::Navigation).is_some());
    drop(permits);
    assert!(budget.acquire(ResponseKind::Script).is_some());
}

#[test]
fn capture_ticket_is_revoked_by_pause_navigation_or_deadline() {
    use ceh_core::{
        control::Authority,
        model::{DocumentGuard, DocumentId, PageKind, SessionId},
    };
    use ceh_services::diagnostics::CaptureTracker;
    let mut authority = Authority::new(SessionId::new("fixture-session").unwrap());
    let guard = DocumentGuard {
        session: SessionId::new("fixture-session").unwrap(),
        epoch: authority.epoch(),
        document: DocumentId::new("fixture-doc").unwrap(),
        expected_page: PageKind::Province,
    };
    let policy = CapturePolicy {
        enabled: true,
        mode: ControlMode::Automatic,
        screen_locked: false,
        first_party: true,
    };
    let mut tracker = CaptureTracker::default();
    let ticket = tracker
        .begin(
            &guard,
            &"a".repeat(64),
            &authority.snapshot(),
            &policy,
            0,
            true,
        )
        .unwrap();
    let paused = authority.pause().unwrap();
    assert!(!tracker.finish(ticket, &guard, &paused, &policy, 1, true));
    let current = authority.resume().unwrap();
    let mut guard = guard;
    guard.epoch = current.epoch();
    let ticket = tracker
        .begin(&guard, &"b".repeat(64), &current, &policy, 0, true)
        .unwrap();
    assert!(!tracker.finish(ticket, &guard, &current, &policy, 10_001, true));
    let ticket = tracker
        .begin(&guard, &"b".repeat(64), &current, &policy, 20_000, true)
        .unwrap();
    assert!(tracker.finish(ticket, &guard, &current, &policy, 20_001, true));
    assert!(
        tracker
            .begin(&guard, &"b".repeat(64), &current, &policy, 20_002, true)
            .is_none()
    );
}
#[test]
fn resource_retention_deletes_expired_sessions_and_only_unreferenced_objects() {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/builds/work/diagnostics-tests");
    std::fs::create_dir_all(&base).unwrap();
    let dir = tempfile::tempdir_in(base).unwrap();
    let archive = ScriptArchive::new(dir.path());
    let old = archive.prepare_session("old", 1, 1000.0, false).unwrap();
    archive
        .save_script(
            &old,
            "https://icp.administracionelectronica.gob.es/icpplus/js/test.js",
            b"function old(){}",
            200,
            "application/javascript",
            1000.0,
        )
        .unwrap();
    let current = archive.prepare_session("new", 1, 200_000.0, true).unwrap();
    assert!(current.exists());
    assert!(!old.exists());
    assert_eq!(
        std::fs::read_dir(dir.path().join("resources"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn script_references_keep_pending_loads_without_requests() {
    use ceh_services::diagnostics::ScriptSources;
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/builds/work/diagnostics-tests");
    std::fs::create_dir_all(&base).unwrap();
    let dir = tempfile::tempdir_in(base).unwrap();
    let archive = ScriptArchive::new(dir.path());
    let session = archive
        .prepare_session("fixture", 14, 1000.0, false)
        .unwrap();
    let url = "https://icp.administracionelectronica.gob.es/icpplus/js/test.js?v=1";
    archive
        .save_script(
            &session,
            url,
            b"function demo(){}",
            200,
            "text/javascript",
            1000.0,
        )
        .unwrap();
    let rows = archive
        .references(
            &session,
            &[ScriptSources {
                url: "https://icp.administracionelectronica.gob.es/icpplus/start".into(),
                base: Some("/icpplus/js/".into()),
                sources: vec![
                    "test.js?v=1".into(),
                    "test.js?v=2".into(),
                    "https://external.example.test/private.js".into(),
                ],
            }],
        )
        .unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[0]["state"], "captured");
    assert_eq!(rows[1]["state"], "missing");
}

#[test]
fn script_body_with_registered_private_value_is_not_archived() {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/builds/work/diagnostics-tests");
    std::fs::create_dir_all(&base).unwrap();
    let dir = tempfile::tempdir_in(base).unwrap();
    let mut archive = ScriptArchive::new(dir.path());
    archive.register_sensitive(["FICTITIOUS_PRIVATE".into()]);
    let session = archive
        .prepare_session("fixture", 14, 1000.0, false)
        .unwrap();
    assert!(
        archive
            .save_script(
                &session,
                "https://icp.administracionelectronica.gob.es/icpplus/js/test.js",
                b"const token='FICTITIOUS_PRIVATE';",
                200,
                "text/javascript",
                1000.0
            )
            .unwrap()
            .is_none()
    );
    assert!(!dir.path().join("resources").exists());
}

#[test]
fn a_busy_archive_fails_quickly_instead_of_blocking_flow() {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/builds/work/diagnostics-tests");
    std::fs::create_dir_all(&base).unwrap();
    let dir = tempfile::tempdir_in(base).unwrap();
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.path().join(".resources.lock"))
        .unwrap();
    fs2::FileExt::lock_exclusive(&file).unwrap();
    let started = std::time::Instant::now();
    let result = ScriptArchive::new(dir.path()).prepare_session("fixture", 14, 1000.0, false);
    assert_eq!(
        result.err(),
        Some(ceh_services::diagnostics::DiagnosticError::Busy)
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
}
#[cfg(unix)]
#[test]
fn retention_does_not_follow_resource_symlink() {
    use std::os::unix::fs::symlink;
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/builds/work/diagnostics-tests");
    std::fs::create_dir_all(&base).unwrap();
    let dir = tempfile::tempdir_in(&base).unwrap();
    let target = tempfile::tempdir_in(base).unwrap();
    let protected = target.path().join("protected.js.gz");
    std::fs::write(&protected, b"fictitious protected file").unwrap();
    symlink(target.path(), dir.path().join("resources")).unwrap();
    assert!(
        ScriptArchive::new(dir.path())
            .prepare_session("fixture", 14, 1000.0, true)
            .is_err()
    );
    assert!(protected.exists());
}

#[test]
fn diagnostic_cleaning_matches_python_contract() {
    use ceh_services::diagnostics::Sanitizer;
    let data: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/fixtures/diagnostics.json"
    ))
    .unwrap();
    for case in data["cases"].as_array().unwrap() {
        let input = &case["input"];
        let text = input["text"].as_str().unwrap();
        let mut sanitizer = Sanitizer::default();
        sanitizer.register(
            input["known"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| v.as_str().unwrap().to_owned()),
        );
        let result = match input["op"].as_str().unwrap() {
            "resource" => {
                serde_json::json!({"url":script_url(text),"reference":script_reference(text)})
            }
            "sanitize" => serde_json::json!(sanitizer.sanitize(text)),
            "clean" => serde_json::json!(sanitizer.clean_startup(text, "/Users/Fictional")),
            _ => panic!("unknown fixture"),
        };
        assert_eq!(result, case["expected"], "{}", case["id"]);
    }
}
#[test]
fn log_urls_do_not_retain_embedded_credentials() {
    let cleaned = ceh_services::diagnostics::Sanitizer::default()
        .sanitize("https://user:FICTITIOUS@example.test/path?token=FICTITIOUS");
    assert_eq!(cleaned, "https://example.test/path");
}

#[test]
fn clipboard_drops_fields_outside_technical_report_schema() {
    use ceh_services::diagnostics::{Sanitizer, StartupReports};
    let reports = StartupReports::new(
        std::path::Path::new("unused"),
        Sanitizer::default(),
        "/Users/Fictional",
    );
    let text = reports
        .clipboard(
            "20261030T120000-1234abcd",
            serde_json::json!({"profile":"FICTITIOUS_UNDECLARED","message":"technical failure"}),
        )
        .unwrap();
    assert!(!text.contains("FICTITIOUS_UNDECLARED"));
    assert!(text.contains("technical failure"));
}

#[test]
fn path_inspection_distinguishes_missing_and_existing_without_raw_errors() {
    use ceh_services::diagnostics::{Sanitizer, inspect_path};
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/builds/work/diagnostics-tests");
    std::fs::create_dir_all(&base).unwrap();
    let dir = tempfile::tempdir_in(base).unwrap();
    let path = dir.path().join("fictitious-browser");
    let missing = inspect_path(&path, &Sanitizer::default(), "/Users/Fictional");
    assert_eq!(missing["exists"], false);
    assert!(missing["stat_error"].is_object());
    std::fs::write(&path, b"fictitious").unwrap();
    let found = inspect_path(&path, &Sanitizer::default(), "/Users/Fictional");
    assert_eq!(found["exists"], true);
    assert_eq!(found["is_file"], true);
}
