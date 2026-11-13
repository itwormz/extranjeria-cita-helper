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

mod portable {
    use ceh_desktop::portable_data_root;
    use std::fs;

    #[test]
    fn a_portable_flag_next_to_the_program_keeps_the_data_in_a_folder_beside_it() {
        let program = tempfile::tempdir().unwrap();
        assert_eq!(portable_data_root(program.path()), None, "no flag: the per-user folder is used");
        fs::write(program.path().join("portable.flag"), "").unwrap();
        assert_eq!(portable_data_root(program.path()), Some(program.path().join("data")));
    }

    #[test]
    fn only_a_plain_file_counts_as_the_flag() {
        let program = tempfile::tempdir().unwrap();
        fs::create_dir(program.path().join("portable.flag")).unwrap();
        assert_eq!(portable_data_root(program.path()), None);
        #[cfg(unix)]
        {
            let other = tempfile::NamedTempFile::new().unwrap();
            let program = tempfile::tempdir().unwrap();
            std::os::unix::fs::symlink(other.path(), program.path().join("portable.flag")).unwrap();
            assert_eq!(portable_data_root(program.path()), None, "a link is never followed");
        }
    }
}

mod portable_update {
    use ceh_desktop::apply_portable_update;
    use std::{fs, io::Write};

    fn zip_with(path: &std::path::Path) {
        let mut writer = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in [("SantaCita/SantaCita.exe", &b"new program"[..]), ("SantaCita/LICENSE.txt", b"new licence"), ("SantaCita/portable.flag", b"")] {
            writer.start_file(name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn the_downloaded_zip_replaces_the_program_and_cleans_up_after_itself() {
        let program = tempfile::tempdir().unwrap();
        let exe = program.path().join("SantaCita.exe");
        fs::write(&exe, b"old program").unwrap();
        fs::write(program.path().join("portable.flag"), "").unwrap();
        fs::write(program.path().join("LICENSE.txt"), b"old licence").unwrap();
        fs::create_dir(program.path().join("data")).unwrap();
        fs::write(program.path().join("data/tasks.yml"), b"my profiles").unwrap();
        let downloads = program.path().join("data/updates");
        fs::create_dir(&downloads).unwrap();
        let zip = downloads.join("SantaCita-1.2.0-windows-x64-portable.zip");
        zip_with(&zip);
        fs::write(downloads.join("SantaCita-1.2.0-windows-x64-portable.zip.sha256"), "abc").unwrap();

        apply_portable_update(&zip, &exe).unwrap();

        assert_eq!(fs::read(&exe).unwrap(), b"new program");
        assert_eq!(fs::read(program.path().join("SantaCita.exe.old")).unwrap(), b"old program");
        assert_eq!(fs::read(program.path().join("LICENSE.txt")).unwrap(), b"new licence");
        assert!(program.path().join("portable.flag").is_file());
        assert_eq!(fs::read(program.path().join("data/tasks.yml")).unwrap(), b"my profiles");
        assert!(!program.path().join("update-staging").exists(), "the unpacked copy is removed");
        assert!(!zip.exists() && !downloads.join("SantaCita-1.2.0-windows-x64-portable.zip.sha256").exists(), "the download is removed");
    }

    #[test]
    fn a_broken_package_changes_nothing() {
        let program = tempfile::tempdir().unwrap();
        let exe = program.path().join("SantaCita.exe");
        fs::write(&exe, b"old program").unwrap();
        let zip = program.path().join("bad.zip");
        fs::write(&zip, b"not a zip").unwrap();
        assert!(apply_portable_update(&zip, &exe).is_err());
        assert_eq!(fs::read(&exe).unwrap(), b"old program");
        assert!(!program.path().join("SantaCita.exe.old").exists());
        assert!(!program.path().join("update-staging").exists());
        assert!(zip.exists(), "the package is kept so the person can retry or inspect it");
    }
}

#[cfg(windows)]
mod windows_running_program {
    use ceh_desktop::apply_portable_update;
    use std::{fs, io::Write};

    /// The point of the whole scheme: Windows refuses to overwrite a running program but allows renaming it.
    #[test]
    fn a_program_that_is_running_can_be_swapped_for_the_new_one() {
        let program = tempfile::tempdir().unwrap();
        let exe = program.path().join("SantaCita.exe");
        let system = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        fs::copy(std::path::Path::new(&system).join("System32").join("ping.exe"), &exe).unwrap();
        let mut running = std::process::Command::new(&exe).args(["-n", "30", "127.0.0.1"]).stdout(std::process::Stdio::null()).spawn().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));
        assert!(running.try_wait().unwrap().is_none(), "the old program is running");

        let zip = program.path().join("p.zip");
        let mut writer = zip::ZipWriter::new(fs::File::create(&zip).unwrap());
        writer.start_file("SantaCita/SantaCita.exe", zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(b"new program bytes").unwrap();
        writer.finish().unwrap();

        let result = apply_portable_update(&zip, &exe);
        let _ = running.kill();
        let _ = running.wait();
        result.unwrap();
        assert_eq!(fs::read(&exe).unwrap(), b"new program bytes");
        assert!(program.path().join("SantaCita.exe.old").is_file(), "the running copy was kept aside");
        // Once the old program has ended, the next start removes its copy.
        ceh_services::portable_update::remove_old(&exe);
        assert!(!program.path().join("SantaCita.exe.old").exists());
    }
}

mod local_update_request {
    use ceh_desktop::local_update_request;
    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| (*a).to_owned()).collect()
    }

