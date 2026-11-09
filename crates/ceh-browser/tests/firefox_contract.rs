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
