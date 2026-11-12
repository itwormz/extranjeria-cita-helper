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

#[cfg(unix)]
#[test]
fn a_profile_held_by_a_live_firefox_is_busy_even_without_this_apps_lock() {
    // After an app restart the flock is gone, but Firefox's own `lock` symlink names its live pid.
    let root = tempfile::tempdir().unwrap();
    let held = ProfileLease::persistent(root.path(), "orphan").unwrap();
    let path = held.path().to_owned();
    drop(held);
    let lock = path.join("lock");
    let alive = format!("127.0.0.1:+{}", std::process::id());
    std::os::unix::fs::symlink(&alive, &lock).unwrap();
    assert!(matches!(ProfileLease::persistent(root.path(), "orphan"), Err(FirefoxError::ProfileBusy)));
    assert!(matches!(ProfileLease::reset_persistent(root.path(), "orphan"), Err(FirefoxError::ProfileBusy)));
    assert!(path.exists(), "a live browser's profile is never deleted");
    // A stale lock left by a crash (dead pid) does not block.
    std::fs::remove_file(&lock).unwrap();
    std::os::unix::fs::symlink("127.0.0.1:+2147483646", &lock).unwrap();
    drop(ProfileLease::persistent(root.path(), "orphan").unwrap());
    ProfileLease::reset_persistent(root.path(), "orphan").unwrap();
    assert!(!path.exists());
}

/// Launches the installed Firefox (headless, temporary profile): run with `-- --ignored`.
#[test]
#[ignore = "launches the installed Firefox"]
fn a_page_that_is_gone_lets_the_app_quit_the_browser_even_after_a_handoff() {
    use ceh_browser::firefox::OwnedFirefox;
    use std::sync::atomic::AtomicBool;
    let Ok(installation) = FirefoxInstallation::discover() else { return };
    let runtime = tempfile::tempdir().unwrap();
    let lease = ProfileLease::persistent(runtime.path(), "gone").unwrap();
    let mut browser = OwnedFirefox::start(&installation, lease, true, &AtomicBool::new(false)).unwrap();
    browser.handoff();
    assert!(!browser.quit_if_page_gone(), "an open page keeps the browser");
    let context = browser.context().to_owned();
    browser.connection().call("browsingContext.create", serde_json::json!({"type":"tab"})).unwrap();
    browser.connection().call("browsingContext.close", serde_json::json!({"context":context})).unwrap();
    assert!(browser.quit_if_page_gone(), "the person's page is gone: nothing to preserve");
    // The profile lease is released, so the next search can start.
    drop(browser);
    drop(ProfileLease::persistent(runtime.path(), "gone").unwrap());
}

#[test]
fn a_reset_never_leaves_an_unmarked_half_deleted_profile_behind() {
    let root = tempfile::tempdir().unwrap();
    let lease = ProfileLease::persistent(root.path(), "one").unwrap();
    let path = lease.path().to_owned();
    std::fs::create_dir(path.join("storage")).unwrap();
    std::fs::write(path.join("storage/data"), "x").unwrap();
    drop(lease);
    // A tombstone of an earlier reset that could not be deleted completely is cleaned up by the next one.
    let tombstone = path.parent().unwrap().join("firefox-one.deleting-1");
    std::fs::create_dir(&tombstone).unwrap();
    std::fs::write(tombstone.join("old"), "x").unwrap();
    ProfileLease::reset_persistent(root.path(), "one").unwrap();
    assert!(!path.exists());
    assert!(!tombstone.exists());
    let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap()).unwrap().flatten().map(|e| e.file_name().into_string().unwrap()).collect();
    assert!(leftovers.iter().all(|name| !name.contains("deleting")), "{leftovers:?}");
    // The key is usable again with a fresh owned profile.
    assert!(ProfileLease::persistent(root.path(), "one").unwrap().path().join("ceh-owned-profile").is_file());
}

/// Launches the installed Firefox on a local `data:` page: run with `-- --ignored`.
#[test]
#[ignore = "launches the installed Firefox"]
fn a_started_firefox_reports_webdriver_false_with_a_native_looking_getter() {
    use ceh_browser::firefox::OwnedFirefox;
    use std::sync::atomic::AtomicBool;
    let Ok(installation) = FirefoxInstallation::discover() else { return };
    let runtime = tempfile::tempdir().unwrap();
    let lease = ProfileLease::temporary(runtime.path()).unwrap();
    let mut browser = OwnedFirefox::start(&installation, lease, true, &AtomicBool::new(false)).unwrap();
    let context = browser.context().to_owned();
    let connection = browser.connection();
    connection.call("browsingContext.navigate", serde_json::json!({"context":context,"url":"data:text/html,<title>t</title>","wait":"complete"})).unwrap();
    let result = connection.call("script.evaluate", serde_json::json!({"expression":"[navigator.webdriver, Object.getOwnPropertyDescriptor(Navigator.prototype,'webdriver').get.toString().includes('[native code]')]","target":{"context":context},"awaitPromise":false})).unwrap();
    let values = result["result"]["value"].as_array().unwrap();
    assert_eq!(values[0]["value"], false);
    assert_eq!(values[1]["value"], true);
    let _ = browser.stop();
}

#[test]
fn each_browser_keeps_its_own_profile_for_a_search_profile_and_a_reset_touches_only_its_own() {
    use ceh_browser::firefox::ProfileKind;
    let runtime = tempfile::tempdir().unwrap();
    let firefox = ProfileLease::persistent(runtime.path(), "p1").unwrap();
    let chrome = ProfileLease::persistent_kind(runtime.path(), "p1", ProfileKind::Chrome).unwrap();
    let brave = ProfileLease::persistent_kind(runtime.path(), "p1", ProfileKind::Brave).unwrap();
    let paths = [firefox.path().to_owned(), chrome.path().to_owned(), brave.path().to_owned()];
    assert!(paths[1].ends_with("chrome-p1") && paths[2].ends_with("brave-p1") && paths[0].ends_with("firefox-p1"));
    drop(chrome);
    ProfileLease::reset_persistent_kind(runtime.path(), "p1", ProfileKind::Chrome).unwrap();
    assert!(!paths[1].exists() && paths[0].exists() && paths[2].exists());
    // A directory with another browser's marker is refused.
    drop(brave);
    std::fs::write(paths[2].join("ceh-owned-profile"), b"CEH Firefox profile v1\n").unwrap();
    assert!(ProfileLease::persistent_kind(runtime.path(), "p1", ProfileKind::Brave).is_err());
}

#[test]
fn the_places_where_linux_distributions_keep_firefox_are_all_searched() {
    let places = ceh_browser::firefox::linux_firefox_locations();
    // Mozilla's tarball and apt repository, Fedora/Arch lib64, and Debian's Firefox ESR (its program is `firefox-esr`).
    for expected in ["/usr/lib/firefox/firefox", "/usr/lib64/firefox/firefox", "/opt/firefox/firefox", "/usr/lib/firefox-esr/firefox-esr", "/usr/lib/firefox-esr/firefox"] {
        assert!(places.iter().any(|p| p == std::path::Path::new(expected)), "{expected} in {places:?}");
    }
    assert!(places.iter().all(|p| p.has_root()), "rooted Linux locations, even when this test runs on Windows");
}
