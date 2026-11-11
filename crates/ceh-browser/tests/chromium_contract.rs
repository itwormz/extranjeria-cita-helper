use ceh_browser::chromium::{launch_arguments, parse_devtools_active_port, parse_version};
use ceh_browser::launch::TargetOs;
use std::path::Path;

fn joined(os: TargetOs) -> Vec<String> {
    launch_arguments(Path::new("/data/chrome-p1"), os)
}

#[test]
fn chrome_starts_like_an_ordinary_browser_with_its_own_profile_and_a_loopback_debug_port() {
    let args = joined(TargetOs::Macos);
    assert!(args.contains(&"--remote-debugging-port=0".to_owned()));
    assert!(args.contains(&"--user-data-dir=/data/chrome-p1".to_owned()));
    assert!(args.contains(&"--no-first-run".to_owned()) && args.contains(&"--no-default-browser-check".to_owned()));
    assert!(args.contains(&"--lang=es-ES".to_owned()));
    // Closing the last window must end the browser (Edge and Chrome otherwise keep a background process alive
    // on Windows, and the app could not tell that the person had closed it).
    for os in [TargetOs::Macos, TargetOs::Windows, TargetOs::Linux] {
        assert!(joined(os).contains(&"--disable-background-mode".to_owned()), "{os:?}");
    }
    assert_eq!(args.last().unwrap(), "about:blank");
}

#[test]
fn no_flag_that_marks_the_browser_as_automated_or_changes_it_from_a_normal_one() {
    for os in [TargetOs::Macos, TargetOs::Windows, TargetOs::Linux] {
        for arg in joined(os) {
            for banned in ["--enable-automation", "--headless", "--remote-debugging-pipe", "--disable-component-update", "--disable-default-apps",
                "--disable-extensions", "--disable-popup-blocking", "--disable-background-networking", "--no-sandbox", "--password-store=basic-x", "--remote-allow-origins"] {
                assert!(!arg.starts_with(banned), "{arg}");
            }
        }
    }
    // The webdriver switch is off, the unsupported-flag warning bar is suppressed, and a window in the
    // background is not throttled.
    for os in [TargetOs::Macos, TargetOs::Windows, TargetOs::Linux] {
        let args = joined(os);
        assert!(args.contains(&"--disable-blink-features=AutomationControlled".to_owned()) && args.contains(&"--test-type".to_owned()));
        for wanted in ["--disable-backgrounding-occluded-windows", "--disable-renderer-backgrounding", "--disable-background-timer-throttling"] {
            assert!(args.contains(&wanted.to_owned()), "{wanted}");
        }
    }
}

#[test]
fn the_keychain_prompt_of_a_fresh_profile_is_avoided_per_platform() {
    assert!(joined(TargetOs::Macos).contains(&"--use-mock-keychain".to_owned()));
    assert!(joined(TargetOs::Linux).contains(&"--password-store=basic".to_owned()));
    assert!(!joined(TargetOs::Windows).iter().any(|a| a.contains("keychain") || a.starts_with("--password-store")));
}

#[test]
fn the_debug_endpoint_is_read_from_the_profile_file_and_only_on_loopback() {
    assert_eq!(parse_devtools_active_port("9222\n/devtools/browser/abc-123\n"), Some((9222, "/devtools/browser/abc-123".to_owned())));
    for bad in ["", "0\n/devtools/browser/x\n", "70000\n/devtools/browser/x\n", "9222\n/other\n", "9222\n", "x\n/devtools/browser/x\n", "9222\n/devtools/browser/../x\n"] {
        assert_eq!(parse_devtools_active_port(bad), None, "{bad:?}");
    }
}

#[test]
fn the_version_line_of_chrome_and_brave_is_understood() {
    assert_eq!(parse_version("Google Chrome 152.0.7977.65 \n"), Some("152.0.7977.65".to_owned()));
    assert_eq!(parse_version("Brave Browser 157.1.99.55"), Some("157.1.99.55".to_owned()));
    assert_eq!(parse_version("nonsense"), None);
    assert_eq!(parse_version(""), None);
}

