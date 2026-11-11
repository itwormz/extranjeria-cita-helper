use ceh_core::profiles::ProfileCatalog;
use ceh_store::legacy_import::{ImportError, Report, candidates, import as import_with};
fn catalog() -> ProfileCatalog {
    ProfileCatalog { provinces: vec!["Fixture Province".into()], nationalities: vec!["FIXTURE NATIONALITY".into()], barcelona_offices: vec![], timezones: vec!["Europe/Madrid".into(), "UTC".into()] }
}
fn import(source: &Path, target: &Path) -> Result<Report, ImportError> {
    import_with(source, target, &catalog())
}
fn profiles_yaml(ids: &[&str], extra: &str) -> String {
    let mut text = String::from("tasks:\n");
    for id in ids {
        text += &format!("- id: {id}\n  label: {id}\n  request:\n    province: Fixture Province\n    procedure: POLICÍA-TOMA DE HUELLAS (EXPEDICIÓN DE TARJETA) INICIAL, RENOVACIÓN, DUPLICADO Y LEY 14/2013\n    access_mode: certificate\n  applicant:\n    document_type: NIE\n    document_number: X0000000T\n    full_name: FICTITIOUS PERSON\n    nationality: FIXTURE NATIONALITY\n    contact:\n      phone: '600000000'\n      email: fixture@example.test\n");
    }
    text + extra
}
fn profile_ids(root: &Path) -> Vec<String> {
    let bundle = ceh_store::profiles::read_profiles(&root.join("tasks.yml"), &catalog(), "en").unwrap();
    bundle.profiles.iter().map(|p| p.validation.normalized.id.clone()).collect()
}
use std::{fs, path::Path};

fn write(root: &Path, relative: &str, bytes: &[u8]) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}
fn python_runtime(root: &Path) {
    write(root, "tasks.yml", b"profiles: []\n");
    write(root, "data/achievements.json", b"{}");
    write(root, "data/profile-statistics.sqlite3", b"SQLite format 3\0history");
    write(root, "data/profile-statistics.json", b"{}");
    write(root, "data/state.db", b"state");
    write(root, "data/appointment-budget/key", &[7u8; 32]);
    write(root, "data/appointment-budget/abc.json", b"{\"sent\":1}");
    write(root, "debug/sessions/p-1/journal.jsonl", b"{\"a\":1}\n");
    write(root, "debug/sessions/p-1/0001.html", b"<html>private page</html>");
    write(root, "debug/sessions/p-1/0001.png", b"png");
    write(root, "debug/resources/app.js", b"script");
    write(root, "browser-profiles/firefox-p/cookies.sqlite", b"cookies");
}
fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() { walk(&path, base, out) } else { out.push((path.strip_prefix(base).unwrap().display().to_string(), fs::read(&path).unwrap())) }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

#[test]
fn profiles_statistics_budget_and_journals_are_copied_and_the_source_is_untouched() {
    let source = tempfile::tempdir().unwrap();
    python_runtime(source.path());
    let before = snapshot(source.path());
    let target = tempfile::tempdir().unwrap();
    let report = import(source.path(), target.path()).unwrap();
    assert_eq!(snapshot(source.path()), before, "the old data is never modified");
    for expected in ["tasks.yml", "data/achievements.json", "data/profile-statistics.sqlite3", "data/state.db",
        "data/appointment-budget/key", "data/appointment-budget/abc.json", "debug/sessions/p-1/journal.jsonl"] {
        assert!(report.copied.contains(&expected.to_owned()), "{expected}");
        assert_eq!(fs::read(target.path().join(expected)).unwrap(), fs::read(source.path().join(expected)).unwrap());
    }
    // Private page snapshots, screenshots, cached scripts and the old browser profile stay behind.
    for left in ["debug/sessions/p-1/0001.html", "debug/sessions/p-1/0001.png", "debug/resources/app.js", "browser-profiles/firefox-p/cookies.sqlite", "data/profile-statistics.json"] {
        assert!(!target.path().join(left).exists(), "{left}");
    }
    assert!(report.kept.is_empty());
}

