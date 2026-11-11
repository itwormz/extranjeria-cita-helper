//! The portable Windows program updates itself: the verified ZIP is unpacked next to it, then the program file is
//! swapped (the running one is renamed, which Windows allows) and the person's `data` folder is never touched.
use ceh_services::portable_update::{extract_program, portable_flag_in, remove_old, swap_program, PortableError};
use std::{fs, io::Write, path::Path};

fn zip_with(path: &Path, entries: &[(&str, &[u8])]) {
    let mut writer = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    for (name, bytes) in entries {
        if name.ends_with('/') {
            writer.add_directory(*name, options).unwrap();
        } else {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
    }
    writer.finish().unwrap();
}
fn good(dir: &Path) -> std::path::PathBuf {
    let zip = dir.join("p.zip");
    zip_with(&zip, &[("SantaCita/", b""), ("SantaCita/SantaCita.exe", b"new program"), ("SantaCita/LICENSE.txt", b"licence"), ("SantaCita/portable.flag", b"")]);
    zip
}

#[test]
fn only_the_program_and_its_licence_are_taken_out_of_the_zip() {
    let dir = tempfile::tempdir().unwrap();
    let staging = dir.path().join("stage");
    let exe = extract_program(&good(dir.path()), &staging, "SantaCita.exe").unwrap();
    assert_eq!(fs::read(&exe).unwrap(), b"new program");
    assert_eq!(fs::read(staging.join("LICENSE.txt")).unwrap(), b"licence");
    // The flag stays with the installed program: the update never rewrites it.
    assert!(!staging.join("portable.flag").exists());
    let mut names: Vec<_> = fs::read_dir(&staging).unwrap().flatten().map(|e| e.file_name().into_string().unwrap()).collect();
    names.sort();
    assert_eq!(names, ["LICENSE.txt", "SantaCita.exe"]);
}

#[test]
fn a_zip_that_reaches_outside_or_adds_other_files_is_refused_and_leaves_nothing() {
    for bad in [
        vec![("SantaCita/SantaCita.exe", &b"x"[..]), ("SantaCita/../evil.exe", b"x")],
        vec![("SantaCita/SantaCita.exe", b"x"), ("/abs/evil.exe", b"x")],
        vec![("Other/SantaCita.exe", b"x")],
        vec![("SantaCita/SantaCita.exe", b"x"), ("SantaCita/evil.dll", b"x")],
        vec![("SantaCita/SantaCita.exe", b"x"), ("SantaCita/data/tasks.yml", b"x")],
        vec![("SantaCita/LICENSE.txt", b"only a licence")],
    ] {
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("p.zip");
        zip_with(&zip, &bad);
        let staging = dir.path().join("stage");
        assert!(extract_program(&zip, &staging, "SantaCita.exe").is_err(), "{:?}", bad.iter().map(|e| e.0).collect::<Vec<_>>());
        assert!(!staging.exists() || fs::read_dir(&staging).unwrap().next().is_none(), "nothing is left behind");
    }
}

#[test]
fn a_program_far_bigger_than_expected_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let zip = dir.path().join("p.zip");
    zip_with(&zip, &[("SantaCita/SantaCita.exe", &vec![0u8; 2 * 1024 * 1024])]);
    let staging = dir.path().join("stage");
    assert_eq!(ceh_services::portable_update::extract_program_limited(&zip, &staging, "SantaCita.exe", 1024 * 1024).err(), Some(PortableError::Invalid));
    assert!(!staging.join("SantaCita.exe").exists());
}

#[test]
fn the_running_program_is_renamed_aside_and_the_new_one_takes_its_place() {
    let dir = tempfile::tempdir().unwrap();
    let current = dir.path().join("SantaCita.exe");
    fs::write(&current, b"old program").unwrap();
    fs::create_dir(dir.path().join("data")).unwrap();
    fs::write(dir.path().join("data/tasks.yml"), b"my profiles").unwrap();
    let new = dir.path().join("stage-SantaCita.exe");
    fs::write(&new, b"new program").unwrap();
    swap_program(&current, &new).unwrap();
    assert_eq!(fs::read(&current).unwrap(), b"new program");
    assert_eq!(fs::read(dir.path().join("SantaCita.exe.old")).unwrap(), b"old program", "kept until the next start");
    assert!(!new.exists());
    assert_eq!(fs::read(dir.path().join("data/tasks.yml")).unwrap(), b"my profiles", "the data is never touched");
    // A later start removes the old copy.
    remove_old(&current);
    assert!(!dir.path().join("SantaCita.exe.old").exists());
    // An older leftover does not block the next update.
    fs::write(dir.path().join("SantaCita.exe.old"), b"leftover").unwrap();
    let again = dir.path().join("stage2.exe");
    fs::write(&again, b"newer").unwrap();
    swap_program(&current, &again).unwrap();
    assert_eq!(fs::read(&current).unwrap(), b"newer");
    assert_eq!(fs::read(dir.path().join("SantaCita.exe.old")).unwrap(), b"new program");
}

#[test]
fn a_failed_swap_puts_the_old_program_back() {
    let dir = tempfile::tempdir().unwrap();
    let current = dir.path().join("SantaCita.exe");
    fs::write(&current, b"old program").unwrap();
    let missing = dir.path().join("not-there.exe");
    assert!(swap_program(&current, &missing).is_err());
    assert_eq!(fs::read(&current).unwrap(), b"old program");
    assert!(!dir.path().join("SantaCita.exe.old").exists());
}

#[test]
fn only_a_plain_flag_file_beside_the_program_makes_an_install_portable() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!portable_flag_in(dir.path()));
    fs::write(dir.path().join("portable.flag"), "").unwrap();
    assert!(portable_flag_in(dir.path()));
    let other = tempfile::tempdir().unwrap();
    fs::create_dir(other.path().join("portable.flag")).unwrap();
    assert!(!portable_flag_in(other.path()));
}

#[test]
fn a_zip_written_by_windows_powershell_with_backslashes_is_accepted_like_any_other() {
    // Compress-Archive of Windows PowerShell 5.1 stores `SantaCita\SantaCita.exe`; our own packaging uses it.
    let dir = tempfile::tempdir().unwrap();
    let zip = dir.path().join("p.zip");
    zip_with(&zip, &[("SantaCita\\SantaCita.exe", b"new program"), ("SantaCita\\LICENSE.txt", b"licence"), ("SantaCita\\portable.flag", b"")]);
    let staging = dir.path().join("stage");
    let exe = extract_program(&zip, &staging, "SantaCita.exe").unwrap();
    assert_eq!(fs::read(&exe).unwrap(), b"new program");
    assert_eq!(fs::read(staging.join("LICENSE.txt")).unwrap(), b"licence");
    // Backslashes do not open a way out of the folder: only the known names count.
    let bad = dir.path().join("bad.zip");
    zip_with(&bad, &[("SantaCita\\SantaCita.exe", b"x"), ("SantaCita\\..\\evil.exe", b"x")]);
    assert!(extract_program(&bad, &dir.path().join("stage2"), "SantaCita.exe").is_err());
}
