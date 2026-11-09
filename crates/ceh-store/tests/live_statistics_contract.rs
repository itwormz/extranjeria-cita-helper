use ceh_core::{
    control::ControlMode,
    model::*,
    statistics::{CounterKind, EventScope},
};
use ceh_store::{live::LiveStatistics, periods::Period, statistics::StatisticsReader};
use serde_json::json;
fn page(id: &str, kind: PageKind) -> Observation {
    Observation {
        document: DocumentId::new(id).unwrap(),
        kind,
        method: RequestMethod::Get,
        operation_code: kind == PageKind::RequestNoSlotsRestart,
        variant: String::new(),
    }
}
fn database() -> rusqlite::Connection {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch(include_str!("../src/schema.sql")).unwrap();
    db.execute_batch("ALTER TABLE episodes ADD COLUMN recognition TEXT DEFAULT 'known';ALTER TABLE episodes ADD COLUMN evidence TEXT DEFAULT '[]';ALTER TABLE episodes ADD COLUMN variant TEXT DEFAULT '';").unwrap();
    db
}
const AT: i64 = 1793592000000;
#[test]
fn live_sessions_accumulate_without_debug_and_deduplicate_pages_operations_and_booking_details() {
    let mut db = database();
    for n in 0..2 {
        let scope = EventScope {
            profile: ProfileId::new("fixture").unwrap(),
            session: SessionId::new(format!("live-{n}")).unwrap(),
        };
        let at = AT + n * 100000;
        let mut stats =
            LiveStatistics::start(&mut db, scope, &json!({"procedure":"huellas"}), at, 0).unwrap();
        let office = page("office", PageKind::Office);
        for tick in 0..3 {
            stats
                .page(
                    &mut db,
                    &office,
                    &["CNP TEST, CALLE EJEMPLO 1".into()],
                    ControlMode::Automatic,
                    at + tick * 1000,
                    tick as u64 * 1000,
                )
                .unwrap();
        }
        for _ in 0..2 {
            stats
                .operation(
                    &mut db,
                    CounterKind::Check,
                    OperationId::new("request-1").unwrap(),
                    ControlMode::Automatic,
                    at + 2000,
                )
                .unwrap();
        }
        stats.set_phase(&mut db, "paused", at + 3000, 3000).unwrap();
        stats.heartbeat(&mut db, at + 8000, 8000).unwrap();
        stats
            .set_phase(&mut db, "working", at + 8000, 8000)
            .unwrap();
        let booking = page("booking", PageKind::Booking);
        stats
            .page(
                &mut db,
                &booking,
                &[],
                ControlMode::Automatic,
                at + 9000,
                9000,
            )
            .unwrap();
        stats.enrich_booking(&mut db,&booking.document,&json!({"office":"CNP TEST, CALLE EJEMPLO 1","confirmation":"DO-NOT-SAVE","offers":[{"date":"2026-11-12","time":"09:30"}]})).unwrap();
        stats
            .set_phase(&mut db, "booking_handoff", at + 9000, 9000)
            .unwrap();
        stats
            .finish(&mut db, at + 10000, 10000, "completed")
            .unwrap();
    }
    let now = chrono::DateTime::from_timestamp_millis(AT + 200000).unwrap();
    let summary = StatisticsReader::new(&db)
        .summary("fixture", Period::All, now)
        .unwrap();
    assert_eq!(summary["checks"], 2);
    assert_eq!(summary["offices_seen"], 2);
    assert_eq!(summary["booking_screens"], 2);
    assert_eq!(summary["pages_processed"], 4);
    assert_eq!(summary["active_seconds"], 8.0);
    let evidence: Vec<String> = db
        .prepare("SELECT evidence FROM events")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(evidence.iter().any(|v| v.contains("09:30")));
    assert!(evidence.iter().all(|v| !v.contains("DO-NOT-SAVE")));
}
#[test]
fn cod_oper_period_is_not_split_by_loading_and_error_streaks_are_not_counted_on_every_poll() {
    let mut db = database();
    let scope = EventScope {
        profile: ProfileId::new("fixture").unwrap(),
        session: SessionId::new("errors").unwrap(),
    };
    let mut stats = LiveStatistics::start(&mut db, scope, &json!({}), AT, 0).unwrap();
    for (tick, kind) in [
        (0, PageKind::ServerError),
        (1, PageKind::ServerError),
        (2, PageKind::Loading),
        (3, PageKind::ServerError),
        (4, PageKind::RequestNoSlotsRestart),
        (5, PageKind::Loading),
        (6, PageKind::RequestNoSlotsRestart),
    ] {
        stats
            .page(
                &mut db,
                &page(&format!("d-{tick}"), kind),
                &[],
                ControlMode::Automatic,
                AT + tick * 1000,
                tick as u64 * 1000,
            )
            .unwrap();
    }
    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM events WHERE kind='server_error' AND counted=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    assert!(stats.heartbeat(&mut db, AT + 10000, 1).is_err());
}
#[test]
fn startup_get_reload_is_an_action_not_an_availability_check_or_new_pass(){
 let mut db=database();let scope=EventScope{profile:ProfileId::new("fixture").unwrap(),session:SessionId::new("preparation").unwrap()};
 let mut stats=LiveStatistics::start(&mut db,scope,&json!({}),AT,0).unwrap();
 stats.preparation_page(&mut db,&page("initial",PageKind::Province),ControlMode::Automatic,AT,0).unwrap();
 stats.operation(&mut db,CounterKind::AutomationAction,OperationId::new("bootstrap-reload").unwrap(),ControlMode::Automatic,AT+5000).unwrap();
 stats.preparation_page(&mut db,&page("reloaded",PageKind::Province),ControlMode::Automatic,AT+6000,6000).unwrap();
 stats.page(&mut db,&page("reloaded",PageKind::Province),&[],ControlMode::Automatic,AT+11000,11000).unwrap();
 assert_eq!(stats.counters().checks,0);assert_eq!(stats.counters().flow_passes,0);assert_eq!(stats.counters().actions_performed,1);assert_eq!(stats.counters().pages_processed,2);
 assert!(stats.preparation_page(&mut db,&page("booking",PageKind::Booking),ControlMode::Automatic,AT+12000,12000).is_err());
}