#[test]
fn existing_data_files_are_never_overwritten_and_a_second_import_changes_nothing() {
    let source = tempfile::tempdir().unwrap();
    python_runtime(source.path());
    let target = tempfile::tempdir().unwrap();
    write(target.path(), "data/achievements.json", b"{\"mine\":1}");
    let first = import(source.path(), target.path()).unwrap();
    assert_eq!(fs::read(target.path().join("data/achievements.json")).unwrap(), b"{\"mine\":1}");
    assert_eq!(first.merged, vec!["data/achievements.json".to_owned()], "nothing to add: the file stays as it is");
    let after_first = snapshot(target.path());
    let second = import(source.path(), target.path()).unwrap();
    assert!(second.copied.is_empty());
    assert_eq!(snapshot(target.path()), after_first);
}

#[test]
fn a_database_in_use_a_missing_profile_file_and_links_are_refused_or_skipped() {
    let source = tempfile::tempdir().unwrap();
    python_runtime(source.path());
    write(source.path(), "data/profile-statistics.sqlite3-wal", b"pending writes");
    assert_eq!(import(source.path(), tempfile::tempdir().unwrap().path()), Err(ImportError::InUse));
    fs::remove_file(source.path().join("data/profile-statistics.sqlite3-wal")).unwrap();
    // Journals that are symlinks are not followed.
    #[cfg(unix)]
    {
        let secret = tempfile::tempdir().unwrap();
        write(secret.path(), "journal.jsonl", b"secret");
        fs::remove_file(source.path().join("debug/sessions/p-1/journal.jsonl")).unwrap();
        std::os::unix::fs::symlink(secret.path().join("journal.jsonl"), source.path().join("debug/sessions/p-1/journal.jsonl")).unwrap();
        let target = tempfile::tempdir().unwrap();
        let report = import(source.path(), target.path()).unwrap();
        assert!(!report.copied.iter().any(|p| p.ends_with("journal.jsonl")));
        assert!(!target.path().join("debug/sessions/p-1/journal.jsonl").exists());
    }
    let empty = tempfile::tempdir().unwrap();
    assert_eq!(import(empty.path(), tempfile::tempdir().unwrap().path()), Err(ImportError::NoProfiles));
    assert_eq!(import(&source.path().join("nope"), tempfile::tempdir().unwrap().path()), Err(ImportError::NoProfiles));
}

#[test]
fn candidates_are_the_known_application_folders_that_hold_profiles() {
    let support = tempfile::tempdir().unwrap();
    write(&support.path().join("Cita Extranjeria Helper"), "tasks.yml", b"x");
    write(&support.path().join("SantaCita"), "other.txt", b"x");
    write(&support.path().join("Cita Extranjeria Helper Experimental"), "tasks.yml", b"x");
    write(&support.path().join("Unrelated"), "tasks.yml", b"x");
    let found = candidates(&[support.path().to_owned()]);
    assert_eq!(found.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["Cita Extranjeria Helper"]);
    assert_eq!(found[0].path(), support.path().join("Cita Extranjeria Helper"));
}

mod merging {
    use super::*;
    use rusqlite::Connection;

