//! Restoring applicant "no appointments" notices from presentation journals (Python 0.3.3+).
use ceh_store::notices::restore_applicant_notices;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn database() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(include_str!("../src/schema.sql")).unwrap();
    db
}
fn session(db: &Connection, name: &str, start: &str, end: Option<&str>) {
    db.execute("INSERT INTO sessions(profile,session,start,end) VALUES ('fixture',?1,?2,?3)", rusqlite::params![name, start, end]).unwrap();
}
fn journal(root: &Path, folder: &str) -> std::path::PathBuf {
    let directory = root.join("sessions").join(folder);
    fs::create_dir_all(&directory).unwrap();
    directory.join("journal.jsonl")
}
fn visit(document: &str, time: &str) -> String {
    format!("{}\n", serde_json::json!({"event":"visit","stage":"applicant_no_slots","document":document,"time":time}))
}
fn notices(db: &Connection) -> Vec<(String, String, String)> {
    let mut statement = db.prepare("SELECT session,document,start FROM applicant_notices ORDER BY session,document,start").unwrap();
    statement.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect()
}
fn pending(db: &Connection) -> i64 {
    db.query_row("SELECT COUNT(*) FROM pending_applicant_notices", [], |r| r.get(0)).unwrap()
}

#[test]
fn journal_visit_becomes_a_notice_for_the_matching_session() {
    let (mut db, root) = (database(), tempfile::tempdir().unwrap());
    session(&db, "s1", "2026-10-30T09:00:00+00:00", Some("2026-10-30T11:00:00+00:00"));
    fs::write(journal(root.path(), "fixture-20261030T0900"), visit("doc-a", "2026-10-30T10:00:00Z")).unwrap();
    assert_eq!(restore_applicant_notices(&mut db, "fixture", root.path()).unwrap(), 1);
    assert_eq!(notices(&db), [("s1".to_owned(), digest("doc-a"), "2026-10-30T10:00:00+00:00".to_owned())]);
    assert_eq!(pending(&db), 0);
}

#[test]
fn truncated_last_line_is_read_after_it_is_completed() {
    let (mut db, root) = (database(), tempfile::tempdir().unwrap());
    session(&db, "s1", "2026-10-30T09:00:00+00:00", None);
    let path = journal(root.path(), "fixture-a");
    let second = visit("doc-b", "2026-10-30T10:05:00+00:00");
    fs::write(&path, format!("{}{}", visit("doc-a", "2026-10-30T10:00:00+00:00"), second.trim_end())).unwrap();
    assert_eq!(restore_applicant_notices(&mut db, "fixture", root.path()).unwrap(), 1, "the unfinished line is not consumed");
    fs::write(&path, format!("{}{}", visit("doc-a", "2026-10-30T10:00:00+00:00"), second)).unwrap();
    assert_eq!(restore_applicant_notices(&mut db, "fixture", root.path()).unwrap(), 1);
    assert_eq!(notices(&db).len(), 2);
}

#[test]
fn shrunken_file_is_reread_from_the_start() {
    let (mut db, root) = (database(), tempfile::tempdir().unwrap());
    session(&db, "s1", "2026-10-30T09:00:00+00:00", None);
    let path = journal(root.path(), "fixture-a");
    fs::write(&path, format!("{}{}", visit("doc-a", "2026-10-30T10:00:00+00:00"), visit("doc-b", "2026-10-30T10:01:00+00:00"))).unwrap();
    restore_applicant_notices(&mut db, "fixture", root.path()).unwrap();
    fs::write(&path, visit("doc-c", "2026-10-30T10:02:00+00:00")).unwrap();
    assert_eq!(restore_applicant_notices(&mut db, "fixture", root.path()).unwrap(), 1);
    assert!(notices(&db).iter().any(|row| row.1 == digest("doc-c")));
}

#[test]
fn replay_is_idempotent() {
    let (mut db, root) = (database(), tempfile::tempdir().unwrap());
    session(&db, "s1", "2026-10-30T09:00:00+00:00", None);
    fs::write(journal(root.path(), "fixture-a"), visit("doc-a", "2026-10-30T10:00:00+00:00")).unwrap();
    assert_eq!(restore_applicant_notices(&mut db, "fixture", root.path()).unwrap(), 1);
    assert_eq!(restore_applicant_notices(&mut db, "fixture", root.path()).unwrap(), 0);
    assert_eq!(notices(&db).len(), 1);
    // Re-reading from the start (cursor lost) must not duplicate or move the notice later.
    db.execute("DELETE FROM metadata WHERE key LIKE 'applicant-notices-v2:%'", []).unwrap();
    restore_applicant_notices(&mut db, "fixture", root.path()).unwrap();
    assert_eq!(notices(&db), [("s1".to_owned(), digest("doc-a"), "2026-10-30T10:00:00+00:00".to_owned())]);
}

