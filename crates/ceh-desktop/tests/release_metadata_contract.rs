//! One version for the whole release: the workspace version is the single source; the web UI package must
//! agree and the Tauri config must not carry a second copy. Release tags are `v{version}`.
use std::{fs, path::PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn workspace_version() -> String {
    let manifest = fs::read_to_string(root().join("Cargo.toml")).unwrap();
    let line = manifest.lines().find(|l| l.starts_with("version = ")).expect("workspace version");
    line.split('"').nth(1).unwrap().to_owned()
}

#[test]
fn the_version_is_a_release_version_not_a_development_placeholder() {
    let version = workspace_version();
    let (core, pre) = version.split_once('-').map_or((version.as_str(), ""), |(c, p)| (c, p));
    assert!(core.split('.').count() == 3 && core.split('.').all(|p| p.parse::<u32>().is_ok()), "{version}");
    assert!(pre.is_empty() || (pre.starts_with("beta.") && pre[5..].parse::<u32>().is_ok()), "{version}");
    assert!(!version.contains("dev"), "{version}");
}

#[test]
fn the_web_ui_package_and_the_tauri_config_agree_with_the_workspace_version() {
    let version = workspace_version();
    let package: serde_json::Value = serde_json::from_str(&fs::read_to_string(root().join("desktop-ui/package.json")).unwrap()).unwrap();
    assert_eq!(package["version"], version.as_str());
    let tauri: serde_json::Value = serde_json::from_str(&fs::read_to_string(root().join("crates/ceh-desktop/tauri.conf.json")).unwrap()).unwrap();
    assert!(tauri.get("version").is_none(), "the Tauri config takes the version from Cargo.toml");
}
