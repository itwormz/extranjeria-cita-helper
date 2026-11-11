use ceh_desktop::navigation_failure;

#[test]
fn a_page_the_person_closed_while_it_loaded_is_a_stop_not_a_failure() {
    assert_eq!(navigation_failure(false, false), None);
    assert_eq!(navigation_failure(false, true), None);
}

#[test]
fn a_rejected_first_page_names_the_missing_certificate_chain_when_it_could_not_be_prepared() {
    assert_eq!(navigation_failure(true, true), Some("browser_certificate_cache_failed"));
    assert_eq!(navigation_failure(true, false), Some("browser_navigation_failed"));
}

#[test]
fn the_macos_bundle_may_load_the_firefox_libraries_that_prepare_the_certificate_chain() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let config = std::fs::read_to_string(dir.join("tauri.conf.json")).unwrap();
    assert!(config.contains("\"entitlements\": \"entitlements.plist\""), "the bundle is signed with entitlements");
    let plist = std::fs::read_to_string(dir.join("entitlements.plist")).unwrap();
    assert!(plist.contains("com.apple.security.cs.disable-library-validation"), "hardened runtime rejects Mozilla's NSS otherwise");
}

#[test]
fn a_browser_error_after_the_person_closed_the_window_is_a_stop_not_an_error() {
    use ceh_desktop::closed_window_stop;
    for code in ["browser_connection_lost", "browser_observation_failed", "browser_action_failed", "browser_recovery_required", "browser_navigation_failed"] {
        assert_eq!(closed_window_stop(Err(code), "stop", || false), Ok("stop"), "{code}");
    }
    // The window is still there: the error stays an error, and no probing happens for other kinds of errors.
    assert_eq!(closed_window_stop(Err("browser_action_failed"), "stop", || true), Err("browser_action_failed"));
    assert_eq!(closed_window_stop(Err("statistics_unavailable"), "stop", || panic!("must not probe")), Err("statistics_unavailable"));
    assert_eq!(closed_window_stop(Ok("done"), "stop", || panic!("must not probe")), Ok("done"));
}

#[test]
fn the_certificate_reminder_rings_once_five_seconds_after_the_click_even_if_the_page_cannot_be_read() {
    use ceh_desktop::certificate_reminder_due;
    assert!(!certificate_reminder_due(None, false, 99_999));
    assert!(!certificate_reminder_due(Some(10_000), false, 14_999), "not yet");
    assert!(certificate_reminder_due(Some(10_000), false, 15_000));
    assert!(!certificate_reminder_due(Some(10_000), true, 60_000), "once");
}

#[test]
fn only_the_availability_request_counts_as_a_check_not_reloads_of_the_waiting_page() {
    use ceh_core::model::ActionKind;
    use ceh_desktop::counts_as_check;
    assert!(counts_as_check(ActionKind::Request), "the submitted Solicitar cita");
    for action in [ActionKind::RequestReload, ActionKind::ApplicantReload, ActionKind::Applicant, ActionKind::Province, ActionKind::Recovery, ActionKind::Cookie] {
        assert!(!counts_as_check(action), "{action:?}");
    }
}
