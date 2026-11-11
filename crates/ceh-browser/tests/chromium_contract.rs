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
    // The page-visible automation switch stays off the way Chrome itself does it.
    assert!(joined(TargetOs::Linux).contains(&"--disable-blink-features=AutomationControlled".to_owned()));
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
