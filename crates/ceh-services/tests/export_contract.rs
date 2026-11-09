use ceh_services::exports::{
    calendar, google_calendar_url, public_duration, public_post, snapshot,
};
use serde_json::json;
fn example() -> serde_json::Value {
    json!({"id":"synthetic-id","recorded_at":"2026-10-14T12:00:00+00:00","statistics":{"checks":3645,"started_at":"2026-09-27T12:00:00+00:00","active_seconds":174720,"first_available_seconds":8040,"name":"PRIVATE PERSON"},"appointment":{"date":"2026-10-22","time":"10:30","office":"PRIVATE OFFICE","address":"PRIVATE ADDRESS","confirmation":"PRIVATE CONFIRMATION","timezone":"Europe/Madrid"}})
}
#[test]
fn public_sharing_never_contains_private_appointment_fields() {
    for language in ["ru", "en", "es"] {
        let value = example();
        let post = public_post(&value, language, true);
        assert!(!post.contains("PRIVATE"));
        assert!(!post.contains("2026-10-22"));
        assert!(post.contains("3645"));
        assert!(public_duration(&value, language).contains("17"));
    }
    assert!(snapshot(&json!({"checks":42}))["actions_performed"].is_null());
}
#[test]
fn calendar_escapes_folds_and_omits_confirmation() {
    let mut value = example();
    value["appointment"]["address"] = json!(format!(
        "a,b;c\\d\r\nEND:VEVENT\nBEGIN:VEVENT{}",
        "я".repeat(100)
    ));
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-30T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let data = calendar(&value, now).unwrap();
    assert!(!data.contains("PRIVATE CONFIRMATION"));
    assert_eq!(data.matches("\r\nBEGIN:VEVENT\r\n").count(), 1);
    assert!(data.split("\r\n").all(|line| line.len() <= 75));
    let url = google_calendar_url(&value).unwrap();
    assert!(!url.contains("CONFIRMATION"));
    assert!(url.contains("20261022T083000Z"));
}

fn check_exports_against_python(op: &str) {
    use ceh_services::exports::public_card;
    let data: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/fixtures/exports.json"
    ))
    .unwrap();
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-30T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let mut checked = 0;
    for case in data["cases"].as_array().unwrap().iter().filter(|case| case["input"]["op"] == op) {
        checked += 1;
        let input = &case["input"];
        let value = &input["achievement"];
        let language = input["language"].as_str().unwrap_or("en");
        let actual = match input["op"].as_str().unwrap() {
            "snapshot" => snapshot(&input["statistics"]),
            "public" => {
                json!({"post":public_post(value,language,input["detailed"].as_bool().unwrap_or(true)),"duration":public_duration(value,language),"card":public_card(value,language)})
            }
            "calendar" => match calendar(value, now) {
                Ok(calendar) => {
                    json!({"calendar":calendar,"google":google_calendar_url(value).unwrap()})
                }
                Err(_) => json!({"error":"invalid"}),
            },
            _ => panic!("unknown fixture"),
        };
        assert_eq!(actual, case["expected"], "{}", case["id"]);
    }
    assert!(checked > 0, "no {op} cases");
}

#[test]
fn snapshots_match_python() {
    check_exports_against_python("snapshot");
}
#[test]
fn public_models_and_posts_match_python() {
    check_exports_against_python("public");
}
#[test]
fn calendars_match_python() {
    check_exports_against_python("calendar");
}

#[test]
fn achievement_store_is_profile_scoped_and_snapshot_is_immutable() {
    use ceh_services::exports::AchievementStore;
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/builds/work/export-tests");
    std::fs::create_dir_all(&base).unwrap();
    let dir = tempfile::tempdir_in(base).unwrap();
    let store = AchievementStore::new(&dir.path().join("achievements.json"));
    let source = example();
    let at = chrono::DateTime::parse_from_rfc3339("2026-10-30T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let mut value = store
        .record("fixture", &source["statistics"], &source["appointment"], at)
        .unwrap();
    let original = value["statistics"].clone();
    let id = value["id"].as_str().unwrap().to_owned();
    value["statistics"]["checks"] = json!(0);
    let updated = store
        .update_appointment("fixture", &id, &json!({"date":"2026-11-01"}))
        .unwrap();
    assert_eq!(updated["statistics"], original);
    assert!(store.list("other").unwrap().is_empty());
    assert!(store.update_appointment("other", &id, &json!({})).is_err());
    assert_eq!(store.list("fixture").unwrap().len(), 1);
}

#[test]
fn achievement_retains_unknown_legacy_quality_in_private_and_public_views() {
    let dir = tempfile::tempdir().unwrap();
    let store = ceh_services::exports::AchievementStore::new(&dir.path().join("achievements.json"));
    let at = chrono::DateTime::parse_from_rfc3339("2026-10-31T15:00:00Z")
        .unwrap()
        .to_utc();
    let stats = json!({"checks":48,"rate_limited":0,"blocked":0,"legacy_unknown_fields":["checks","rate_limited","blocked","UNSAFE"],"confirmation":"PRIVATE"});
    let record = store
        .record("fixture", &stats, &json!({"confirmation":"PRIVATE"}), at)
        .unwrap();
    assert_eq!(
        record["statistics_provenance"]["unknown_fields"],
        json!(["checks", "rate_limited", "blocked"])
    );
    let card = ceh_services::exports::public_card(&record, "en");
    let rows = card["rows"].as_array().unwrap();
    assert_eq!(rows[0][1], "48");
    assert_eq!(rows[7][1], "—");
    assert_eq!(rows[8][1], "—");
    assert!(!card.to_string().contains("PRIVATE"));
    let updated = store
        .update_appointment(
            "fixture",
            record["id"].as_str().unwrap(),
            &json!({"office":"changed"}),
        )
        .unwrap();
    assert_eq!(
        updated["statistics_provenance"],
        record["statistics_provenance"]
    );
}
