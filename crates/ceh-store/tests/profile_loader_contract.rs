use ceh_core::profiles::ProfileCatalog;
use ceh_store::profiles::*;
use serde_json::{Value, json};
#[test]
fn read_only_profiles_match_existing_desktop_store() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/fixtures/legacy-profiles.json"
    ))
    .unwrap();
    let catalog: ProfileCatalog = serde_json::from_value(fixture["catalog"].clone()).unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 64);
    for case in fixture["cases"].as_array().unwrap() {
        let source = case["source"].as_str().unwrap().as_bytes();
        let before = source.to_vec();
        let actual = match decode_profiles(
            source,
            &catalog,
            case["system_language"].as_str().unwrap_or("en"),
        ) {
            Ok(bundle) => {
                json!({"profiles":bundle.profiles.iter().map(|p|json!({"normalized":p.validation.normalized,"valid":p.validation.errors.is_empty(),"has_notifications":p.has_notifications})).collect::<Vec<_>>(),"preferences":bundle.preferences,"last_profile_id":bundle.last_profile_id,"revision":bundle.revision(),"unchanged":source==before})
            }
            Err(error) => json!({"error":error.code(),"unchanged":source==before}),
        };
        assert_eq!(actual, case["expected"], "{}", case["id"]);
    }
}
#[test]
fn loading_a_chosen_file_does_not_rewrite_it_or_touch_secret_store() {
    use std::{fs, path::PathBuf};
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/builds/work/rust-profile-read-only-test");
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("{}-profiles.yml", std::process::id()));
    let source = b"tasks: []\nunknown: keep-me\ndesktop: {last_profile_id: old}\n";
    fs::write(&path, source).unwrap();
    let catalog = ProfileCatalog {
        provinces: vec![],
        nationalities: vec![],
        barcelona_offices: vec![],
        timezones: vec!["Europe/Madrid".into()],
    };
    let bundle = read_profiles(&path, &catalog, "en").unwrap();
    assert_eq!(bundle.last_profile_id.as_deref(), Some("old"));
    assert_eq!(fs::read(&path).unwrap(), source);
    assert!(bundle.raw().root().get("unknown").is_some());
    fs::remove_file(&path).unwrap();
}

#[test]
fn missing_file_is_a_clean_first_start_without_creating_it() {
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/builds/work/rust-profile-read-only-test");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("missing-{}.yml", std::process::id()));
    assert!(!path.exists());
    let catalog = ProfileCatalog {
        provinces: vec![],
        nationalities: vec![],
        barcelona_offices: vec![],
        timezones: vec!["Europe/Madrid".into()],
    };
    let bundle = read_profiles(&path, &catalog, "es").unwrap();
    assert!(bundle.profiles.is_empty());
    assert_eq!(bundle.preferences.language, "es");
    assert_eq!(
        bundle.revision(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert!(!path.exists());
}