#[test]
fn unmatched_visit_stays_pending_and_matches_when_the_session_appears() {
    let (mut db, root) = (database(), tempfile::tempdir().unwrap());
    fs::write(journal(root.path(), "fixture-a"), visit("doc-a", "2026-10-30T10:00:00+00:00")).unwrap();
    assert_eq!(restore_applicant_notices(&mut db, "fixture", root.path()).unwrap(), 0);
    assert_eq!((notices(&db).len(), pending(&db)), (0, 1));
    session(&db, "late", "2026-10-30T09:00:00+00:00", None);
    assert_eq!(restore_applicant_notices(&mut db, "fixture", root.path()).unwrap(), 1, "pending visits are retried even when no journal grew");
    assert_eq!((notices(&db).len(), pending(&db)), (1, 0));
}

#[test]
fn malformed_foreign_and_non_visit_rows_are_ignored() {
    let (mut db, root) = (database(), tempfile::tempdir().unwrap());
    session(&db, "s1", "2026-10-30T09:00:00+00:00", None);
    let rows = [
        "not json\n".to_owned(),
        "[1,2]\n".to_owned(),
        "{\"event\":\"page\",\"stage\":\"applicant_no_slots\",\"document\":\"d\",\"time\":\"2026-10-30T10:00:00+00:00\"}\n".to_owned(),
        "{\"event\":\"visit\",\"stage\":\"office\",\"document\":\"d\",\"time\":\"2026-10-30T10:00:00+00:00\"}\n".to_owned(),
        "{\"event\":\"visit\",\"stage\":\"applicant_no_slots\",\"document\":\"\",\"time\":\"2026-10-30T10:00:00+00:00\"}\n".to_owned(),
        "{\"event\":\"visit\",\"stage\":\"applicant_no_slots\",\"document\":\"d\",\"time\":5}\n".to_owned(),
        "{\"event\":\"visit\",\"stage\":\"applicant_no_slots\",\"document\":\"d\",\"time\":\"yesterday\"}\n".to_owned(),
        visit("good", "2026-10-30T10:00:00+00:00"),
    ];
    fs::write(journal(root.path(), "fixture-a"), rows.concat()).unwrap();
    fs::write(journal(root.path(), "other-a"), visit("foreign", "2026-10-30T10:00:00+00:00")).unwrap();
    assert_eq!(restore_applicant_notices(&mut db, "fixture", root.path()).unwrap(), 1);
    assert_eq!(notices(&db).len(), 1);
    assert_eq!(notices(&db)[0].1, digest("good"));
}

#[test]
fn documents_are_stored_only_as_digests() {
    let (mut db, root) = (database(), tempfile::tempdir().unwrap());
    fs::write(journal(root.path(), "fixture-a"), visit("PRIVATE-DOCUMENT-ID", "2026-10-30T10:00:00+00:00")).unwrap();
    restore_applicant_notices(&mut db, "fixture", root.path()).unwrap();
    let dump: String = db
        .query_row("SELECT group_concat(document, ' ') FROM pending_applicant_notices", [], |r| r.get(0))
        .unwrap();
    assert!(!dump.contains("PRIVATE") && dump.contains(&digest("PRIVATE-DOCUMENT-ID")));
}

#[test]
fn file_restore_needs_an_existing_migrated_database_and_never_creates_one() {
    use ceh_store::notices::restore_applicant_notices_file;
    let root = tempfile::tempdir().unwrap();
    fs::write(journal(root.path(), "fixture-a"), visit("doc-a", "2026-10-30T10:00:00+00:00")).unwrap();
    let path = root.path().join("stats.sqlite3");
    assert_eq!(restore_applicant_notices_file(&path, "fixture", root.path()), Ok(0));
    assert!(!path.exists(), "no database is created");
    // A database from before the notice tables existed is left alone.
    let old = Connection::open(&path).unwrap();
    old.execute_batch("CREATE TABLE metadata(key TEXT PRIMARY KEY,value TEXT);").unwrap();
    drop(old);
    assert_eq!(restore_applicant_notices_file(&path, "fixture", root.path()), Ok(0));
    let db = Connection::open(&path).unwrap();
    let tables: i64 = db.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table'", [], |r| r.get(0)).unwrap();
    assert_eq!(tables, 1, "an un-migrated database is not changed");
    drop(db);
    // A migrated one is restored.
    fs::remove_file(&path).unwrap();
    let db = database_at(&path);
    session(&db, "s1", "2026-10-30T09:00:00+00:00", None);
    drop(db);
    assert_eq!(restore_applicant_notices_file(&path, "fixture", root.path()), Ok(1));
}

fn database_at(path: &Path) -> Connection {
    let db = Connection::open(path).unwrap();
    db.execute_batch(include_str!("../src/schema.sql")).unwrap();
    db
}
