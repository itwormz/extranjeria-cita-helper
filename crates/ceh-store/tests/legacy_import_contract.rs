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
            db.execute("INSERT INTO episodes(profile,session,kind,stage,mode,document,start,end,last_seen,confirmed) VALUES (?2,'s','page','x','auto',?1,'a','b','c',1)", rusqlite::params![format!("d{n}"),events.first().map_or("none", |e|e.0)]).unwrap();
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


mod beta2_regressions {
    use super::*;
    use ceh_store::appointment_budget::BudgetStore;
    use ceh_core::appointment_budget::AppointmentPolicy;
    use rusqlite::Connection;

    fn profile(root: &Path) -> ceh_core::profiles::ProfileDraft {
        ceh_store::profiles::read_profiles(&root.join("tasks.yml"), &catalog(), "en").unwrap().profiles[0].validation.normalized.clone()
    }
    #[test]
    fn older_day_import_preserves_active_restriction_without_replacing_todays_budget() {
        let (source,target)=(tempfile::tempdir().unwrap(),tempfile::tempdir().unwrap());
        write(source.path(),"tasks.yml",profiles_yaml(&["old"],"").as_bytes());
        let person=profile(source.path());let policy=AppointmentPolicy::default();
        let yesterday="2026-11-12T20:00:00Z".parse().unwrap();let today="2026-11-13T10:00:00Z".parse().unwrap();
        let old=BudgetStore::open(&source.path().join("data"),&person).unwrap();let mut history=old.load().unwrap();
        history.submitted(yesterday,&policy);history.cod_oper(yesterday,&policy);
        history.office_during_restriction(yesterday,vec!["Synthetic office".into()]);old.save(&history).unwrap();drop(old);
        let own=BudgetStore::open(&target.path().join("data"),&person).unwrap();let mut current=own.load().unwrap();
        for _ in 0..2 {current.submitted(today,&policy);}current.set_daily_target(Some(40),today,&policy).unwrap();own.save(&current).unwrap();drop(own);
        import(source.path(),target.path()).unwrap();
        for _ in 0..2 {
            let store=BudgetStore::open(&target.path().join("data"),&person).unwrap();let mut budget=store.load().unwrap();
            let summary=budget.summary(today,&policy,false);
            assert!(summary.restricted,"active restriction survives midnight and consumed-snapshot reopen");
            assert_eq!(summary.first_cod_oper,Some(yesterday));assert_eq!(summary.last_cod_oper,Some(yesterday));
            assert_eq!(summary.sent,2);assert_eq!(summary.manual_target,Some(40));
            assert_eq!(budget.offices_during_restriction,1);assert_eq!(summary.episode_offices.len(),1);
        }
    }
    #[test]
    fn newer_day_import_respects_recovery_history_and_keeps_a_genuinely_new_restriction() {
        let (source,target)=(tempfile::tempdir().unwrap(),tempfile::tempdir().unwrap());
        write(source.path(),"tasks.yml",profiles_yaml(&["old"],"").as_bytes());
        let person=profile(source.path());let policy=AppointmentPolicy::default();
        let yesterday="2026-11-12T20:00:00Z".parse().unwrap();let today="2026-11-13T10:00:00Z".parse().unwrap();
        let recovered="2026-11-13T09:00:00Z".parse().unwrap();
        for root in [source.path(),target.path()] {
            let store=BudgetStore::open(&root.join("data"),&person).unwrap();let mut budget=store.load().unwrap();
            budget.submitted(yesterday,&policy);budget.cod_oper(yesterday,&policy);
            budget.office_during_restriction(yesterday,vec!["Old office".into()]);
            if root==source.path() {budget.submitted(today,&policy);} else {budget.ordinary_result(recovered);}
            store.save(&budget).unwrap();
        }
        import(source.path(),target.path()).unwrap();
        let store=BudgetStore::open(&target.path().join("data"),&person).unwrap();let mut merged=store.load().unwrap();
        assert!(!merged.restricted(),"newer daily snapshot cannot revive an already recovered restriction");
        assert!(merged.episode_offices.is_empty());assert_eq!(merged.episode_history.len(),1);
        assert_eq!(merged.summary(today,&policy,false).sent,1);drop(store);
        let store=BudgetStore::open(&source.path().join("data"),&person).unwrap();let mut updated=store.load().unwrap();
        updated.cod_oper(today,&policy);store.save(&updated).unwrap();drop(store);
        import(source.path(),target.path()).unwrap();
        let store=BudgetStore::open(&target.path().join("data"),&person).unwrap();let mut merged=store.load().unwrap();
        assert!(merged.restricted());assert_eq!(merged.summary(today,&policy,false).last_cod_oper,Some(today));
        assert_eq!(merged.summary(today,&policy,false).first_cod_oper,Some(today));
        assert!(merged.episode_offices.is_empty(),"offices before local recovery belong to the closed episode");
    }
    #[test]
    fn recovery_from_an_older_daily_snapshot_closes_the_current_active_episode() {
        let (source,target)=(tempfile::tempdir().unwrap(),tempfile::tempdir().unwrap());
        write(source.path(),"tasks.yml",profiles_yaml(&["old"],"").as_bytes());
        let person=profile(source.path());let policy=AppointmentPolicy::default();
        let yesterday="2026-11-12T20:00:00Z".parse().unwrap();let today="2026-11-13T10:00:00Z".parse().unwrap();
        for root in [source.path(),target.path()] {
            let store=BudgetStore::open(&root.join("data"),&person).unwrap();let mut budget=store.load().unwrap();
            budget.submitted(yesterday,&policy);budget.cod_oper(yesterday,&policy);
            if root==source.path() {budget.ordinary_result(today);} else {budget.submitted(today,&policy);}
            store.save(&budget).unwrap();
        }
        import(source.path(),target.path()).unwrap();
        let store=BudgetStore::open(&target.path().join("data"),&person).unwrap();let mut merged=store.load().unwrap();
        assert!(!merged.restricted());assert!(merged.episode_offices.is_empty());
        assert_eq!(merged.episode_history.len(),1);assert_eq!(merged.summary(today,&policy,false).sent,1);
    }
    #[test]
    fn mixed_budget_keys_keep_imported_document_state_reachable_without_replacing_current_state() {
        let (source, target) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        write(source.path(), "tasks.yml", profiles_yaml(&["old"], "").as_bytes());
        let person = profile(source.path());
        let source_store = BudgetStore::open(&source.path().join("data"), &person).unwrap();
        let mut budget = source_store.load().unwrap();
        budget.submitted("2026-11-12T10:00:00Z".parse().unwrap(), &AppointmentPolicy::default());
        source_store.save(&budget).unwrap();
        drop(source_store);
        // The target already has another document and a different salt.
        let mut another = person.clone(); another.id = "other".into(); another.document_number = "X1234567L".into();
        let target_store = BudgetStore::open(&target.path().join("data"), &another).unwrap();
        let mut own = target_store.load().unwrap();
        for _ in 0..3 { own.submitted("2026-11-12T10:00:00Z".parse().unwrap(), &AppointmentPolicy::default()); }
        target_store.save(&own).unwrap(); drop(target_store);
        let salt = fs::read(target.path().join("data/appointment-budget/key")).unwrap();
        import(source.path(), target.path()).unwrap();
        assert_eq!(fs::read(target.path().join("data/appointment-budget/key")).unwrap(), salt);
        let imported = BudgetStore::open(&target.path().join("data"), &person).unwrap();
        assert!(imported.exists()); assert_eq!(imported.load().unwrap().sent(), 1);
        // A second profile with copied identity must lock the exact same file.
        let mut copied = person.clone(); copied.fill_mode = "copy".into(); copied.id = "copied".into();
        assert!(BudgetStore::open_for_document(&target.path().join("data"), &copied, Some((&person.document_type,&person.document_number))).is_err());
        drop(imported);
        let copied = BudgetStore::open_for_document(&target.path().join("data"), &copied, Some((&person.document_type,&person.document_number))).unwrap();
        assert_eq!(copied.load().unwrap().sent(), 1); drop(copied);
        let copied_profile=ceh_core::profiles::ProfileDraft {fill_mode:"copy".into(),id:"copied".into(),..person.clone()};
        let saved=BudgetStore::open_saved(&target.path().join("data"),&copied_profile).unwrap();
        assert!(BudgetStore::open(&target.path().join("data"),&person).is_err(),"editing saved copy budget locks the same canonical current identity");drop(saved);
        assert_eq!(BudgetStore::open(&target.path().join("data"), &another).unwrap().load().unwrap().sent(), 3);
    }
    #[test]
    fn mixed_states_keep_maximum_observations_latest_restriction_and_local_manual_choice() {
        let (source,target)=(tempfile::tempdir().unwrap(),tempfile::tempdir().unwrap());
        write(source.path(),"tasks.yml",profiles_yaml(&["old"],"").as_bytes());let person=profile(source.path());
        let policy=AppointmentPolicy::default();let at="2026-11-12T10:00:00Z".parse().unwrap();
        let old=BudgetStore::open(&source.path().join("data"),&person).unwrap();let mut history=old.load().unwrap();
        for _ in 0..3 {history.submitted(at,&policy);}history.cod_oper(at,&policy);old.save(&history).unwrap();drop(old);
        let own=BudgetStore::open(&target.path().join("data"),&person).unwrap();let mut current=own.load().unwrap();current.submitted(at,&policy);current.set_daily_target(Some(40),at,&policy).unwrap();own.save(&current).unwrap();drop(own);
        import(source.path(),target.path()).unwrap();
        let store=BudgetStore::open(&target.path().join("data"),&person).unwrap();let mut merged=store.load().unwrap();
        assert_eq!(merged.sent(),3);assert_eq!(merged.summary(at,&policy,false).manual_target,Some(40));assert!(merged.restricted());
        store.save(&merged).unwrap();drop(store);
        import(source.path(),target.path()).unwrap();let store=BudgetStore::open(&target.path().join("data"),&person).unwrap();assert_eq!(store.load().unwrap().sent(),3,"reimport never adds the shared history again");
        let mut recovered=store.load().unwrap();recovered.ordinary_result("2026-11-12T11:00:00Z".parse().unwrap());
        assert!(!recovered.restricted());store.save(&recovered).unwrap();drop(store);
        let store=BudgetStore::open(&target.path().join("data"),&person).unwrap();
        assert!(!store.load().unwrap().restricted(),"a consumed imported restriction never returns after successful recovery");
        assert!(!BudgetStore::load_profile(&target.path().join("data"),&person.id).unwrap().unwrap().restricted(),"read-only dashboard agrees with canonical state");
        drop(store);
        let source_store=BudgetStore::open(&source.path().join("data"),&person).unwrap();let mut grown=source_store.load().unwrap();
        grown.submitted("2026-11-12T10:30:00Z".parse().unwrap(),&policy);source_store.save(&grown).unwrap();drop(source_store);
        import(source.path(),target.path()).unwrap();let store=BudgetStore::open(&target.path().join("data"),&person).unwrap();
        assert_eq!(store.load().unwrap().sent(),4,"changed source snapshots refresh used attempts");
        assert!(!store.load().unwrap().restricted(),"a changed old snapshot cannot resurrect a restriction recovered later locally");drop(store);
        let source_store=BudgetStore::open(&source.path().join("data"),&person).unwrap();let mut blocked=source_store.load().unwrap();blocked.cod_oper("2026-11-12T12:00:00Z".parse().unwrap(),&policy);source_store.save(&blocked).unwrap();drop(source_store);
        import(source.path(),target.path()).unwrap();let store=BudgetStore::open(&target.path().join("data"),&person).unwrap();
        assert!(store.load().unwrap().restricted(),"a genuinely later source restriction is imported");


    }
    #[test]
    fn same_salt_budget_updates_preserve_local_choices_and_do_not_write_through_active_locks() {
        let (source,target)=(tempfile::tempdir().unwrap(),tempfile::tempdir().unwrap());
        write(source.path(),"tasks.yml",profiles_yaml(&["old"],"").as_bytes());let person=profile(source.path());
        let policy=AppointmentPolicy::default();let at="2026-11-12T10:00:00Z".parse().unwrap();
        let old=BudgetStore::open(&source.path().join("data"),&person).unwrap();let mut history=old.load().unwrap();history.submitted(at,&policy);old.save(&history).unwrap();drop(old);
        import(source.path(),target.path()).unwrap();
        let own=BudgetStore::open(&target.path().join("data"),&person).unwrap();let mut local=own.load().unwrap();local.set_daily_target(Some(40),at,&policy).unwrap();own.save(&local).unwrap();
        let old=BudgetStore::open(&source.path().join("data"),&person).unwrap();let mut history=old.load().unwrap();history.submitted(at,&policy);history.submitted(at,&policy);old.save(&history).unwrap();drop(old);
        assert!(import(source.path(),target.path()).is_err(),"an import never overwrites the budget owned by an active search");
        assert_eq!(own.load().unwrap().sent(),1);drop(own);
        import(source.path(),target.path()).unwrap();let own=BudgetStore::open(&target.path().join("data"),&person).unwrap();let mut result=own.load().unwrap();assert_eq!(result.sent(),3);assert_eq!(result.summary(at,&policy,false).manual_target,Some(40));
    }
    fn db(root: &Path, rich: bool) -> Connection {
        fs::create_dir_all(root.join("data")).unwrap();
        let db = Connection::open(root.join("data/profile-statistics.sqlite3")).unwrap();
        db.execute_batch(include_str!("../src/schema.sql")).unwrap();
        if rich { db.execute_batch("ALTER TABLE episodes ADD COLUMN recognition TEXT DEFAULT 'known'; ALTER TABLE episodes ADD COLUMN evidence TEXT DEFAULT '[]'; ALTER TABLE episodes ADD COLUMN variant TEXT DEFAULT '';").unwrap(); }
        db
    }
    #[test]
    fn growing_source_enriches_an_episode_without_duplication_or_losing_evidence() {
        let (source, target) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        write(source.path(), "tasks.yml", b"profiles: []\n");
        let old = db(source.path(), true); let current = db(target.path(), true);
        old.execute("INSERT INTO episodes(profile,session,kind,stage,mode,document,start,last_seen,evidence,recognition,variant) VALUES ('p','s','page','no_slots','auto','doc','2026-11-12T10:00:00Z','2026-11-12T10:00:01Z','[\"operation_code\"]','known','special')", []).unwrap();
        import(source.path(), target.path()).unwrap();
        let evidence: (String,String) = current.query_row("SELECT evidence,variant FROM episodes",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(evidence,("[\"operation_code\"]".into(),"special".into()));
        old.execute("UPDATE episodes SET end='2026-11-12T10:01:00Z',last_seen='2026-11-12T10:01:00Z',confirmed=1", []).unwrap();
        old.execute("INSERT INTO metadata(key,value) VALUES ('changed',?1)",["a".repeat(6000)]).unwrap();
        import(source.path(), target.path()).unwrap();
        let row: (i64,String,i64) = current.query_row("SELECT COUNT(*),MAX(last_seen),MAX(confirmed) FROM episodes",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        assert_eq!(row,(1,"2026-11-12T10:01:00Z".into(),1));
    }
    #[test]
    fn old_episode_schema_is_supported_and_oversized_journal_is_refused_before_profiles_are_copied() {
        let (source, target) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        write(source.path(), "tasks.yml", b"profiles: []\n");
        let old = db(source.path(), false); let current = db(target.path(), false);
        old.execute("INSERT INTO episodes(profile,session,kind,start) VALUES ('p','s','page','a')",[]).unwrap();
        import(source.path(), target.path()).unwrap();
        assert_eq!(current.query_row("SELECT COUNT(*) FROM episodes",[],|r|r.get::<_,i64>(0)).unwrap(),1);
        let target = tempfile::tempdir().unwrap();
        let journal=source.path().join("debug/sessions/large/journal.jsonl"); fs::create_dir_all(journal.parent().unwrap()).unwrap();
        fs::File::create(journal).unwrap().set_len(65*1024*1024).unwrap();
        assert_eq!(import(source.path(),target.path()),Err(ImportError::Invalid));
        assert!(!target.path().join("tasks.yml").exists());
    }
}
