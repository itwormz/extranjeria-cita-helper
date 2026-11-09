use ceh_store::migrate::*;
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/builds/work/rust-sqlite-tests");
        std::fs::create_dir_all(&base).unwrap();
        loop {
            let path = base.join(format!(
                "{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("scratch creation: {e}"),
            }
        }
    }
    fn options(&self) -> MigrationOptions {
        MigrationOptions {
            database: self.0.join("stats.sqlite3"),
            legacy_json: Some(self.0.join("stats.json")),
            backup_directory: self.0.join("backups"),
        }
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn snapshot(path: &std::path::Path) -> Value {
    let db = Connection::open(path).unwrap();
    let mut baseline = serde_json::Map::new();
    let mut provenance = serde_json::Map::new();
    let mut metadata = serde_json::Map::new();
    let mut schema = serde_json::Map::new();
    for (table, result) in [
        ("baseline", &mut baseline),
        ("baseline_provenance", &mut provenance),
    ] {
        let mut stmt = db
            .prepare(&format!(
                "SELECT profile,data FROM {table} ORDER BY profile"
            ))
            .unwrap();
        for row in stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .unwrap()
        {
            let (profile, data) = row.unwrap();
            result.insert(profile, serde_json::from_str(&data).unwrap());
        }
    }
    let mut stmt = db
        .prepare("SELECT key,value FROM metadata WHERE key!='rust_core_ready_v1' ORDER BY key")
        .unwrap();
    for row in stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .unwrap()
    {
        let (k, v) = row.unwrap();
        metadata.insert(k, json!(v));
    }
    let mut stmt=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap();
    for table in stmt.query_map([], |r| r.get::<_, String>(0)).unwrap() {
        let table = table.unwrap();
        let mut info = db.prepare(&format!("PRAGMA table_info({table})")).unwrap();
        let columns: Vec<Value> = info
            .query_map([], |r| {
                Ok(json!([
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, i64>(5)?
                ]))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect();
        schema.insert(table, json!(columns));
    }
    let mut stmt=db.prepare("SELECT name,tbl_name,sql FROM sqlite_master WHERE type='index' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap();
    let indexes: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(json!([
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?
            ]))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    json!({"baseline":baseline,"provenance":provenance,"metadata":metadata,"schema":schema,"indexes":indexes,"unchanged":true})
}
#[test]
fn migration_matches_current_python_schema_and_legacy_provenance() {
    let data: Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/fixtures/sqlite-migration.json"
    ))
    .unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 14);
    for case in data["cases"].as_array().unwrap() {
        let scratch = Scratch::new();
        let options = scratch.options();
        if let Some(source) = case["source"].as_str() {
            std::fs::write(options.legacy_json.as_ref().unwrap(), source).unwrap();
        }
        let at = chrono::DateTime::parse_from_rfc3339("2026-10-29T12:00:00+00:00")
            .unwrap()
            .timestamp_millis();
        let result = migrate_paths(&options, at);
        if case["expected"]["error"] == true {
            assert!(result.is_err(), "{}", case["id"]);
        } else {
            result.unwrap();
            assert_eq!(
                snapshot(&options.database),
                case["expected"],
                "{}",
                case["id"]
            );
            let before = snapshot(&options.database);
            let again = migrate_paths(&options, at).unwrap();
            assert!(again.already_ready);
            assert_eq!(snapshot(&options.database), before);
        }
        if let Some(source) = case["source"].as_str() {
            assert_eq!(
                std::fs::read_to_string(options.legacy_json.as_ref().unwrap()).unwrap(),
                source
            );
        }
    }
}

#[test]
fn existing_wal_rows_are_in_standalone_backup_before_migration() {
    let scratch = Scratch::new();
    let options = scratch.options();
    let source = Connection::open(&options.database).unwrap();
    source.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE retained(value TEXT); INSERT INTO retained VALUES ('committed in WAL');").unwrap();
    assert!(options.database.with_extension("sqlite3-wal").exists());
    let result = migrate_paths(&options, 1793275200000).unwrap();
    let backup = result.backup.unwrap();
    let saved =
        Connection::open_with_flags(&backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        saved
            .query_row("SELECT value FROM retained", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "committed in WAL"
    );
    assert_eq!(
        saved
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='baseline'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        source
            .query_row("SELECT value FROM retained", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "committed in WAL"
    );
    drop(saved);
    drop(source);
}
#[test]
fn old_episode_columns_upgrade_and_existing_data_survive() {
    let scratch = Scratch::new();
    let options = scratch.options();
    let db = Connection::open(&options.database).unwrap();
    db.execute_batch("CREATE TABLE episodes (id INTEGER PRIMARY KEY,profile TEXT,session TEXT,kind TEXT,stage TEXT,mode TEXT,document TEXT,start TEXT,end TEXT,last_seen TEXT,confirmed INTEGER DEFAULT 0); INSERT INTO episodes VALUES (1,'fixture','s','page','request_no_slots','working','doc','2026-10-20T00:00:00+00:00',NULL,'2026-10-20T00:01:00+00:00',0);").unwrap();
    drop(db);
    migrate_paths(&options, 1793275200000).unwrap();
    let db = Connection::open(&options.database).unwrap();
    let result: (String, String, String, String) = db
        .query_row(
            "SELECT document,recognition,evidence,variant FROM episodes WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        result,
        ("doc".into(), "known".into(), "[]".into(), "".into())
    );
}
#[test]
fn future_schema_is_rejected_without_changing_rows() {
    let scratch = Scratch::new();
    let options = scratch.options();
    let db = Connection::open(&options.database).unwrap();
    db.execute_batch("CREATE TABLE metadata(key TEXT PRIMARY KEY,value TEXT);INSERT INTO metadata VALUES ('schema_version','999');CREATE TABLE retained(value TEXT);INSERT INTO retained VALUES ('keep');").unwrap();
    assert!(matches!(
        migrate_paths(&options, 1793275200000),
        Err(StoreError::UnsupportedSchema)
    ));
    assert_eq!(
        db.query_row("SELECT value FROM retained", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "keep"
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='baseline'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}
#[test]
fn conflicting_legacy_backup_rolls_back_all_schema_changes() {
    let scratch = Scratch::new();
    let options = scratch.options();
    std::fs::write(options.legacy_json.as_ref().unwrap(), "{\"fixture\":7}").unwrap();
    let backup = options
        .legacy_json
        .as_ref()
        .unwrap()
        .with_extension("json.bak");
    std::fs::write(&backup, b"old distinct backup").unwrap();
    assert!(matches!(
        migrate_paths(&options, 1793275200000),
        Err(StoreError::BackupConflict)
    ));
    let db = Connection::open(&options.database).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(std::fs::read(backup).unwrap(), b"old distinct backup");
}

#[test]
fn migration_adds_notice_tables_to_an_existing_database_without_touching_rows() {
    let scratch = Scratch::new();
    let options = scratch.options();
    let old_schema: String = include_str!("../src/schema.sql")
        .lines()
        .filter(|line| !line.contains("applicant_notices"))
        .collect::<Vec<_>>()
        .join("\n");
    let db = Connection::open(&options.database).unwrap();
    db.execute_batch(&old_schema).unwrap();
    db.execute_batch("INSERT INTO events(profile,identity,kind,time,session,counted,offices) VALUES ('fixture','one','check','2026-10-29T10:00:00+00:00','session',1,'[]');").unwrap();
    drop(db);
    migrate_paths(&options, 1793275200000).unwrap();
    let db = Connection::open(&options.database).unwrap();
    for table in ["applicant_notices", "pending_applicant_notices"] {
        let found: i64 = db.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1", [table], |r| r.get(0)).unwrap();
        assert_eq!(found, 1, "{table}");
    }
    let events: i64 = db.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0)).unwrap();
    assert_eq!(events, 1);
}
