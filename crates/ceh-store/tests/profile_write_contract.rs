use ceh_core::profiles::{ProfileCatalog, ProfileDraft};
use ceh_store::{
    profile_write::{ProfileWriteError, save_profile},
    profiles::decode_profiles,
};
fn catalog() -> ProfileCatalog {
    ProfileCatalog {
        provinces: vec!["Fixture Province".into()],
        nationalities: vec!["FIXTURE NATIONALITY".into()],
        barcelona_offices: vec![],
        timezones: vec!["Europe/Madrid".into()],
    }
}
fn source() -> String {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/fixtures/legacy-profiles.json"
    ))
    .unwrap();
    corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "huellas_defaults")
        .unwrap()["source"]
        .as_str()
        .unwrap()
        .to_string()
}
fn draft(source: &str) -> ProfileDraft {
    decode_profiles(source.as_bytes(), &catalog(), "ru")
        .unwrap()
        .profiles[0]
        .validation
        .normalized
        .clone()
}
#[test]
fn save_preserves_unknown_fields_and_old_revision_cannot_overwrite() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("tasks.yml");
    let source = source()
        + "vendor: {switch: true, timestamp: 2026-10-30, count: 012, extra: [a, b]}\ndesktop: {language: ru, last_profile_id: fixture}\n";
    std::fs::write(&path, &source).unwrap();
    let bundle = decode_profiles(source.as_bytes(), &catalog(), "ru").unwrap();
    let revision = bundle.revision().to_owned();
    let mut edited = draft(&source);
    edited.label = "Updated".into();
    let result = save_profile(
        &path,
        Some("fixture"),
        edited.clone(),
        &revision,
        &catalog(),
    )
    .unwrap();
    assert_eq!(result.id, "fixture");
    let next = ceh_store::legacy_yaml::read_document(&path).unwrap();
    let vendor = next.root().get("vendor").unwrap();
    assert_eq!(vendor.get("count").unwrap().scalar_text().unwrap(), "10");
    assert_eq!(
        vendor.get("timestamp").unwrap().scalar_text().unwrap(),
        "2026-10-30"
    );
    assert_eq!(vendor.get("switch").unwrap().scalar_text().unwrap(), "True");
    assert_eq!(
        decode_profiles(&std::fs::read(&path).unwrap(), &catalog(), "ru")
            .unwrap()
            .profiles[0]
            .validation
            .normalized
            .label,
        "Updated"
    );
    assert!(matches!(
        save_profile(&path, Some("fixture"), edited, &revision, &catalog()),
        Err(ProfileWriteError::Conflict)
    ));
}
#[test]
fn invalid_changes_do_not_rewrite_profile() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("tasks.yml");
    let source = source();
    std::fs::write(&path, &source).unwrap();
    let mut edited = draft(&source);
    edited.phone = "invalid".into();
    let bundle = decode_profiles(source.as_bytes(), &catalog(), "ru").unwrap();
    assert!(matches!(
        save_profile(
            &path,
            Some("fixture"),
            edited,
            bundle.revision(),
            &catalog()
        ),
        Err(ProfileWriteError::Validation(_))
    ));
    assert_eq!(std::fs::read_to_string(path).unwrap(), source);
}
#[test]
fn unknown_numeric_mapping_keys_keep_their_yaml_type() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("tasks.yml");
    let source = source() + "vendor: {1.0e-7: tiny, .inf: infinity}\n";
    std::fs::write(&path, &source).unwrap();
    let old = ceh_store::legacy_yaml::read_document(&path).unwrap();
    let ceh_store::legacy_yaml::RawValue::Mapping(original) = old.root().get("vendor").unwrap()
    else {
        panic!()
    };
    let keys: Vec<_> = original.keys().cloned().collect();
    let revision = old.revision().to_owned();
    save_profile(
        &path,
        Some("fixture"),
        draft(&source),
        &revision,
        &catalog(),
    )
    .unwrap();
    let saved = ceh_store::legacy_yaml::read_document(&path).unwrap();
    let ceh_store::legacy_yaml::RawValue::Mapping(current) = saved.root().get("vendor").unwrap()
    else {
        panic!()
    };
    assert_eq!(current.keys().cloned().collect::<Vec<_>>(), keys);
}
#[test]
fn new_profile_uses_generated_id_and_preserves_vault_reference_on_plain_edit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("tasks.yml");
    let empty = decode_profiles(&[], &catalog(), "ru").unwrap();
    let result = save_profile(&path, None, draft(&source()), empty.revision(), &catalog()).unwrap();
    assert!(result.id.starts_with("person_"));
    let mut source = std::fs::read_to_string(&path).unwrap();
    source = source.trim_end().to_string();
    let first = source.find("\"enabled\"").unwrap();
    source.insert_str(first, "\"notifications_ref\": \"notifications_fixture\", ");
    std::fs::write(&path, &source).unwrap();
    let loaded = decode_profiles(source.as_bytes(), &catalog(), "ru").unwrap();
    let mut edited = loaded.profiles[0].validation.normalized.clone();
    edited.label = "Renamed".into();
    save_profile(
        &path,
        Some(&result.id),
        edited,
        loaded.revision(),
        &catalog(),
    )
    .unwrap();
    let raw = ceh_store::legacy_yaml::read_document(&path).unwrap();
    assert_eq!(
        raw.root().get("tasks").unwrap().sequence().unwrap()[0]
            .get("notifications_ref")
            .unwrap()
            .scalar_text()
            .unwrap(),
        "notifications_fixture"
    );
}
#[test]
fn selecting_profile_and_preferences_preserve_profile_data() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("tasks.yml");
    std::fs::write(&path, source()).unwrap();
    ceh_store::profile_write::save_selection(&path, "fixture", &catalog()).unwrap();
    let current = ceh_store::profiles::read_profiles(&path, &catalog(), "ru").unwrap();
    let mut preferences = current.preferences;
    preferences.dark = true;
    preferences.language = "es".into();
    ceh_store::profile_write::save_preferences(&path, &preferences, &catalog()).unwrap();
    let loaded = ceh_store::profiles::read_profiles(&path, &catalog(), "ru").unwrap();
    assert_eq!(loaded.last_profile_id.as_deref(), Some("fixture"));
    assert!(loaded.preferences.dark);
    assert_eq!(loaded.preferences.language, "es");
    assert_eq!(
        loaded.profiles[0].validation.normalized.document_number,
        "X0000000T"
    );
    let before = std::fs::read(&path).unwrap();
    assert!(matches!(
        ceh_store::profile_write::save_selection(&path, "missing", &catalog()),
        Err(ProfileWriteError::Conflict)
    ));
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn timing_patch_is_validated_atomically_and_preserves_unknown_overrides() {
    use ceh_store::profile_write::save_profile_with_timings;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("tasks.yml");
    let original = source().replacen(
        "label:",
        "timings: {reload_seconds: [41, 73], vendor_pause: 99}\n  label:",
        1,
    );
    std::fs::write(&path, &original).unwrap();
    let loaded = ceh_store::profiles::read_profiles(&path, &catalog(), "ru").unwrap();
    let invalid = serde_json::json!({"reload_seconds": [1, 2]});
    assert!(matches!(
        save_profile_with_timings(
            &path,
            Some("fixture"),
            draft(&original),
            loaded.revision(),
            &catalog(),
            Some(&invalid)
        ),
        Err(ProfileWriteError::Validation(_))
    ));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    let patch = serde_json::json!({"reload_seconds": [45, 90]});
    save_profile_with_timings(
        &path,
        Some("fixture"),
        draft(&original),
        loaded.revision(),
        &catalog(),
        Some(&patch),
    )
    .unwrap();
    let next = ceh_store::profiles::read_profiles(&path, &catalog(), "ru").unwrap();
    assert_eq!(
        next.profiles[0].timings.as_ref().unwrap().reload_seconds(),
        [45., 90.]
    );
    let raw = &next.raw().root().get("tasks").unwrap().sequence().unwrap()[0];
    assert_eq!(
        raw.get("timings")
            .unwrap()
            .get("vendor_pause")
            .unwrap()
            .scalar_text()
            .unwrap(),
        "99"
    );
    let rev = next.revision().to_owned();
    let mut rename = draft(&std::fs::read_to_string(&path).unwrap());
    rename.label = "Renamed".into();
    save_profile(&path, Some("fixture"), rename, &rev, &catalog()).unwrap();
    let latest = ceh_store::profiles::read_profiles(&path, &catalog(), "ru").unwrap();
    assert_eq!(
        latest.profiles[0]
            .timings
            .as_ref()
            .unwrap()
            .reload_seconds(),
        [45., 90.]
    );
}

