use ceh_desktop::journal_log::{load_history, JournalLog};
use serde_json::json;

fn row(id: u64, ended: Option<i64>) -> serde_json::Value {
    json!({"id": id, "time": 1000 + id as i64, "ended_at": ended, "event": "visit"})
}

#[test]
fn rows_of_a_finished_search_come_back_for_the_profile_oldest_first() {
    let root = tempfile::tempdir().unwrap();
    let mut log = JournalLog::open(root.path(), "profile-a", 100).unwrap();
    log.record(&[row(1, None), row(2, None)]);
    let mut other = JournalLog::open(root.path(), "profile-b", 100).unwrap();
    other.record(&[row(9, None)]);
    let history = load_history(root.path(), "profile-a", 250);
    assert_eq!(history.iter().map(|r| r["id"].as_u64().unwrap()).collect::<Vec<_>>(), [1, 2]);
}

#[test]
fn a_row_that_changes_later_is_stored_again_and_the_last_version_wins() {
    let root = tempfile::tempdir().unwrap();
    let mut log = JournalLog::open(root.path(), "p", 100).unwrap();
    log.record(&[row(1, None)]);
    log.record(&[row(1, None)]);
    log.record(&[row(1, Some(5)), row(2, None)]);
    let history = load_history(root.path(), "p", 250);
    assert_eq!(history.len(), 2);
    assert_eq!(history[0]["ended_at"], 5);
    let text = std::fs::read_to_string(std::fs::read_dir(root.path().join("data/journal/p")).unwrap().next().unwrap().unwrap().path()).unwrap();
    assert_eq!(text.lines().count(), 3, "an unchanged row is not written twice");
}

#[test]
fn history_spans_searches_newest_last_and_is_capped() {
    let root = tempfile::tempdir().unwrap();
    for (start, base) in [(100, 0u64), (200, 1000)] {
        let mut log = JournalLog::open(root.path(), "p", start).unwrap();
        let rows: Vec<_> = (1..=200).map(|i| row(base + i, None)).collect();
        log.record(&rows);
    }
    let history = load_history(root.path(), "p", 250);
    assert_eq!(history.len(), 250);
    assert_eq!(history.last().unwrap()["id"], 1200, "the newest row is last");
    assert_eq!(history.first().unwrap()["id"], 151, "the 250 newest rows: 200 of the newer search, the last 50 of the older");
}

#[test]
fn a_profile_id_cannot_leave_the_journal_folder_and_old_searches_are_pruned() {
    let root = tempfile::tempdir().unwrap();
    assert!(JournalLog::open(root.path(), "../escape", 1).is_none());
    assert!(JournalLog::open(root.path(), "", 1).is_none());
    assert!(load_history(root.path(), "../escape", 10).is_empty());
    for start in 1..=30 {
        JournalLog::open(root.path(), "p", start).unwrap().record(&[row(start as u64, None)]);
    }
    let files = std::fs::read_dir(root.path().join("data/journal/p")).unwrap().count();
    assert_eq!(files, 20);
    assert_eq!(load_history(root.path(), "p", 250).last().unwrap()["id"], 30);
}

#[test]
fn row_ids_stay_unique_across_searches() {
    let root = tempfile::tempdir().unwrap();
    for start in [1_000_000u64, 2_000_000] {
        JournalLog::open(root.path(), "p", start).unwrap().record(&[row(1, None)]);
    }
    let ids: Vec<_> = load_history(root.path(), "p", 250).iter().map(|r| r["id"].as_u64().unwrap()).collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
    assert!(ids.iter().all(|id| *id < (1u64 << 53)));
}
