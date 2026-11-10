use ceh_browser::firefox::{FirefoxError, FirefoxInstallation, ProfileLease};
#[test]
fn firefox_version_is_read_without_starting_the_browser() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("firefox");
    std::fs::write(&bin, "").unwrap();
    std::fs::write(
        dir.path().join("application.ini"),
        "[App]\nName=Firefox\nVersion=158.0.1\n",
    )
    .unwrap();
    let installation = FirefoxInstallation::from_executable(&bin).unwrap();
    assert_eq!(installation.version(), "158.0.1");
    std::fs::write(
        dir.path().join("application.ini"),
        "[App]\nName=Other\nVersion=158.0.1\n",
    )
    .unwrap();
    assert!(FirefoxInstallation::from_executable(&bin).is_err());
}
#[test]
fn profiles_are_exclusive_persistent_or_disposable_and_do_not_adopt_existing_data() {
    let root = tempfile::tempdir().unwrap();
    let first = ProfileLease::persistent(root.path(), "synthetic").unwrap();
    let path = first.path().to_owned();
    std::fs::write(path.join("synthetic-cookie"), "local").unwrap();
    assert!(matches!(
        ProfileLease::persistent(root.path(), "synthetic"),
        Err(FirefoxError::ProfileBusy)
    ));
    drop(first);
    let again = ProfileLease::persistent(root.path(), "synthetic").unwrap();
    assert_eq!(again.path(), path);
    assert!(path.join("synthetic-cookie").is_file());
    drop(again);
    let temporary = ProfileLease::temporary(root.path()).unwrap();
    let path = temporary.path().to_owned();
    assert!(path.is_dir());
    drop(temporary);
    assert!(!path.exists());
    assert!(ProfileLease::persistent(root.path(), "../private").is_err());
    let foreign = root.path().join("browser-profiles/firefox-foreign");
    std::fs::create_dir_all(&foreign).unwrap();
    std::fs::write(foreign.join("prefs.js"), "personal").unwrap();
    assert!(ProfileLease::persistent(root.path(), "foreign").is_err());
    assert!(foreign.join("prefs.js").is_file());
}
#[cfg(unix)]
#[test]
fn profile_symlinks_cannot_redirect_cleanup_or_settings_into_personal_data() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let personal = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("browser-profiles")).unwrap();
    symlink(
        personal.path(),
        root.path().join("browser-profiles/firefox-linked"),
    )
    .unwrap();
    assert!(ProfileLease::persistent(root.path(), "linked").is_err());
    assert!(std::fs::read_dir(personal.path()).unwrap().next().is_none());
}

#[test]
fn cancellation_and_symlinked_preferences_do_not_start_or_write_outside_owned_profile() {
    use ceh_browser::firefox::OwnedFirefox;
    use std::sync::atomic::AtomicBool;
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("firefox");
    std::fs::write(&bin, "").unwrap();
    std::fs::write(
        root.path().join("application.ini"),
        "[App]\nName=Firefox\nVersion=158.0.1\n",
    )
    .unwrap();
    let install = FirefoxInstallation::from_executable(&bin).unwrap();
    let profile = ProfileLease::temporary(root.path()).unwrap();
    let path = profile.path().to_owned();
    assert!(matches!(
        OwnedFirefox::start(&install, profile, true, &AtomicBool::new(true)),
        Err(FirefoxError::Cancelled)
    ));
    assert!(!path.exists());
    #[cfg(unix)]
    {
        let profile = ProfileLease::temporary(root.path()).unwrap();
        let personal = root.path().join("personal-prefs");
        std::fs::write(&personal, "unchanged").unwrap();
        std::os::unix::fs::symlink(&personal, profile.path().join("user.js")).unwrap();
        assert!(matches!(
            OwnedFirefox::start(&install, profile, true, &AtomicBool::new(false)),
            Err(FirefoxError::InvalidProfile)
        ));
        assert_eq!(std::fs::read_to_string(personal).unwrap(), "unchanged");
    }
}

/// Launches the installed Firefox (headless, temporary profile): run with `-- --ignored`.
#[test]
#[ignore = "launches the installed Firefox"]
fn resume_needs_a_handed_off_live_browser_and_returns_ownership() {
    use ceh_browser::firefox::OwnedFirefox;
    use std::sync::atomic::AtomicBool;
    let Ok(installation) = FirefoxInstallation::discover() else { return };
    let runtime = tempfile::tempdir().unwrap();
    let lease = ProfileLease::temporary(runtime.path()).unwrap();
    let mut browser = OwnedFirefox::start(&installation, lease, true, &AtomicBool::new(false)).unwrap();
    assert_eq!(browser.resume_automation(), Err(FirefoxError::Unavailable), "nothing was handed off");
    browser.handoff();
    assert_eq!(browser.stop(), Err(FirefoxError::ManualOwnership), "a handed-off browser is never closed by the app");
    browser.resume_automation().unwrap();
    assert_eq!(browser.resume_automation(), Err(FirefoxError::Unavailable), "a second resume has nothing to take back");
    browser.stop().unwrap();
}

#[test]
fn a_certificate_cache_failure_defers_the_cache_and_never_blocks_the_browser() {
    use ceh_browser::firefox::CertificateCache;
    // Python 0.3.6: the optional intermediate cache must not stop the browser (HTTPS stays strict).
    for error in [FirefoxError::CertificateCache, FirefoxError::Unavailable, FirefoxError::InvalidInstallation] {
        assert_eq!(CertificateCache::from_result(Err(error)), CertificateCache::Deferred);
    }
    assert_eq!(CertificateCache::from_result(Ok(3)), CertificateCache::Ready(3));
}

