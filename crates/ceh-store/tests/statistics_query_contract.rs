use ceh_store::{periods::Period, statistics::StatisticsReader};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-30T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}
fn db() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(include_str!("../src/schema.sql")).unwrap();
    db.execute(
        "INSERT INTO baseline VALUES ('fixture',?1)",
        [r#"{"checks":12,"_offices":["Legacy office"],"_last_office":"Legacy office"}"#],
    )
    .unwrap();
    for (id, kind, time, counted, offices) in [
        ("one", "check", "2026-10-29T22:59:59+00:00", 1, "[]"),
        ("two", "check", "2026-10-29T23:00:00+00:00", 1, "[]"),
        ("three", "check", "2026-10-30T01:00:00+00:00", 0, "[]"),
        (
            "four",
            "office",
            "2026-10-30T02:00:00+00:00",
            1,
            "[\"Office A\",\"Office B\"]",
        ),
        (
            "five",
            "office",
            "2026-10-30T03:00:00+00:00",
            1,
            "[\"Office A\"]",
        ),
    ] {
        db.execute("INSERT INTO events(profile,identity,kind,time,session,counted,offices) VALUES ('fixture',?1,?2,?3,'session',?4,?5)",rusqlite::params![id,kind,time,counted,offices]).unwrap();
    }
    db
}
#[test]
fn counted_checks_respect_today_boundary_and_legacy_all_only() {
    let db = db();
    let reader = StatisticsReader::new(&db);
    assert_eq!(reader.total("fixture", Period::All, now()).unwrap(), 14);
    assert_eq!(reader.total("fixture", Period::Today, now()).unwrap(), 1);
    assert_eq!(reader.total("other", Period::All, now()).unwrap(), 0);
}
#[test]
fn office_history_preserves_times_sessions_and_unknown_legacy_count() {
    let db = db();
    let reader = StatisticsReader::new(&db);
    let rows = reader
        .office_history("fixture", Period::All, now())
        .unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].label, "Office A");
    assert_eq!(rows[0].count, Some(2));
    assert_eq!(rows[0].appearances.len(), 2);
    assert_eq!(rows[0].appearances[0].session_id, "session");
    assert_eq!(rows[2].count, None);
    assert_eq!(
        reader.last_office("fixture", Period::Today, now()).unwrap(),
        "Office A"
    );
    assert_eq!(
        reader
            .known_offices("fixture", Period::Today, now())
            .unwrap(),
        vec!["Office A", "Office B"]
    );
}

