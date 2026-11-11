use ceh_core::{
    model::{DocumentId, Observation, PageKind, ProfileId, RequestMethod, SessionId},
    statistics::{CounterKind, EventKey, EventScope, EvidenceId, PublicEvent},
};
use ceh_store::{periods::Period, statistics::StatisticsReader, writer::StatisticsWriter};
fn scope() -> EventScope {
    EventScope {
        profile: ProfileId::new("fixture").unwrap(),
        session: SessionId::new("session").unwrap(),
    }
}
#[test]
fn persisted_counter_dedup_and_booking_enrichment_preserve_time() {
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch(include_str!("../src/schema.sql")).unwrap();
    db.execute_batch("ALTER TABLE episodes ADD COLUMN recognition TEXT DEFAULT 'known';ALTER TABLE episodes ADD COLUMN evidence TEXT DEFAULT '[]';ALTER TABLE episodes ADD COLUMN variant TEXT DEFAULT '';").unwrap();
    let event = PublicEvent {
        key: EventKey {
            scope: scope(),
            kind: CounterKind::Booking,
            identity: EvidenceId::Document(DocumentId::new("doc").unwrap()),
        },
        at: 1793361600000,
        counted: true,
        offices: vec![],
        booking_details: Default::default(),
    };
    {
        let mut writer = StatisticsWriter::new(&mut db);
        assert!(writer.counter(&event).unwrap());
        assert!(!writer.counter(&event).unwrap());
        assert!(writer.enrich_booking(&scope(),&DocumentId::new("doc").unwrap(),&serde_json::json!({"office":"Office A","confirmation":"PRIVATE","offers":[{"date":"2026-11-12","time":"09:30"}]})).unwrap());
    }
    let evidence: String = db
        .query_row("SELECT evidence FROM events", [], |r| r.get(0))
        .unwrap();
    assert!(!evidence.contains("PRIVATE"));
    let reader = StatisticsReader::new(&db);
    assert_eq!(
        reader
            .summary(
                "fixture",
                Period::All,
                chrono::DateTime::from_timestamp_millis(event.at).unwrap()
            )
            .unwrap()["booking_screens"],
        1
    );
    assert_eq!(
        db.query_row("SELECT time FROM events", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "2026-10-30T12:00:00+00:00"
    );
}
#[test]
fn live_phase_heartbeat_extends_same_page_and_loading_does_not_clear_error() {
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch(include_str!("../src/schema.sql")).unwrap();
    db.execute_batch("ALTER TABLE episodes ADD COLUMN recognition TEXT DEFAULT 'known';ALTER TABLE episodes ADD COLUMN evidence TEXT DEFAULT '[]';ALTER TABLE episodes ADD COLUMN variant TEXT DEFAULT '';").unwrap();
    let observed = Observation {
        document: DocumentId::new("doc").unwrap(),
        kind: PageKind::ServerError,
        method: RequestMethod::Get,
        operation_code: false,
        variant: String::new(),
    };
    let mut writer = StatisticsWriter::new(&mut db);
    writer
        .start_session(&scope(), 1793361600000, &serde_json::json!({}))
        .unwrap();
    writer.page(&scope(), &observed, 1793361600000).unwrap();
    writer.phase(&scope(), "waiting", 1793361600000).unwrap();
    for at in [1793361630000, 1793361660000] {
        writer.heartbeat(&scope(), at).unwrap();
    }
    writer.page(&scope(), &observed, 1793361660000).unwrap();
    let mut loading = observed;
    loading.kind = PageKind::Loading;
    writer.page(&scope(), &loading, 1793361661000).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM episodes WHERE kind='page'", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn stored_events_and_serial_episodes_match_python() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/fixtures/statistics-writes.json"
    ))
    .unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!("../src/schema.sql")).unwrap();
        db.execute_batch("ALTER TABLE episodes ADD COLUMN recognition TEXT DEFAULT 'known';ALTER TABLE episodes ADD COLUMN evidence TEXT DEFAULT '[]';ALTER TABLE episodes ADD COLUMN variant TEXT DEFAULT '';").unwrap();
        for step in case["input"]["steps"].as_array().unwrap() {
            let at = 1793361600000 + step["at"].as_i64().unwrap() * 1000;
            let mut writer = StatisticsWriter::new(&mut db);
            match step["op"].as_str().unwrap() {
                "start" => writer
                    .start_session(&scope(), at, &step["context"])
                    .unwrap(),
                "end" => writer
                    .end_session(&scope(), at, step["reason"].as_str().unwrap_or("stopped"))
                    .unwrap(),
                "phase" => writer
                    .phase(&scope(), step["mode"].as_str().unwrap(), at)
                    .unwrap(),
                "heartbeat" => writer.heartbeat(&scope(), at).unwrap(),
                "active" => writer
                    .add_active(&scope(), step["seconds"].as_f64().unwrap(), at)
                    .unwrap(),
                "page" => {
                    let observed = Observation {
                        document: DocumentId::new(step["document"].as_str().unwrap_or("doc"))
                            .unwrap(),
                        kind: serde_json::from_value(step["stage"].clone()).unwrap(),
                        method: RequestMethod::Unknown,
                        operation_code: step["operation_code"].as_bool().unwrap_or(false),
                        variant: step["variant"].as_str().unwrap_or("").into(),
                    };
                    writer.page(&scope(), &observed, at).unwrap();
                }
                "counter" => {
                    let kind: CounterKind = serde_json::from_value(step["kind"].clone()).unwrap();
                    let document = step["document"].as_str().unwrap();
                    let event = PublicEvent {
                        key: EventKey {
                            scope: scope(),
                            kind,
                            identity: if matches!(
                                kind,
                                CounterKind::Check | CounterKind::AutomationAction
                            ) {
                                EvidenceId::Operation(
                                    ceh_core::model::OperationId::new(document).unwrap(),
                                )
                            } else {
                                EvidenceId::Document(DocumentId::new(document).unwrap())
                            },
                        },
                        at,
                        counted: step["counted"].as_bool().unwrap_or(true),
                        offices: step["offices"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|v| v.as_str().unwrap().to_owned())
                            .collect(),
                        booking_details: Default::default(),
                    };
                    writer.counter(&event).unwrap();
                }
                "enrich" => {
                    writer
                        .enrich_booking(
                            &scope(),
                            &DocumentId::new(step["document"].as_str().unwrap()).unwrap(),
                            &step["details"],
                        )
                        .unwrap();
                }
                _ => panic!("unknown fixture"),
            }
        }
        let mut actual = serde_json::Map::new();
        for table in ["events", "episodes", "sessions", "active", "activity_days"] {
            let mut stmt = db.prepare(&format!("SELECT * FROM {table}")).unwrap();
            let columns = stmt.column_names().map(str::to_owned).collect::<Vec<_>>();
            let rows = stmt
                .query_map([], |row| {
                    let mut map = serde_json::Map::new();
                    for (index, key) in columns.iter().enumerate() {
                        let value = match row.get_ref(index)? {
                            rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                            rusqlite::types::ValueRef::Integer(value) => serde_json::json!(value),
                            rusqlite::types::ValueRef::Real(value) => serde_json::json!(value),
                            rusqlite::types::ValueRef::Text(value) => {
                                let value = std::str::from_utf8(value).unwrap();
                                if ["evidence", "offices", "context"].contains(&key.as_str()) {
                                    serde_json::from_str(value).unwrap()
                                } else {
                                    serde_json::json!(value)
                                }
                            }
                            _ => panic!("unexpected blob"),
                        };
                        map.insert(key.clone(), value);
                    }
                    Ok(serde_json::Value::Object(map))
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            actual.insert(table.into(), serde_json::json!(rows));
        }
        assert_eq!(
            serde_json::Value::Object(actual),
            case["expected"],
            "{}",
            case["id"]
        );
    }
}

