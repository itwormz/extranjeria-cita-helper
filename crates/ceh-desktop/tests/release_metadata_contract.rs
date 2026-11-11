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

#[test]
fn the_window_and_dock_icon_is_the_largest_png_not_a_small_one() {
    let tauri: serde_json::Value = serde_json::from_str(&fs::read_to_string(root().join("crates/ceh-desktop/tauri.conf.json")).unwrap()).unwrap();
    let icons = tauri["bundle"]["icon"].as_array().unwrap();
    // Tauri embeds the first PNG of the list as the runtime window icon, which the macOS Dock then scales.
    let first_png = icons.iter().filter_map(|i| i.as_str()).find(|i| i.ends_with(".png")).unwrap();
    assert!(first_png.ends_with("/icon.png"), "{first_png}");
    assert!(icons.iter().any(|i| i.as_str().is_some_and(|i| i.ends_with(".icns"))) && icons.iter().any(|i| i.as_str().is_some_and(|i| i.ends_with(".ico"))));
}

#[test]
fn the_installed_application_keeps_its_data_beside_the_python_folder_never_inside_the_source_tree() {
    use ceh_desktop::application_data_root;
    let support = std::path::Path::new("/support");
    assert_eq!(application_data_root(support), support.join("SantaCita"));
    // It must never be the folder the Python version used, or an import would copy a folder onto itself.
    assert_ne!(application_data_root(support), support.join("Cita Extranjeria Helper"));
    let found = ceh_desktop::installed_data_root().expect("a data folder on this system");
    assert!(found.ends_with("SantaCita") && !found.to_string_lossy().contains(".local/builds"), "{found:?}");
}

#[test]
fn the_macos_icons_keep_the_system_margin_while_other_systems_use_the_full_frame() {
    let read = |path: &str| -> serde_json::Value { serde_json::from_str(&fs::read_to_string(root().join(path)).unwrap()).unwrap() };
    let mac = read("crates/ceh-desktop/tauri.macos.conf.json");
    let icons: Vec<&str> = mac["bundle"]["icon"].as_array().unwrap().iter().filter_map(|i| i.as_str()).collect();
    assert!(icons.iter().all(|i| i.contains("rust-ui-icons-macos/")), "{icons:?}");
    assert!(icons.iter().find(|i| i.ends_with(".png")).unwrap().ends_with("/icon.png"));
    assert!(icons.iter().any(|i| i.ends_with(".icns")));
    // The artwork there is 824 of 1024 px on a transparent canvas (the macOS icon grid).
    let svg = fs::read_to_string(root().join("assets/santacita-icon-macos.svg")).unwrap();
    assert!(svg.contains("viewBox=\"0 0 1024 1024\"") && svg.contains("translate(100 100) scale(1.609375)"));
    // The shared configuration still names the full-frame icons.
    let shared = read("crates/ceh-desktop/tauri.conf.json");
    assert!(shared["bundle"]["icon"].as_array().unwrap().iter().all(|i| !i.as_str().unwrap().contains("-macos")));
}
