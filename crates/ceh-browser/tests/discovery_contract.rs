use ceh_browser::discovery::{BrowserFamily, Support, discover_browsers};
use std::path::Path;

fn app(root: &Path, name: &str, executable: &str) -> std::path::PathBuf {
    let contents = root.join(format!("{name}.app/Contents"));
    std::fs::create_dir_all(contents.join("MacOS")).unwrap();
    std::fs::create_dir_all(contents.join("Resources")).unwrap();
    std::fs::write(contents.join("MacOS").join(executable), "").unwrap();
    contents
}
fn plist(contents: &Path, version: &str) {
    std::fs::write(
        contents.join("Info.plist"),
        format!("<?xml version=\"1.0\"?><plist version=\"1.0\"><dict><key>CFBundleShortVersionString</key><string>{version}</string></dict></plist>"),
    )
    .unwrap();
}

#[test]
fn only_firefox_is_supported_and_others_are_listed_with_their_status() {
    let root = tempfile::tempdir().unwrap();
    let firefox = app(root.path(), "Firefox", "firefox");
    std::fs::write(firefox.join("Resources/application.ini"), "[App]\nName=Firefox\nVersion=158.0.1\n").unwrap();
    plist(&app(root.path(), "Google Chrome", "Google Chrome"), "140.0.1");
    plist(&app(root.path(), "Brave Browser", "Brave Browser"), "1.80");
    plist(&app(root.path(), "Microsoft Edge", "Microsoft Edge"), "140.0");
    app(root.path(), "Safari", "Safari");
    let found = discover_browsers(&[root.path().to_owned()]);
    let summary: Vec<_> = found.iter().map(|b| (b.family, b.version.as_str(), b.support)).collect();
    assert_eq!(summary, vec![
        (BrowserFamily::Firefox, "158.0.1", Support::Supported),
        (BrowserFamily::Chrome, "140.0.1", Support::Supported),
        (BrowserFamily::Brave, "1.80", Support::Supported),
        (BrowserFamily::Edge, "140.0", Support::Planned),
        (BrowserFamily::Safari, "", Support::Unsupported("no_input_automation")),
    ]);
}

#[test]
fn invalid_or_foreign_applications_are_ignored() {
    let root = tempfile::tempdir().unwrap();
    // Firefox bundle whose ini names another product, a bundle without an executable, an unrelated app.
    let fake = app(root.path(), "Firefox", "firefox");
    std::fs::write(fake.join("Resources/application.ini"), "[App]\nName=Other\nVersion=1.0\n").unwrap();
    std::fs::create_dir_all(root.path().join("Google Chrome.app/Contents/MacOS")).unwrap();
    app(root.path(), "Slack", "Slack");
    std::fs::write(root.path().join("Brave Browser.app"), "not a directory").unwrap();
    assert!(discover_browsers(&[root.path().to_owned()]).is_empty());
}

#[test]
fn duplicate_installations_across_roots_collapse_and_the_first_root_wins() {
    let system = tempfile::tempdir().unwrap();
    let user = tempfile::tempdir().unwrap();
    for (root, version) in [(system.path(), "158.0"), (user.path(), "159.0")] {
        let contents = app(root, "Firefox", "firefox");
        std::fs::write(contents.join("Resources/application.ini"), format!("[App]\nName=Firefox\nVersion={version}\n")).unwrap();
    }
    let found = discover_browsers(&[system.path().to_owned(), user.path().to_owned()]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].version, "158.0");
}

#[test]
fn oversized_or_binary_plists_do_not_break_discovery() {
    let root = tempfile::tempdir().unwrap();
    let chrome = app(root.path(), "Google Chrome", "Google Chrome");
    std::fs::write(chrome.join("Info.plist"), vec![b'x'; 2 * 1024 * 1024]).unwrap();
    let brave = app(root.path(), "Brave Browser", "Brave Browser");
    std::fs::write(brave.join("Info.plist"), b"bplist00\x00\x01").unwrap();
    let found = discover_browsers(&[root.path().to_owned()]);
    assert_eq!(found.len(), 2);
    assert!(found.iter().all(|b| b.version.is_empty() && b.support == Support::Supported));
}

#[test]
fn stable_ids_are_what_the_window_receives() {
    assert_eq!([BrowserFamily::Firefox.id(), BrowserFamily::Edge.id()], ["firefox", "edge"]);
    assert_eq!([Support::Supported.id(), Support::Planned.id(), Support::Unsupported("x").id()], ["supported", "planned", "unsupported"]);
}