#[test]
fn per_profile_browser_defaults_to_firefox_roundtrips_and_rejects_unsupported_choices() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("tasks.yml");
    let original = source();
    std::fs::write(&path, &original).unwrap();
    let bundle = decode_profiles(original.as_bytes(), &catalog(), "ru").unwrap();
    assert_eq!(bundle.profiles[0].validation.normalized.browser, "firefox");
    let edited = draft(&original);
    let saved = save_profile(
        &path,
        Some("fixture"),
        edited,
        bundle.revision(),
        &catalog(),
    )
    .unwrap();
    let read = ceh_store::profiles::read_profiles(&path, &catalog(), "ru").unwrap();
    assert_eq!(read.profiles[0].validation.normalized.browser, "firefox");
    assert_eq!(
        read.raw().root().get("tasks").unwrap().sequence().unwrap()[0]
            .get("request")
            .unwrap()
            .get("browser")
            .unwrap()
            .scalar_text()
            .unwrap(),
        "firefox"
    );
    let before = std::fs::read(&path).unwrap();
    let mut invalid = read.profiles[0].validation.normalized.clone();
    invalid.browser = "other".into();
    assert!(matches!(
        save_profile(&path, Some("fixture"), invalid, &saved.revision, &catalog()),
        Err(ProfileWriteError::Validation(_))
    ));
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn distribute_attempts_survives_save_and_reload() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("tasks.yml");
    let source = source().replace(
        "    access_mode: certificate\n",
        "    access_mode: certificate\n    appointment_protection:\n      distribute_attempts: true\n",
    );
    assert!(source.contains("distribute_attempts: true"));
    std::fs::write(&path, &source).unwrap();
    let revision = decode_profiles(source.as_bytes(), &catalog(), "ru").unwrap().revision().to_owned();
    let mut edited = draft(&source);
    edited.label = "Renamed".into();
    save_profile(&path, Some("fixture"), edited, &revision, &catalog()).unwrap();
    let reloaded = decode_profiles(&std::fs::read(&path).unwrap(), &catalog(), "ru").unwrap();
    assert!(reloaded.profiles[0].appointment_policy.as_ref().unwrap().distribute_attempts);
}