    fn database(path: &Path, events: &[(&str, &str)], episodes: u32) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let db = Connection::open(path).unwrap();
        db.execute_batch(include_str!("../src/schema.sql")).unwrap();
        for (profile, identity) in events {
            db.execute("INSERT INTO events(profile,identity,kind,time,session,stage,mode,counted,offices,evidence) VALUES (?1,?2,'check','2026-10-01T10:00:00+00:00','s','x','auto',1,'','')", [profile, identity]).unwrap();
        }
        for n in 0..episodes {
            db.execute("INSERT INTO episodes(profile,session,kind,stage,mode,document,start,end,last_seen,confirmed) VALUES ('old','s','page','x','auto',?1,'a','b','c',1)", [format!("d{n}")]).unwrap();
        }
        db.execute("INSERT OR IGNORE INTO baseline(profile,data) VALUES (?1,'{}')", [events.first().map_or("none", |e| e.0)]).unwrap();
    }
    fn count(path: &Path, table: &str) -> i64 {
        Connection::open(path).unwrap().query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn the_old_statistics_are_merged_into_the_new_database_once_and_the_new_one_is_backed_up() {
        let (source, target) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        write(source.path(), "tasks.yml", b"profiles: []\n");
        database(&source.path().join("data/profile-statistics.sqlite3"), &[("old", "e1"), ("old", "e2")], 3);
        database(&target.path().join("data/profile-statistics.sqlite3"), &[("new", "n1")], 1);
        let report = import(source.path(), target.path()).unwrap();
        assert!(report.merged.iter().any(|f| f.ends_with("profile-statistics.sqlite3")), "{report:?}");
        let db = target.path().join("data/profile-statistics.sqlite3");
        assert_eq!(count(&db, "events"), 3, "old and new events side by side");
        assert_eq!(count(&db, "episodes"), 4);
        assert_eq!(count(&db, "baseline"), 2);
        assert!(target.path().join("data/profile-statistics.sqlite3.before-import").exists(), "the new database is backed up first");
        // Importing the same folder again adds nothing.
        import(source.path(), target.path()).unwrap();
        assert_eq!(count(&db, "events"), 3);
        assert_eq!(count(&db, "episodes"), 4);
    }

    #[test]
    fn achievements_are_merged_by_id_and_the_new_file_is_backed_up() {
        let (source, target) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        write(source.path(), "tasks.yml", b"profiles: []\n");
        write(source.path(), "data/achievements.json", br#"{"person_a":[{"id":"1","x":"old"}],"person_b":[{"id":"2"}]}"#);
        write(target.path(), "data/achievements.json", br#"{"person_a":[{"id":"1","x":"new"},{"id":"3"}],"person_c":[{"id":"4"}]}"#);
        import(source.path(), target.path()).unwrap();
        let merged: serde_json::Value = serde_json::from_slice(&fs::read(target.path().join("data/achievements.json")).unwrap()).unwrap();
        let ids = |key: &str| merged[key].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
        assert_eq!(ids("person_a"), ["1", "3"], "an id that exists keeps the new app's record");
        assert_eq!(ids("person_b"), ["2"]);
        assert_eq!(ids("person_c"), ["4"]);
        assert!(target.path().join("data/achievements.json.before-import").exists());
    }
}

mod profile_merge {
    use super::*;

    #[test]
    fn old_profiles_missing_from_the_new_app_are_added_and_the_new_apps_own_stay_exactly_as_they_are() {
        let (source, target) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        write(source.path(), "tasks.yml", profiles_yaml(&["a", "b"], "").as_bytes());
        let mine = profiles_yaml(&["b", "c"], "desktop:\n  dark: true\n  language: es\n").replace("label: b", "label: Mine");
        write(target.path(), "tasks.yml", mine.as_bytes());
        let report = import(source.path(), target.path()).unwrap();
        assert_eq!(profile_ids(target.path()), ["b", "c", "a"]);
        let text = fs::read_to_string(target.path().join("tasks.yml")).unwrap();
        assert!(text.contains("Mine") && text.contains("language") && text.contains("es"), "{text}");
        assert!(report.merged.contains(&"tasks.yml".to_owned()), "{report:?}");
        assert_eq!(fs::read_to_string(target.path().join("tasks.yml.before-import")).unwrap(), mine, "the new list is backed up first");
        // Importing the same folder again adds nothing and leaves the file alone.
        let after = fs::read(target.path().join("tasks.yml")).unwrap();
        let second = import(source.path(), target.path()).unwrap();
        assert_eq!(fs::read(target.path().join("tasks.yml")).unwrap(), after);
        assert!(!second.merged.contains(&"tasks.yml".to_owned()));
        assert!(!target.path().join("tasks.yml.before-import.2").exists());
    }

    #[test]
    fn nothing_to_add_changes_nothing_and_a_damaged_new_list_is_never_touched() {
        let (source, target) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        write(source.path(), "tasks.yml", profiles_yaml(&["a"], "").as_bytes());
        write(target.path(), "tasks.yml", profiles_yaml(&["a"], "").as_bytes());
        import(source.path(), target.path()).unwrap();
        assert!(!target.path().join("tasks.yml.before-import").exists());
        write(target.path(), "tasks.yml", b"tasks: [SECRET-DONT-ECHO");
        let report = import(source.path(), target.path()).unwrap();
        assert_eq!(fs::read(target.path().join("tasks.yml")).unwrap(), b"tasks: [SECRET-DONT-ECHO");
        assert!(report.kept.contains(&"tasks.yml".to_owned()));
    }
}
