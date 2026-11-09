//! The statistics command restores applicant notices from journals into an existing migrated database.
use ceh_store::migrate::{MigrationOptions, migrate_paths};
use std::{fs, path::Path};

fn runtime_with_profile_and_session() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("tasks.yml"), "tasks:\n  - id: fixture\n    label: Fixture\n").unwrap();
    let database = root.path().join("data/profile-statistics.sqlite3");
    migrate_paths(
        &MigrationOptions {
            database: database.clone(),
            legacy_json: None,
            backup_directory: root.path().join("data/backups"),
        },
        1793275200000,
    )
    .unwrap();
    let db = rusqlite::Connection::open(&database).unwrap();
    db.execute("INSERT INTO sessions(profile,session,start) VALUES ('fixture','s1','2026-10-30T09:00:00+00:00')", []).unwrap();
    root
}
fn journal(root: &Path, folder: &str) {
    let directory = root.join(folder).join("sessions/fixture-20261030");
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join("journal.jsonl"),
        "{\"event\":\"visit\",\"stage\":\"applicant_no_slots\",\"document\":\"doc-a\",\"time\":\"2026-10-30T11:00:00.123+01:00\"}\n",
    )
    .unwrap();
}
fn banners(root: &Path) -> i64 {
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-30T12:00:00Z").unwrap().to_utc();
    ceh_desktop::profile_statistics_at(root, "fixture", now).unwrap().unwrap().all["banners"].as_i64().unwrap()
}

#[test]
fn python_journals_under_debug_are_restored_into_banners_once() {
    let root = runtime_with_profile_and_session();
    journal(root.path(), "debug");
    assert_eq!(banners(root.path()), 1, "a journal visit with a local offset and milliseconds becomes a banner");
    assert_eq!(banners(root.path()), 1, "opening the statistics again adds nothing");
}

#[test]
fn rust_diagnostics_journals_are_restored_too() {
    let root = runtime_with_profile_and_session();
    journal(root.path(), "data/diagnostics");
    assert_eq!(banners(root.path()), 1);
}

#[test]
fn without_a_database_the_statistics_command_creates_nothing() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("tasks.yml"), "tasks:\n  - id: fixture\n    label: Fixture\n").unwrap();
    journal(root.path(), "debug");
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-30T12:00:00Z").unwrap().to_utc();
    assert!(ceh_desktop::profile_statistics_at(root.path(), "fixture", now).unwrap().is_none());
    assert!(!root.path().join("data").exists());
}