#[test]
fn read_queries_match_independent_python_fixtures() {
    let fixtures: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/fixtures/statistics-queries.json"
    ))
    .unwrap();
    for case in fixtures["cases"].as_array().unwrap() {
        let input = &case["input"];
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!("../src/schema.sql")).unwrap();
        if !input["baseline"].is_null() {
            db.execute(
                "INSERT INTO baseline VALUES ('fixture',?1)",
                [input["baseline"].to_string()],
            )
            .unwrap();
        }
        for (i, event) in input["events"].as_array().unwrap().iter().enumerate() {
            db.execute("INSERT INTO events(profile,identity,kind,time,session,counted,offices) VALUES (?1,?2,?3,?4,?5,?6,?7)",rusqlite::params![event["profile"].as_str().unwrap_or("fixture"),i.to_string(),event["kind"].as_str().unwrap(),event["time"].as_str().unwrap(),event["session"].as_str().unwrap_or("session"),event["counted"].as_i64().unwrap_or(1),event.get("offices").cloned().unwrap_or(serde_json::json!([])).to_string()]).unwrap();
        }
        if let Some(provenance) = input.get("provenance") {
            db.execute(
                "INSERT INTO baseline_provenance VALUES ('fixture',?1,?2)",
                rusqlite::params![
                    provenance.to_string(),
                    input["migrated_at"].as_str().unwrap()
                ],
            )
            .unwrap();
        }
        for interval in input
            .get("active")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            db.execute("INSERT INTO active(profile,session,start,end,seconds) VALUES ('fixture','session',?1,?2,?3)",rusqlite::params![interval["start"].as_str().unwrap(),interval["end"].as_str().unwrap(),interval["seconds"].as_f64().unwrap()]).unwrap();
        }
        for interval in input
            .get("observed")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            db.execute(
                "INSERT INTO observed_active VALUES ('fixture','observed',?1,?2)",
                rusqlite::params![
                    interval["start"].as_str().unwrap(),
                    interval["end"].as_str().unwrap()
                ],
            )
            .unwrap();
        }
        let reader = StatisticsReader::new(&db);
        let period = if input["period"] == "today" {
            Period::Today
        } else {
            Period::All
        };
        let now = DateTime::parse_from_rfc3339(input["now"].as_str().unwrap())
            .unwrap()
            .with_timezone(&Utc);
        let profile = input["profile"].as_str().unwrap_or("fixture");
        let b = ceh_store::periods::bounds(period, now).unwrap();
        let actual = serde_json::json!({"summary":reader.summary(profile,period,now).unwrap(),"total":reader.total(profile,period,now).unwrap(),"offices":reader.office_history(profile,period,now).unwrap(),"known":reader.known_offices(profile,period,now).unwrap(),"last":reader.last_office(profile,period,now).unwrap(),"bounds":[b.start.map(|v|v.to_rfc3339()),b.end.map(|v|v.to_rfc3339())]});
        fn numeric(value: serde_json::Value) -> serde_json::Value {
            match value {
                serde_json::Value::Number(number) => serde_json::json!(number.as_f64().unwrap()),
                serde_json::Value::Array(values) => values.into_iter().map(numeric).collect(),
                serde_json::Value::Object(values) => serde_json::Value::Object(
                    values
                        .into_iter()
                        .map(|(key, value)| (key, numeric(value)))
                        .collect(),
                ),
                other => other,
            }
        }
        assert_eq!(
            numeric(actual),
            numeric(case["expected"].clone()),
            "{}",
            case["id"]
        );
    }
}

#[test]
fn active_time_keeps_recorded_seconds_but_clips_cross_midnight() {
    let db = db();
    db.execute("INSERT INTO active(profile,session,start,end,seconds) VALUES ('fixture','session','2026-10-29T22:00:00+00:00','2026-10-30T00:00:00+00:00',7000)",[]).unwrap();
    let reader = StatisticsReader::new(&db);
    assert_eq!(
        reader
            .active_seconds("fixture", Period::All, now())
            .unwrap(),
        7000.0
    );
    assert_eq!(
        reader
            .active_seconds("fixture", Period::Today, now())
            .unwrap(),
        3600.0
    );
    assert_eq!(
        reader.active_seconds("other", Period::All, now()).unwrap(),
        0.0
    );
}

