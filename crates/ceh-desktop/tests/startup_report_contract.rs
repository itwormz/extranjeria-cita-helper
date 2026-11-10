use ceh_desktop::startup_report::{clipboard_text, phase_of, save_failure};

fn at(minute: u32) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(&format!("2026-11-10T10:{minute:02}:00Z")).unwrap().to_utc()
}

#[test]
fn a_failed_start_leaves_a_private_report_that_copies_without_paths_or_profile_data() {
    let root = tempfile::tempdir().unwrap();
    let id = save_failure(root.path(), "/Users/Fictional", "browser_start_failed", true, at(0)).unwrap();
    assert!(id.starts_with("20261110T100000-"));
    let text = clipboard_text(root.path(), "/Users/Fictional", &id).unwrap();
    assert!(text.starts_with(&format!("SantaCita — CEH-{id}")));
    assert!(text.contains("browser_launch") && text.contains("browser_start_failed"));
    assert!(!text.contains("/Users/Fictional"), "home directory is scrubbed: {text}");
    // Only the fixed fields are kept: nothing else can be smuggled in later.
    let file = root.path().join(format!("data/startup-errors/{id}.json"));
    let mut stored: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    stored["profile"] = serde_json::json!({"document_number": "Y1234567X"});
    std::fs::write(&file, stored.to_string()).unwrap();
    assert!(!clipboard_text(root.path(), "/Users/Fictional", &id).unwrap().contains("Y1234567X"));
}

#[test]
fn only_ten_reports_are_kept_and_unknown_ids_are_refused() {
    let root = tempfile::tempdir().unwrap();
    let ids: Vec<_> = (0..12).map(|i| save_failure(root.path(), "/Users/Fictional", "firefox_not_installed", false, at(i)).unwrap()).collect();
    let kept = std::fs::read_dir(root.path().join("data/startup-errors")).unwrap().count();
    assert_eq!(kept, 10);
    assert!(clipboard_text(root.path(), "/Users/Fictional", &ids[11]).is_ok());
    for bad in ["../x", "nope", "20261110T100000-ZZZZZZZZ", ""] {
        assert_eq!(clipboard_text(root.path(), "/Users/Fictional", bad), Err("report_unavailable"));
    }
}

#[test]
fn launch_failures_and_later_failures_are_told_apart() {
    for code in ["firefox_not_installed", "browser_profile_unavailable", "browser_start_failed"] {
        assert_eq!(phase_of(code), "browser_launch");
    }
    for code in ["search_unavailable", "statistics_unavailable", "browser_connection_lost"] {
        assert_eq!(phase_of(code), "worker");
    }
}