#[test]
fn a_new_profile_asks_for_spanish_pages_and_an_existing_one_is_left_as_it_is() {
    use ceh_browser::chromium::seed_preferences;
    let dir = tempfile::tempdir().unwrap();
    seed_preferences(dir.path()).unwrap();
    let written = std::fs::read_to_string(dir.path().join("Default/Preferences")).unwrap();
    let value: serde_json::Value = serde_json::from_str(&written).unwrap();
    assert_eq!(value["intl"]["accept_languages"], "es-ES,es");
    std::fs::write(dir.path().join("Default/Preferences"), "{\"kept\":true}").unwrap();
    seed_preferences(dir.path()).unwrap();
    assert_eq!(std::fs::read_to_string(dir.path().join("Default/Preferences")).unwrap(), "{\"kept\":true}");
}

#[test]
fn the_windows_version_comes_from_the_numbered_folder_beside_the_executable() {
    // chrome.exe / msedge.exe print nothing for --version on Windows; their version is the folder name next to them.
    let application = tempfile::tempdir().unwrap();
    for name in ["156.0.7501.3", "157.0.4341.46", "Installer", "SetupMetrics", "99.1"] {
        std::fs::create_dir(application.path().join(name)).unwrap();
    }
    std::fs::write(application.path().join("msedge.exe"), "").unwrap();
    std::fs::write(application.path().join("200.0.0.1"), "a file is not a version folder").unwrap();
    assert_eq!(ceh_browser::chromium::version_from_install_folder(application.path()), Some("157.0.4341.46".to_owned()));
    let empty = tempfile::tempdir().unwrap();
    assert_eq!(ceh_browser::chromium::version_from_install_folder(empty.path()), None);
    // Numeric comparison, not text: 99.x is older than 157.x.
    let numbers = tempfile::tempdir().unwrap();
    for name in ["99.0.1.1", "157.0.1.1", "1000.0.0.1"] {
        std::fs::create_dir(numbers.path().join(name)).unwrap();
    }
    assert_eq!(ceh_browser::chromium::version_from_install_folder(numbers.path()), Some("1000.0.0.1".to_owned()));
}

mod locations {
    use ceh_browser::chromium::{app_path_from_value, candidate_paths, registry_path_fits};
    use ceh_browser::discovery::is_snap_install;
    use ceh_browser::firefox::{ProfileKind, ProfileLease};
    use ceh_browser::launch::TargetOs;
    use std::path::{Path, PathBuf};

    fn has(paths: &[PathBuf], expected: &str) -> bool {
        paths.iter().any(|p| p == Path::new(expected))
    }

    #[test]
    fn linux_looks_at_the_real_program_and_at_the_usual_links() {
        let chrome = candidate_paths(ProfileKind::Chrome, TargetOs::Linux, &[]);
        assert!(has(&chrome, "/usr/bin/google-chrome") && has(&chrome, "/usr/bin/google-chrome-stable") && has(&chrome, "/opt/google/chrome/chrome"));
        let brave = candidate_paths(ProfileKind::Brave, TargetOs::Linux, &[]);
        assert!(has(&brave, "/usr/bin/brave-browser") && has(&brave, "/opt/brave.com/brave/brave-browser"));
        let edge = candidate_paths(ProfileKind::Edge, TargetOs::Linux, &[]);
        assert!(has(&edge, "/usr/bin/microsoft-edge") && has(&edge, "/opt/microsoft/msedge/microsoft-edge"));
        let chromium = candidate_paths(ProfileKind::Chromium, TargetOs::Linux, &[]);
        for expected in ["/usr/lib/chromium/chromium", "/usr/lib/chromium-browser/chromium-browser", "/usr/bin/chromium", "/usr/bin/chromium-browser"] {
            assert!(has(&chromium, expected), "{expected}");
        }
        assert!(chromium.iter().all(|p| p.is_absolute() && !p.starts_with("/snap")), "never a snap path");
    }