#[test]
fn startup_preferences_add_the_windows_occlusion_switches_only_on_windows() {
    use ceh_browser::firefox::startup_preferences;
    use ceh_browser::launch::TargetOs;
    // Python `_background_browser_prefs`: a covered Windows window must keep processing the search's input.
    let windows = startup_preferences(TargetOs::Windows);
    assert_eq!(windows["widget.windows.window_occlusion_tracking.enabled"], false);
    assert_eq!(windows["widget.windows.window_occlusion_tracking_peek.enabled"], false);
    for os in [TargetOs::Macos, TargetOs::Linux] {
        let prefs = startup_preferences(os);
        assert!(prefs.get("widget.windows.window_occlusion_tracking.enabled").is_none());
        assert!(prefs.get("widget.windows.window_occlusion_tracking_peek.enabled").is_none());
    }
    for os in [TargetOs::Macos, TargetOs::Windows, TargetOs::Linux] {
        let prefs = startup_preferences(os);
        assert_eq!(prefs["security.enterprise_roots.enabled"], true);
        assert_eq!(prefs["security.osclientcerts.autoload"], true);
        assert_eq!(prefs["dom.disable_window_flip"], true);
        assert_eq!(prefs["browser.tabs.loadDivertedInBackground"], true);
        // HTTPS validation is never loosened.
        assert!(prefs.as_object().unwrap().keys().all(|k| !k.contains("cert_pinning") && !k.contains("ignore_cert")));
    }
}

#[test]
fn resetting_a_session_removes_only_that_owned_idle_browser_profile() {
    let root = tempfile::tempdir().unwrap();
    // Nothing to reset is not an error (Python resets a profile that was never created).
    ProfileLease::reset_persistent(root.path(), "one").unwrap();
    let first = ProfileLease::persistent(root.path(), "one").unwrap();
    let first_path = first.path().to_owned();
    std::fs::write(first_path.join("cookies.sqlite"), "session").unwrap();
    let other = ProfileLease::persistent(root.path(), "two").unwrap();
    let other_path = other.path().to_owned();
    std::fs::write(other_path.join("cookies.sqlite"), "other").unwrap();
    drop(other);
    // A profile in use by a running browser is never touched.
    assert!(matches!(ProfileLease::reset_persistent(root.path(), "one"), Err(FirefoxError::ProfileBusy)));
    assert!(first_path.join("cookies.sqlite").is_file());
    drop(first);
    ProfileLease::reset_persistent(root.path(), "one").unwrap();
    assert!(!first_path.exists());
    assert!(other_path.join("cookies.sqlite").is_file(), "other profiles are untouched");
    // The next start gets a fresh owned profile.
    let again = ProfileLease::persistent(root.path(), "one").unwrap();
    assert!(!again.path().join("cookies.sqlite").exists());
}

#[test]
fn resetting_refuses_foreign_symlinked_and_invalid_profiles() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("browser-profiles")).unwrap();
    let foreign = root.path().join("browser-profiles/firefox-foreign");
    std::fs::create_dir(&foreign).unwrap();
    std::fs::write(foreign.join("prefs.js"), "personal").unwrap();
    assert!(ProfileLease::reset_persistent(root.path(), "foreign").is_err());
    assert!(foreign.join("prefs.js").is_file(), "an unmarked directory is not ours");
    #[cfg(unix)]
    {
        let personal = tempfile::tempdir().unwrap();
        std::fs::write(personal.path().join("keep"), "x").unwrap();
        std::os::unix::fs::symlink(personal.path(), root.path().join("browser-profiles/firefox-linked")).unwrap();
        assert!(ProfileLease::reset_persistent(root.path(), "linked").is_err());
        assert!(personal.path().join("keep").is_file());
    }
    assert!(ProfileLease::reset_persistent(root.path(), "../escape").is_err());
    assert!(ProfileLease::reset_persistent(root.path(), "").is_err());
}

#[test]
fn a_context_tree_lists_the_page_only_while_it_is_open() {
    use ceh_browser::firefox::context_is_listed;
    let tree = serde_json::json!({"contexts":[{"context":"a","children":[]},{"context":"b","children":[{"context":"c","children":[]}]}]});
    assert!(context_is_listed(&tree, "a"));
    assert!(context_is_listed(&tree, "c"), "child frames count");
    assert!(!context_is_listed(&tree, "gone"));
    assert!(!context_is_listed(&serde_json::json!({"contexts":[]}), "a"));
    assert!(!context_is_listed(&serde_json::json!({}), "a"));
}

/// Launches the installed Firefox (headless, temporary profile): run with `-- --ignored`.
#[test]
#[ignore = "launches the installed Firefox"]
fn real_firefox_reports_when_the_person_closes_the_page() {
    use ceh_browser::firefox::OwnedFirefox;
    use std::sync::atomic::AtomicBool;
    let Ok(installation) = FirefoxInstallation::discover() else { return };
    let runtime = tempfile::tempdir().unwrap();
    let lease = ProfileLease::temporary(runtime.path()).unwrap();
    let mut browser = OwnedFirefox::start(&installation, lease, true, &AtomicBool::new(false)).unwrap();
    assert!(browser.page_open());
    // Opening a second tab keeps ours listed; closing ours is noticed.
    let context = browser.context().to_owned();
    browser.connection().call("browsingContext.create", serde_json::json!({"type":"tab"})).unwrap();
    assert!(browser.page_open());
    browser.connection().call("browsingContext.close", serde_json::json!({"context":context})).unwrap();
    assert!(!browser.page_open());
    let _ = browser.stop();
}