#[test]
fn heartbeat_does_not_bridge_gap_over_35_seconds_by_rounding() {
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch(include_str!("../src/schema.sql")).unwrap();
    db.execute_batch("ALTER TABLE episodes ADD COLUMN recognition TEXT DEFAULT 'known';ALTER TABLE episodes ADD COLUMN evidence TEXT DEFAULT '[]';ALTER TABLE episodes ADD COLUMN variant TEXT DEFAULT ''; ").unwrap();
    let start = 1793361600100;
    let mut writer = StatisticsWriter::new(&mut db);
    writer
        .start_session(&scope(), start, &serde_json::json!({}))
        .unwrap();
    writer.phase(&scope(), "waiting", start).unwrap();
    writer.heartbeat(&scope(), start + 35_001).unwrap();
    let last: String = db
        .query_row(
            "SELECT last_seen FROM episodes WHERE kind='phase'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(last, "2026-10-30T12:00:00.100000+00:00");
}

fn notice_db() -> rusqlite::Connection {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch(include_str!("../src/schema.sql")).unwrap();
    db.execute_batch("ALTER TABLE episodes ADD COLUMN recognition TEXT DEFAULT 'known';ALTER TABLE episodes ADD COLUMN evidence TEXT DEFAULT '[]';ALTER TABLE episodes ADD COLUMN variant TEXT DEFAULT ''").unwrap();
    db
}
fn observed(document: &str, kind: PageKind) -> Observation {
    Observation {
        document: DocumentId::new(document).unwrap(),
        kind,
        method: RequestMethod::Get,
        operation_code: false,
        variant: String::new(),
    }
}
fn notices(db: &rusqlite::Connection) -> Vec<(String, String, String)> {
    let mut statement = db.prepare("SELECT session,document,start FROM applicant_notices ORDER BY session,document").unwrap();
    statement.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect()
}

#[test]
fn applicant_notice_is_recorded_once_per_session_and_document_keeping_the_earliest_time() {
    let mut db = notice_db();
    let other = EventScope { profile: ProfileId::new("fixture").unwrap(), session: SessionId::new("other").unwrap() };
    {
        let mut writer = StatisticsWriter::new(&mut db);
        writer.page(&scope(), &observed("doc-a", PageKind::ApplicantNoSlots), 1793361600000).unwrap();
        writer.page(&scope(), &observed("doc-a", PageKind::ApplicantNoSlots), 1793361660000).unwrap();
        writer.page(&scope(), &observed("doc-a", PageKind::ApplicantNoSlots), 1793361000000).unwrap();
        writer.page(&scope(), &observed("doc-b", PageKind::ApplicantNoSlots), 1793361700000).unwrap();
        writer.page(&other, &observed("doc-a", PageKind::ApplicantNoSlots), 1793361800000).unwrap();
    }
    let rows = notices(&db);
    assert_eq!(rows.len(), 3, "{rows:?}");
    let first = rows.iter().find(|r| r.0 == "session" && r.1.len() == 64 && r.2 == "2026-10-30T11:50:00+00:00");
    assert!(first.is_some(), "the earliest observation of doc-a wins: {rows:?}");
    assert!(rows.iter().all(|r| r.1.chars().all(|c| c.is_ascii_hexdigit())), "documents are stored only as digests");
}

#[test]
fn other_stages_do_not_create_notices() {
    let mut db = notice_db();
    {
        let mut writer = StatisticsWriter::new(&mut db);
        writer.page(&scope(), &observed("doc-a", PageKind::Applicant), 1793361600000).unwrap();
        writer.page(&scope(), &observed("doc-b", PageKind::RequestNoSlots), 1793361660000).unwrap();
    }
    assert!(notices(&db).is_empty());
}
