use serde_json::json;
fn database() -> rusqlite::Connection {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch(include_str!("../../ceh-store/src/schema.sql"))
        .unwrap();
    db.execute_batch("ALTER TABLE episodes ADD COLUMN recognition TEXT DEFAULT 'known';ALTER TABLE episodes ADD COLUMN evidence TEXT DEFAULT '[]';ALTER TABLE episodes ADD COLUMN variant TEXT DEFAULT '';").unwrap();
    db
}
fn run(
    input: serde_json::Value,
    db: &mut rusqlite::Connection,
) -> Result<serde_json::Value, ceh_cli::replay::ReplayError> {
    let spec = serde_json::from_value(input).unwrap();
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
        .block_on(ceh_cli::replay::run(spec, db))
}
#[test]
fn booking_handoff_prevents_all_later_automation_but_keeps_public_details() {
    let mut db = database();
    let input = json!({"schema_version":1,"profile":"fixture","session":"session","wall_start_millis":1793361600000_i64,"notifications":true,"steps":[{"event":"observe","at_ms":0,"observation":{"document":"slot","kind":"booking"},"details_pending":true},{"event":"booking_details","at_ms":10,"document":"slot","details":{"office":"Office A","confirmation":"PRIVATE","offers":[{"date":"2026-11-12","time":"09:30"}]}},{"event":"observe","at_ms":20,"observation":{"document":"next","kind":"applicant"}}]});
    let report = run(input, &mut db).unwrap();
    assert_eq!(report["control"]["mode"], "manual");
    assert!(report["browser_actions"].as_array().unwrap().is_empty());
    assert_eq!(report["statistics"]["booking_screens"], 1);
    assert_eq!(report["session_counters"]["pages_processed"], 1);
    assert_eq!(report["notifications"].as_array().unwrap().len(), 2);
    assert!(!report.to_string().contains("PRIVATE"));
}
#[test]
fn only_observed_availability_request_increments_checks() {
    let mut db = database();
    let report=run(json!({"schema_version":1,"profile":"fixture","session":"session","wall_start_millis":1793361600000_i64,"steps":[{"event":"observe","at_ms":0,"observation":{"document":"menu","kind":"request_menu"}},{"event":"availability_request","at_ms":1,"operation":"request-1"},{"event":"availability_request","at_ms":2,"operation":"request-1"}]}),&mut db).unwrap();
    assert_eq!(report["statistics"]["checks"], 1);
}

#[test]
fn storage_failure_cannot_prevent_booking_handoff() {
    let mut db = database();
    db.execute_batch("DROP TABLE events").unwrap();
    let report=run(json!({"schema_version":1,"profile":"fixture","session":"session","wall_start_millis":1793361600000_i64,"steps":[{"event":"observe","at_ms":0,"observation":{"document":"slot","kind":"booking"}}]}),&mut db).unwrap();
    assert_eq!(report["control"]["mode"], "manual");
    assert!(report["statistics_errors"].as_u64().unwrap() > 0);
    assert!(report["public_card"].is_null());
}
#[test]
fn persistent_totals_survive_new_sessions() {
    let mut db = database();
    for session in ["first", "second"] {
        let report=run(json!({"schema_version":1,"profile":"fixture","session":session,"wall_start_millis":1793361600000_i64,"steps":[{"event":"availability_request","at_ms":0,"operation":"request"}]}),&mut db).unwrap();
        assert_eq!(
            report["statistics"]["checks"],
            if session == "first" { 1 } else { 2 }
        );
    }
}
#[test]
fn pause_excludes_idle_time_and_clock_correction_does_not_change_measured_effort() {
    let mut db = database();
    let report=run(json!({"schema_version":1,"profile":"fixture","session":"session","wall_start_millis":1793361600000_i64,"steps":[{"event":"heartbeat","at_ms":1000},{"event":"pause","at_ms":2000},{"event":"heartbeat","at_ms":62000},{"event":"resume","at_ms":63000,"wall_millis":1793361501000_i64},{"event":"heartbeat","at_ms":64000}]}),&mut db).unwrap();
    let seconds: f64 = db
        .query_row("SELECT SUM(seconds) FROM active", [], |row| row.get(0))
        .unwrap();
    assert_eq!(seconds, 3.0);
    assert_eq!(report["statistics_errors"], 0);
}
#[test]
fn lost_ack_never_replays_applicant_post_after_pause() {
    let mut db = database();
    let report=run(json!({"schema_version":1,"profile":"fixture","session":"session","wall_start_millis":1793361600000_i64,"steps":[{"event":"observe","at_ms":0,"observation":{"document":"form","kind":"applicant"},"facts":{"recovery":{"official_site":true}}},{"event":"prepared","at_ms":1,"guard":{"session":"session","epoch":0,"document":"form","expected_page":"applicant"},"action":"applicant","outcome":"completed"},{"event":"observe","at_ms":2,"observation":{"document":"form","kind":"applicant"},"facts":{"recovery":{"official_site":true}},"lost_ack":true},{"event":"pause","at_ms":3},{"event":"resume","at_ms":4},{"event":"observe","at_ms":5,"observation":{"document":"form","kind":"applicant"},"facts":{"recovery":{"official_site":true}}},{"event":"observe","at_ms":5003,"observation":{"document":"form","kind":"applicant"},"facts":{"recovery":{"official_site":true}}},{"event":"observe","at_ms":10003,"observation":{"document":"form","kind":"applicant"},"facts":{"recovery":{"official_site":true}}}]}),&mut db).unwrap();
    assert_eq!(report["browser_actions"].as_array().unwrap().len(), 1);
    assert_eq!(report["statistics"]["checks"], 0);
    // The protected result wait survives pause; recovery later uses a safe GET, never another POST.
    assert_eq!(report["trace"][5]["phase"], "waiting");
    assert_eq!(
        report["trace"].as_array().unwrap().last().unwrap()["phase"],
        "recovery"
    );
}

#[test]
fn normal_transition_clears_previous_recovery_phase() {
    for previous in ["server_error", "request_no_slots"] {
        let mut db = database();
        let report=run(json!({"schema_version":1,"profile":"fixture","session":"session","wall_start_millis":1793361600000_i64,"steps":[{"event":"observe","at_ms":0,"observation":{"document":"error","kind":previous}},{"event":"observe","at_ms":1000,"observation":{"document":"menu","kind":"request_menu"}}]}),&mut db).unwrap();
        assert_eq!(
            report["trace"].as_array().unwrap().last().unwrap()["phase"],
            "working"
        );
    }
}