    #[test]
    fn a_zip_given_on_the_command_line_is_used_only_when_everything_allows_it() {
        let list = args(&["SantaCita.exe", "--apply-update", "C:\\x\\new.zip"]);
        assert_eq!(local_update_request(&list, true, true), Some(std::path::PathBuf::from("C:\\x\\new.zip")));
        assert_eq!(local_update_request(&list, false, true), None, "not allowed by the environment");
        assert_eq!(local_update_request(&list, true, false), None, "not a portable install");
        assert_eq!(local_update_request(&args(&["SantaCita.exe", "--apply-update"]), true, true), None, "no path");
        assert_eq!(local_update_request(&args(&["SantaCita.exe"]), true, true), None);
        assert_eq!(local_update_request(&args(&["SantaCita.exe", "--apply-update", ""]), true, true), None, "an empty path");
    }
}


#[test]
fn incomplete_auto_import_retries_after_a_late_io_failure_without_replacing_existing_data() {
    let support=tempfile::tempdir().unwrap();let old=python_folder(support.path());
    fs::create_dir_all(old.join("debug/sessions/fixture")).unwrap();
    fs::write(old.join("debug/sessions/fixture/journal.jsonl"),"{}\n").unwrap();
    let root=support.path().join("SantaCita");fs::create_dir_all(&root).unwrap();
    fs::write(root.join("debug"),"existing unrelated file").unwrap();
    assert_eq!(auto_import(&root,&[support.path().to_owned()],catalog()),None);
    assert!(root.join("tasks.yml").exists(),"the late failure happened after profiles copied");
    assert!(root.join(".legacy-import-incomplete").is_file());
    assert_eq!(fs::read_to_string(root.join("debug")).unwrap(),"existing unrelated file");
    let marker=root.join(".legacy-import-incomplete");
    let stamp=std::time::SystemTime::UNIX_EPOCH+std::time::Duration::from_secs(946684800);
    fs::OpenOptions::new().write(true).open(&marker).unwrap().set_modified(stamp).unwrap();
    let before=fs::metadata(&marker).unwrap().modified().unwrap();
    assert_eq!(auto_import(&root,&[support.path().to_owned()],catalog()),None);
    assert_eq!(fs::metadata(&marker).unwrap().modified().unwrap(),before,"a retry never truncates or rewrites its valid pending marker");
    assert!(ceh_desktop::import_incomplete(&root));
    fs::remove_file(root.join("debug")).unwrap();
    assert_eq!(auto_import(&root,&[support.path().to_owned()],catalog()),Some("Cita Extranjeria Helper".into()));
    assert!(root.join("debug/sessions/fixture/journal.jsonl").is_file());
    assert!(!root.join(".legacy-import-incomplete").exists());
}
#[test]
fn incomplete_import_is_visible_until_the_same_source_finishes() {
    let root=tempfile::tempdir().unwrap();
    let marker=root.path().join(".legacy-import-incomplete");
    assert!(!ceh_desktop::import_incomplete(root.path()));
    let name=ceh_store::legacy_import::SOURCE_NAMES[0];
    std::fs::write(&marker, name).unwrap();
    assert!(ceh_desktop::import_incomplete(root.path()));
    ceh_desktop::finish_legacy_import(root.path(), "other-source").unwrap();
    assert!(ceh_desktop::import_incomplete(root.path()));
    ceh_desktop::finish_legacy_import(root.path(), name).unwrap();
    assert!(!ceh_desktop::import_incomplete(root.path()));
    std::fs::write(&marker, "arbitrary-source").unwrap();
    assert!(!ceh_desktop::import_incomplete(root.path()));
}
