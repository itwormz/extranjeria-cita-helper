use ceh_core::profiles::ProfileCatalog;
use ceh_store::profiles::*;
use serde_json::{Value, json};
#[test]
fn read_only_profiles_match_existing_desktop_store() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../contracts/fixtures/legacy-profiles.json"
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
        let mut expected=case["expected"].clone();
        // The Python reference predates the explicitly opt-in beta setting.
        if let Some(prefs)=expected.get_mut("preferences").and_then(Value::as_object_mut){prefs.insert("beta_updates".into(),json!(false));}
        assert_eq!(actual, expected, "{}", case["id"]);
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

fn loaded_with_protection(protection: &str) -> LoadedProfile {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../contracts/fixtures/legacy-profiles.json"
    ))
    .unwrap();
    let catalog: ProfileCatalog = serde_json::from_value(fixture["catalog"].clone()).unwrap();
    let source = format!(
        "tasks:\n- id: fixture\n  label: Fixture\n  request:\n    province: Fixture Province\n    procedure: POLICÍA-TOMA DE HUELLAS (EXPEDICIÓN DE TARJETA) INICIAL, RENOVACIÓN, DUPLICADO Y LEY 14/2013\n    access_mode: certificate\n{protection}  applicant:\n    document_type: NIE\n    document_number: X0000000T\n    full_name: FICTITIOUS PERSON\n    nationality: FIXTURE NATIONALITY\n    contact:\n      phone: '600000000'\n      email: fixture@example.test\n"
    );
    decode_profiles(source.as_bytes(), &catalog, "en")
        .unwrap()
        .profiles
        .into_iter()
        .next()
        .unwrap()
}

#[test]
fn existing_profile_without_distribute_attempts_defaults_to_false() {
    let loaded = loaded_with_protection("");
    assert!(!loaded.appointment_policy.unwrap().distribute_attempts);
    let loaded = loaded_with_protection("    appointment_protection:\n      daily_target: 5\n");
    assert!(!loaded.appointment_policy.unwrap().distribute_attempts);
}

#[test]
fn distribute_attempts_true_is_read() {
    let loaded = loaded_with_protection("    appointment_protection:\n      distribute_attempts: true\n");
    assert!(loaded.appointment_policy.unwrap().distribute_attempts);
}

#[test]
fn non_boolean_distribute_attempts_is_rejected() {
    let loaded = loaded_with_protection("    appointment_protection:\n      distribute_attempts: 'yes'\n");
    assert!(loaded.appointment_policy.is_none());
    assert!(loaded.validation.errors.iter().any(|e| e == "Invalid appointment protection settings"));
}

#[test]
fn python_beta_opt_in_is_loaded_strictly_and_survives_saving(){
    let catalog=ProfileCatalog{provinces:vec![],nationalities:vec![],barcelona_offices:vec![],timezones:vec!["Europe/Madrid".into()]};
    for (value,expected) in [("true",true),("false",false),("'false'",false),("1",false),("null",false)]{
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("tasks.yml");
        std::fs::write(&path,format!("tasks: []\ndesktop: {{beta_updates: {value}}}\n")).unwrap();
        let prefs=read_profiles(&path,&catalog,"en").unwrap().preferences;
        assert_eq!(prefs.beta_updates,expected);
        ceh_store::profile_write::save_preferences(&path,&prefs,&catalog).unwrap();
        assert_eq!(read_profiles(&path,&catalog,"en").unwrap().preferences.beta_updates,expected);
    }
}
