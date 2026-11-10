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