    #[test]
    fn windows_and_macos_use_their_install_folders_including_chromium() {
        let bases = [PathBuf::from("C:/Program Files"), PathBuf::from("C:/Users/x/AppData/Local")];
        let chromium = candidate_paths(ProfileKind::Chromium, TargetOs::Windows, &bases);
        assert!(has(&chromium, "C:/Users/x/AppData/Local/Chromium/Application/chrome.exe") && has(&chromium, "C:/Program Files/Chromium/Application/chrome.exe"));
        let chrome = candidate_paths(ProfileKind::Chrome, TargetOs::Windows, &bases);
        assert!(has(&chrome, "C:/Users/x/AppData/Local/Google/Chrome/Application/chrome.exe"), "a per-user Chrome");
        assert!(has(&candidate_paths(ProfileKind::Chromium, TargetOs::Macos, &[]), "/Applications/Chromium.app/Contents/MacOS/Chromium"));
    }

    #[test]
    fn the_windows_registry_value_is_a_path_possibly_quoted_and_nothing_else() {
        assert_eq!(app_path_from_value("C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe"), Some(PathBuf::from("C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe")));
        assert_eq!(app_path_from_value("\"D:\\Apps\\Brave\\brave.exe\""), Some(PathBuf::from("D:\\Apps\\Brave\\brave.exe")));
        assert_eq!(app_path_from_value("  \"D:\\x\\msedge.exe\"  "), Some(PathBuf::from("D:\\x\\msedge.exe")));
        for bad in ["", "chrome.exe", "..\\chrome.exe", "C:\\x\\..\\chrome.exe", "C:\\x\\readme.txt", "\"C:\\x\\chrome.exe --evil\"", "C:\\x\\chrome.exe --remote-debugging-port=1"] {
            assert_eq!(app_path_from_value(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_registry_path_is_trusted_only_for_the_browser_it_belongs_to() {
        // Chrome and Chromium both register chrome.exe: each kind takes only its own folder.
        let google = Path::new("C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe");
        let chromium = Path::new("C:\\Users\\x\\AppData\\Local\\Chromium\\Application\\chrome.exe");
        assert!(registry_path_fits(ProfileKind::Chrome, google) && !registry_path_fits(ProfileKind::Chrome, chromium));
        assert!(registry_path_fits(ProfileKind::Chromium, chromium) && !registry_path_fits(ProfileKind::Chromium, google));
        assert!(registry_path_fits(ProfileKind::Brave, Path::new("D:\\BraveSoftware\\Brave-Browser\\Application\\brave.exe")));
        assert!(registry_path_fits(ProfileKind::Edge, Path::new("C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe")));
        assert!(!registry_path_fits(ProfileKind::Edge, google));
    }

    #[test]
    fn a_snap_package_or_its_wrapper_is_recognised_so_it_is_never_started() {
        assert!(is_snap_install(Path::new("/snap/chromium/current/usr/lib/chromium-browser/chrome")));
        let dir = tempfile::tempdir().unwrap();
        let wrapper = dir.path().join("chromium-browser");
        std::fs::write(&wrapper, "#!/bin/sh\n# Ubuntu: the deb is a stub for the snap\nexec /snap/bin/chromium \"$@\"\n").unwrap();
        assert!(is_snap_install(&wrapper), "Ubuntu's chromium-browser is a snap launcher script");
        let real = dir.path().join("chromium");
        std::fs::write(&real, vec![0x7f, b'E', b'L', b'F', 0, 1, 2, 3]).unwrap();
        assert!(!is_snap_install(&real), "a real program");
        let plain_script = dir.path().join("plain");
        std::fs::write(&plain_script, "#!/bin/sh\nexec /usr/lib/chromium/chromium \"$@\"\n").unwrap();
        assert!(!is_snap_install(&plain_script), "Debian's launcher script is fine");
    }

    #[test]
    fn chromium_has_its_own_profile_that_no_other_browser_opens() {
        let runtime = tempfile::tempdir().unwrap();
        let chromium = ProfileLease::persistent_kind(runtime.path(), "p1", ProfileKind::Chromium).unwrap();
        assert!(chromium.path().ends_with("chromium-p1"));
        drop(chromium);
        let marker = std::fs::read(runtime.path().join("browser-profiles/chromium-p1/ceh-owned-profile")).unwrap();
        assert_eq!(marker, b"CEH Chromium profile v1\n");
        // The same folder name is refused for another browser kind.
        assert!(ProfileLease::persistent_kind(runtime.path(), "p1", ProfileKind::Chrome).map(|l| l.path().to_owned()).map_or(true, |p| !p.ends_with("chromium-p1")));
    }
}