#[test]
fn adaptive_reload_on_429_roundtrips_through_timing_patch_and_rejects_non_bool() {
    use ceh_store::profile_write::save_profile_with_timings;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("tasks.yml");
    let original = source();
    std::fs::write(&path, &original).unwrap();
    let revision = ceh_store::profiles::read_profiles(&path, &catalog(), "ru").unwrap().revision().to_owned();
    let invalid = serde_json::json!({"adaptive_reload_on_429": "yes"});
    assert!(matches!(
        save_profile_with_timings(&path, Some("fixture"), draft(&original), &revision, &catalog(), Some(&invalid)),
        Err(ProfileWriteError::Validation(_))
    ));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    let patch = serde_json::json!({"adaptive_reload_on_429": false});
    save_profile_with_timings(&path, Some("fixture"), draft(&original), &revision, &catalog(), Some(&patch)).unwrap();
    let next = ceh_store::profiles::read_profiles(&path, &catalog(), "ru").unwrap();
    assert!(!next.profiles[0].timings.as_ref().unwrap().adaptive_reload_on_429());
}

mod protection {
    use super::*;
    use ceh_core::appointment_budget::AppointmentPolicy;
    use ceh_store::profile_write::{ProfileEdits, save_profile_changes};

    fn with_block(block: &str) -> String {
        source().replace(
            "    access_mode: certificate\n",
            &format!("    access_mode: certificate\n    appointment_protection:\n{block}"),
        )
    }

    #[test]
    fn protection_is_written_and_unknown_policy_keys_survive() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tasks.yml");
        let original = with_block("      vendor_flag: 7\n      distribute_attempts: false\n");
        std::fs::write(&path, &original).unwrap();
        let revision = decode_profiles(original.as_bytes(), &catalog(), "ru").unwrap().revision().to_owned();
        let policy = AppointmentPolicy { daily_target: 9, distribute_attempts: true, minimum_pause_minutes: 20, ..Default::default() };
        save_profile_changes(&path, Some("fixture"), draft(&original), &revision, &catalog(),
            ProfileEdits { protection: Some(&policy), ..Default::default() }).unwrap();
        let reloaded = decode_profiles(&std::fs::read(&path).unwrap(), &catalog(), "ru").unwrap();
        // The minimum pause between attempts is gone (owner decision): whatever is stored, it loads as no pause.
        assert_eq!(reloaded.profiles[0].appointment_policy.as_ref().unwrap(), &policy.clone().without_minimum_pause());
        let document = ceh_store::legacy_yaml::read_document(&path).unwrap();
        let block = document.root().get("tasks").unwrap().sequence().unwrap()[0].get("request").unwrap().get("appointment_protection").unwrap().clone();
        assert_eq!(block.get("vendor_flag").unwrap().scalar_text().unwrap(), "7", "unknown policy keys are kept");
    }

    #[test]
    fn invalid_protection_does_not_rewrite_the_profile() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tasks.yml");
        let original = with_block("      distribute_attempts: true\n");
        std::fs::write(&path, &original).unwrap();
        let revision = decode_profiles(original.as_bytes(), &catalog(), "ru").unwrap().revision().to_owned();
        for bad in [
            AppointmentPolicy { daily_target: 0, ..Default::default() },
        ] {
            let result = save_profile_changes(&path, Some("fixture"), draft(&original), &revision, &catalog(),
                ProfileEdits { protection: Some(&bad), ..Default::default() });
            assert!(matches!(result, Err(ProfileWriteError::Validation(_))), "{bad:?}");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        }
    }
}
