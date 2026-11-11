//! The very first start of the installed application brings the Python version's data over by itself.
use ceh_desktop::{auto_import, catalog};
use std::fs;

fn python_folder(support: &std::path::Path) -> std::path::PathBuf {
    let folder = support.join("Cita Extranjeria Helper");
    fs::create_dir_all(folder.join("data")).unwrap();
    fs::write(folder.join("tasks.yml"), "profiles: []\n").unwrap();
    fs::write(folder.join("data/achievements.json"), "{}").unwrap();
    folder
}

#[test]
fn the_old_data_is_imported_on_the_first_start_and_never_again() {
    let support = tempfile::tempdir().unwrap();
    python_folder(support.path());
    let root = support.path().join("SantaCita");
    assert_eq!(auto_import(&root, &[support.path().to_owned()], catalog()), Some("Cita Extranjeria Helper".to_owned()));
    assert!(root.join("tasks.yml").is_file() && root.join("data/achievements.json").is_file());
    // The old folder is left exactly as it was.
    assert_eq!(fs::read_to_string(support.path().join("Cita Extranjeria Helper/tasks.yml")).unwrap(), "profiles: []\n");
    // The new app now has its own profile list, so a later start does not touch anything.
    fs::write(root.join("tasks.yml"), "tasks: []\n").unwrap();
    assert_eq!(auto_import(&root, &[support.path().to_owned()], catalog()), None);
    assert_eq!(fs::read_to_string(root.join("tasks.yml")).unwrap(), "tasks: []\n");
}

#[test]
fn nothing_is_created_without_an_old_folder_and_the_own_folder_is_never_a_source() {
    let support = tempfile::tempdir().unwrap();
    let root = support.path().join("SantaCita");
    assert_eq!(auto_import(&root, &[support.path().to_owned()], catalog()), None);
    assert!(!root.exists());
    // A folder with the new app's own name is never imported into itself.
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("tasks.yml.tmp"), "x").unwrap();
    fs::remove_file(root.join("tasks.yml.tmp")).unwrap();
    assert_eq!(auto_import(&root, &[support.path().to_owned()], catalog()), None);
}

#[test]
fn a_database_that_is_still_in_use_postpones_the_import_without_a_trace() {
    let support = tempfile::tempdir().unwrap();
    let old = python_folder(support.path());
    fs::write(old.join("data/profile-statistics.sqlite3"), b"SQLite format 3\0").unwrap();
    fs::write(old.join("data/profile-statistics.sqlite3-wal"), b"pending").unwrap();
    let root = support.path().join("SantaCita");
    assert_eq!(auto_import(&root, &[support.path().to_owned()], catalog()), None);
    assert!(!root.join("tasks.yml").exists());
}