#[test]
fn history_keeps_open_episode_and_exact_event_facts() {
    let db = db();
    db.execute_batch("ALTER TABLE episodes ADD COLUMN recognition TEXT DEFAULT 'known';ALTER TABLE episodes ADD COLUMN evidence TEXT DEFAULT '[]';ALTER TABLE episodes ADD COLUMN variant TEXT DEFAULT '';UPDATE events SET stage='',mode='',evidence='[]';INSERT INTO episodes(profile,session,kind,stage,mode,document,start,last_seen,confirmed) VALUES ('fixture','session','page','request_no_slots','waiting','doc','2026-10-30T01:00:00+00:00','2026-10-30T04:00:00+00:00',0);").unwrap();
    let reader = StatisticsReader::new(&db);
    let rows = reader
        .history("fixture", Period::Today, now(), None, None)
        .unwrap();
    assert_eq!(rows.len(), 5);
    let page = rows.iter().find(|row| row["kind"] == "page").unwrap();
    assert!(page["end"].is_null());
    assert_eq!(page["last_seen"], "2026-10-30T04:00:00+00:00");
    assert_eq!(page["confirmed_end"], false);
    assert!(
        reader
            .history("other", Period::All, now(), None, None)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn stored_timeline_loads_session_context_before_visible_range() {
    let db = db();
    db.execute_batch("ALTER TABLE episodes ADD COLUMN recognition TEXT DEFAULT 'known';ALTER TABLE episodes ADD COLUMN evidence TEXT DEFAULT '[]';ALTER TABLE episodes ADD COLUMN variant TEXT DEFAULT '';UPDATE events SET stage='',mode='',evidence='[]';INSERT INTO sessions VALUES ('fixture','session','2026-10-29T22:00:00+00:00','2026-10-30T04:00:00+00:00','2026-10-30T04:00:00+00:00','completed','{}');INSERT INTO episodes(profile,session,kind,stage,mode,document,start,end,last_seen,confirmed) VALUES ('fixture','session','page','request_no_slots_restart','waiting','doc','2026-10-29T22:00:00+00:00','2026-10-30T04:00:00+00:00','2026-10-30T04:00:00+00:00',1);UPDATE episodes SET evidence='[\"operation_code\"]';").unwrap();
    let reader = StatisticsReader::new(&db);
    let history = reader
        .timeline_history("fixture", Period::Today, now(), None, None)
        .unwrap();
    assert!(
        history
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["timeline_state"] == "cod")
    );
    let bounds = reader
        .timeline_bounds("fixture", Period::All, now())
        .unwrap();
    assert_eq!(bounds["start"], "2026-10-29T22:00:00+00:00");
    assert_eq!(bounds["end"], "2026-10-30T04:00:00+00:00");
}

#[test]
fn dashboard_keeps_all_today_and_profile_isolation_in_one_snapshot() {
    let db = db();
    let snapshot = ceh_store::statistics::dashboard(&db, "fixture", now()).unwrap();
    assert_eq!(snapshot.all["checks"], 14);
    assert_eq!(snapshot.today["checks"], 1);
    assert_eq!(snapshot.offices.len(), 3);
    assert_eq!(snapshot.offices[2].count, None);
    let other = ceh_store::statistics::dashboard(&db, "other", now()).unwrap();
    assert_eq!(other.all["checks"], 0);
    assert!(other.offices.is_empty());
}

#[test]
fn dashboard_file_does_not_create_or_migrate_storage() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite3");
    assert!(
        ceh_store::statistics::dashboard_file(&path, "fixture", now())
            .unwrap()
            .is_none()
    );
    assert!(!path.exists());
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(include_str!("../src/schema.sql"))
        .unwrap();
    connection
        .execute(
            "INSERT INTO baseline VALUES ('fixture',?1)",
            [r#"{"checks":42}"#],
        )
        .unwrap();
    drop(connection);
    let before = std::fs::read(&path).unwrap();
    let value = ceh_store::statistics::dashboard_file(&path, "fixture", now())
        .unwrap()
        .unwrap();
    assert_eq!(value.all["checks"], 42);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    std::fs::write(&path, b"not a database").unwrap();
    assert!(ceh_store::statistics::dashboard_file(&path, "fixture", now()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"not a database");
}

#[test]
fn history_snapshot_filters_offices_and_keeps_session_context_for_clips() {
    let db = db();
    db.execute_batch("ALTER TABLE episodes ADD COLUMN recognition TEXT DEFAULT 'known';ALTER TABLE episodes ADD COLUMN evidence TEXT DEFAULT '[]';ALTER TABLE episodes ADD COLUMN variant TEXT DEFAULT '';UPDATE events SET stage='',mode='',evidence='[]';INSERT INTO sessions VALUES ('fixture','session','2026-10-29T22:00:00+00:00','2026-10-30T04:00:00+00:00','2026-10-30T04:00:00+00:00','completed','{}');INSERT INTO episodes(profile,session,kind,stage,mode,document,start,end,last_seen,confirmed) VALUES ('fixture','session','page','request_no_slots_restart','waiting','doc','2026-10-29T22:00:00+00:00','2026-10-30T04:00:00+00:00','2026-10-30T04:00:00+00:00',1);").unwrap();
    let start = chrono::DateTime::parse_from_rfc3339("2026-10-30T02:30:00Z")
        .unwrap()
        .to_utc();
    let end = chrono::DateTime::parse_from_rfc3339("2026-10-30T04:00:00Z")
        .unwrap()
        .to_utc();
    let value =
        ceh_store::statistics::history_snapshot(&db, "fixture", now(), Some(start), Some(end))
            .unwrap();
    assert_eq!(value.offices.len(), 1);
    assert_eq!(value.offices[0].label, "Office A");
    assert_eq!(value.offices[0].count, Some(1));
    assert!(
        value
            .timeline
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["start"] == "2026-10-30T02:00:00+00:00"
                && row["source_rows"][0]["start"] == "2026-10-29T22:00:00+00:00")
    );
    let all = ceh_store::statistics::history_snapshot(&db, "fixture", now(), None, None).unwrap();
    assert_eq!(all.offices.len(), 3);
    assert_eq!(all.offices[2].count, None);
    assert!(
        ceh_store::statistics::history_snapshot(&db, "fixture", now(), Some(end), Some(start))
            .is_err()
    );
}

#[test]
fn selection_uses_only_current_profile_projected_ids_and_clips_the_requested_range() {
    let db = db();
    db.execute_batch("ALTER TABLE episodes ADD COLUMN recognition TEXT DEFAULT 'known';ALTER TABLE episodes ADD COLUMN evidence TEXT DEFAULT '[]';ALTER TABLE episodes ADD COLUMN variant TEXT DEFAULT '';UPDATE events SET stage='',mode='',evidence='[]';INSERT INTO sessions VALUES ('fixture','session','2026-10-29T22:00:00+00:00','2026-10-30T04:00:00+00:00','2026-10-30T04:00:00+00:00','completed','{}');INSERT INTO episodes(profile,session,kind,stage,mode,document,start,end,last_seen,confirmed) VALUES ('fixture','session','page','request_no_slots','waiting','doc','2026-10-29T22:00:00+00:00','2026-10-30T04:00:00+00:00','2026-10-30T04:00:00+00:00',1);").unwrap();
    let lower = chrono::DateTime::parse_from_rfc3339("2026-10-30T02:30:00Z")
        .unwrap()
        .to_utc();
    let upper = chrono::DateTime::parse_from_rfc3339("2026-10-30T04:00:00Z")
        .unwrap()
        .to_utc();
    let snapshot =
        ceh_store::statistics::history_snapshot(&db, "fixture", now(), Some(lower), Some(upper))
            .unwrap();
    let ids = snapshot
        .timeline
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == "clip")
        .map(ceh_store::statistics::timeline_key)
        .collect::<Vec<_>>();
    let actual = ceh_store::statistics::selected_snapshot(
        &db,
        "fixture",
        now(),
        Some(lower),
        Some(upper),
        &ids,
    )
    .unwrap();
    assert_eq!(actual["seconds"], 5400.0);
    assert_eq!(actual["counts"]["checks"], 0);
    assert!(
        ceh_store::statistics::selected_snapshot(
            &db,
            "other",
            now(),
            Some(lower),
            Some(upper),
            &ids
        )
        .is_err()
    );
    assert!(
        ceh_store::statistics::selected_snapshot(
            &db,
            "fixture",
            now(),
            None,
            None,
            &["invented".into()]
        )
        .is_err()
    );
}

#[test]
fn appointment_offices_keep_last_observed_and_legacy_default_without_creating_db() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("history.sqlite3");
    assert!(
        ceh_store::statistics::appointment_offices_file(&path, "fixture", now())
            .unwrap()
            .is_none()
    );
    assert!(!path.exists());
    let source = db();
    source
        .execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    let data = ceh_store::statistics::appointment_offices_file(&path, "fixture", now())
        .unwrap()
        .unwrap();
    assert_eq!(data.1, "Office A");
    assert_eq!(data.0, vec!["Legacy office", "Office A", "Office B"]);
    let data = ceh_store::statistics::appointment_offices_file(&path, "other", now())
        .unwrap()
        .unwrap();
    assert!(data.0.is_empty());
    assert!(data.1.is_empty());
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("DELETE FROM events", []).unwrap();
    let data = ceh_store::statistics::appointment_offices_file(&path, "fixture", now())
        .unwrap()
        .unwrap();
    assert_eq!(data.1, "Legacy office");
}
