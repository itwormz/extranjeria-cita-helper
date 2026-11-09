//! Developer-only compatibility acceptance on explicit private COPIES under .local/builds.
//! Prints structural differences, never profile/appointment values. Does not access native secrets.
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};
fn differences(a: &Value, b: &Value, path: &str, output: &mut Vec<String>) {
    if a == b || output.len() >= 100 {
        return;
    }
    if let (Some(a), Some(b)) = (a.as_f64(), b.as_f64())
        && (a - b).abs() < 0.0000001
    {
        return;
    }
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            for key in a.keys().chain(b.keys()).collect::<BTreeSet<_>>() {
                differences(
                    a.get(key).unwrap_or(&Value::Null),
                    b.get(key).unwrap_or(&Value::Null),
                    &format!("{path}.{key}"),
                    output,
                )
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                output.push(format!("{path}.length"));
            }
            for (index, (a, b)) in a.iter().zip(b).enumerate() {
                differences(a, b, &format!("{path}[{index}]"), output)
            }
        }
        _ => output.push(path.to_owned()),
    }
}
fn same_raw(
    a: Option<&ceh_store::legacy_yaml::RawValue>,
    b: Option<&ceh_store::legacy_yaml::RawValue>,
) -> bool {
    use ceh_store::legacy_yaml::RawValue::*;
    match (a, b) {
        (None, None) => true,
        (Some(Null), Some(Null)) | (Some(MergeKey), Some(MergeKey)) => true,
        (Some(Bool(a)), Some(Bool(b))) => a == b,
        (Some(Integer(a)), Some(Integer(b)))
        | (Some(String(a)), Some(String(b)))
        | (Some(Timestamp(a)), Some(Timestamp(b))) => a == b,
        (Some(Float(a)), Some(Float(b))) => a.to_bits() == b.to_bits(),
        (Some(Sequence(a)), Some(Sequence(b))) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_raw(Some(a), Some(b)))
        }
        (Some(Mapping(a)), Some(Mapping(b))) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, a)| same_raw(Some(a), b.get(key).map(|v| v.as_ref())))
        }
        _ => false,
    }
}
fn profiles(root: &Path) -> Result<Value, &'static str> {
    let bundle =
        ceh_store::profiles::read_profiles(&root.join("tasks.yml"), ceh_desktop::catalog(), "en")
            .map_err(|_| "profiles_read_failed")?;
    let values:Vec<_>=bundle.profiles.iter().map(|p|json!({"normalized":p.validation.normalized,"valid":p.validation.errors.is_empty(),"has_notifications":p.has_notifications,"timings":p.timings.clone().unwrap_or_default()})).collect();
    Ok(
        json!({"profiles":values,"preferences":bundle.preferences,"last_profile_id":bundle.last_profile_id,"revision":bundle.revision()}),
    )
}
fn run() -> Result<Value, &'static str> {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/builds/runtime/legacy-compat")
        .canonicalize()
        .map_err(|_| "copy_root_missing")?;
    let at = chrono::DateTime::parse_from_rfc3339("2026-10-31T18:00:00Z")
        .map_err(|_| "clock_invalid")?
        .to_utc();
    let mut reports = serde_json::Map::new();
    for case in ["stable", "historical", "json-only", "experimental"] {
        let root = base
            .join(case)
            .join("rust")
            .canonicalize()
            .map_err(|_| "copy_case_missing")?;
        if !root.starts_with(&base) {
            return Err("copy_path_invalid");
        }
        let expected: Value = serde_json::from_slice(
            &std::fs::read(base.join(case).join("expected.json"))
                .map_err(|_| "reference_missing")?,
        )
        .map_err(|_| "reference_invalid")?;
        let mut mismatches = Vec::new();
        let original = profiles(&root)?;
        let expected_profile = json!({"profiles":expected["profiles"],"preferences":expected["preferences"],"last_profile_id":expected["last_profile_id"],"revision":expected["revision"]});
        differences(&original, &expected_profile, "profiles", &mut mismatches);
        let options = ceh_store::migrate::MigrationOptions {
            database: root.join("data/profile-statistics.sqlite3"),
            legacy_json: Some(root.join("data/profile-statistics.json")),
            backup_directory: root.join("data/migration-backups"),
        };
        let first = ceh_store::migrate::migrate_paths(&options, at.timestamp_millis())
            .map_err(|_| "migration_failed")?;
        let before = std::fs::read(&options.database).map_err(|_| "database_read_failed")?;
        let second = ceh_store::migrate::migrate_paths(&options, at.timestamp_millis())
            .map_err(|_| "second_migration_failed")?;
        if !second.already_ready
            || std::fs::read(&options.database).map_err(|_| "database_read_failed")? != before
        {
            mismatches.push("migration.idempotence".into());
        }
        let mut timeline_rows = 0usize;
        let mut office_rows = 0usize;
        let mut achievement_rows = 0usize;
        for (index, (id, data)) in expected["data"]
            .as_object()
            .ok_or("reference_invalid")?
            .iter()
            .enumerate()
        {
            let dashboard = ceh_store::statistics::dashboard_file(&options.database, id, at)
                .map_err(|_| "statistics_read_failed")?
                .ok_or("statistics_missing")?;
            differences(
                &dashboard.all,
                &data["all"],
                &format!("statistics[{index}].all"),
                &mut mismatches,
            );
            differences(
                &dashboard.today,
                &data["today"],
                &format!("statistics[{index}].today"),
                &mut mismatches,
            );
            differences(
                &serde_json::to_value(&dashboard.offices).map_err(|_| "encode_failed")?,
                &data["offices"],
                &format!("statistics[{index}].offices"),
                &mut mismatches,
            );
            office_rows += dashboard.offices.len();
            let offices =
                ceh_store::statistics::appointment_offices_file(&options.database, id, at)
                    .map_err(|_| "offices_read_failed")?
                    .ok_or("statistics_missing")?;
            differences(
                &json!(offices.0),
                &data["known"],
                &format!("statistics[{index}].known"),
                &mut mismatches,
            );
            differences(
                &json!(offices.1),
                &data["last"],
                &format!("statistics[{index}].last"),
                &mut mismatches,
            );
            let history =
                ceh_store::statistics::history_file(&options.database, id, at, None, None)
                    .map_err(|_| "history_read_failed")?
                    .ok_or("statistics_missing")?;
            differences(
                &history.bounds,
                &data["bounds"],
                &format!("statistics[{index}].bounds"),
                &mut mismatches,
            );
            differences(
                &history.timeline,
                &data["timeline"],
                &format!("statistics[{index}].timeline"),
                &mut mismatches,
            );
            timeline_rows += history.timeline.as_array().map_or(0, Vec::len);
            let records =
                ceh_services::exports::AchievementStore::new(&root.join("data/achievements.json"))
                    .list(id)
                    .map_err(|_| "achievements_read_failed")?;
            achievement_rows += records.len();
            differences(
                &json!(records),
                &data["achievements"],
                &format!("statistics[{index}].achievements"),
                &mut mismatches,
            );
        }
        let bundle = ceh_store::profiles::read_profiles(
            &root.join("tasks.yml"),
            ceh_desktop::catalog(),
            "en",
        )
        .map_err(|_| "profiles_read_failed")?;
        let mut saved = 0;
        let roundtrip = tempfile::Builder::new()
            .prefix("profile-roundtrip-")
            .tempdir_in(root.join("data"))
            .map_err(|_| "roundtrip_directory_failed")?;
        std::fs::copy(root.join("tasks.yml"), roundtrip.path().join("tasks.yml"))
            .map_err(|_| "roundtrip_copy_failed")?;

        for profile in bundle
            .profiles
            .iter()
            .filter(|p| p.validation.errors.is_empty())
        {
            let current = ceh_store::profiles::read_profiles(
                &roundtrip.path().join("tasks.yml"),
                ceh_desktop::catalog(),
                "en",
            )
            .map_err(|_| "profiles_read_failed")?;
            let draft = profile.validation.normalized.clone();
            ceh_store::profile_write::save_profile(
                &roundtrip.path().join("tasks.yml"),
                Some(&draft.id.clone()),
                draft,
                current.revision(),
                ceh_desktop::catalog(),
            )
            .map_err(|_| "profile_save_failed")?;
            saved += 1;
        }
        let mut after = profiles(roundtrip.path())?;
        after["revision"] = original["revision"].clone();
        differences(&after, &original, "profile_roundtrip", &mut mismatches);
        let roundtrip_bundle = ceh_store::profiles::read_profiles(
            &roundtrip.path().join("tasks.yml"),
            ceh_desktop::catalog(),
            "en",
        )
        .map_err(|_| "roundtrip_read_failed")?;
        for field in ["settings", "desktop"] {
            if !same_raw(
                bundle.raw().root().get(field),
                roundtrip_bundle.raw().root().get(field),
            ) {
                mismatches.push(format!("profile_roundtrip.opaque_{field}"));
            }
        }
        let before_tasks = bundle
            .raw()
            .root()
            .get("tasks")
            .and_then(|v| v.sequence())
            .ok_or("tasks_missing")?;
        let after_tasks = roundtrip_bundle
            .raw()
            .root()
            .get("tasks")
            .and_then(|v| v.sequence())
            .ok_or("tasks_missing")?;
        for (index, before) in before_tasks.iter().enumerate() {
            let after = after_tasks.get(index).ok_or("tasks_missing")?;
            for field in ["notifications", "timings"] {
                if !same_raw(before.get(field), after.get(field)) {
                    mismatches.push(format!("profile_roundtrip[{index}].opaque_{field}"));
                }
            }
        }
        reports.insert(case.into(),json!({"profile_count":bundle.profiles.len(),"saved_profiles":saved,"timeline_rows":timeline_rows,"office_rows":office_rows,"achievement_rows":achievement_rows,"already_migrated":first.already_ready,"mismatches":mismatches}));
    }
    Ok(Value::Object(reports))
}
fn main() {
    match run() {
        Ok(report) => {
            println!("{}", serde_json::to_string_pretty(&report).expect("report"));
            if report.as_object().is_some_and(|cases| {
                cases
                    .values()
                    .any(|r| r["mismatches"].as_array().is_some_and(|m| !m.is_empty()))
            }) {
                std::process::exit(1)
            }
        }
        Err(code) => {
            eprintln!("compatibility probe: {code}");
            std::process::exit(2)
        }
    }
}
