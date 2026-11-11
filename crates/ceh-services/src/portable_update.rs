//! Self-update of the portable Windows program. The verified ZIP is unpacked beside the program, then the program
//! file is swapped: the running `.exe` is renamed aside (Windows allows that) and the new one takes its name. The
//! person's `data` folder and the `portable.flag` file are never touched.
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, PartialEq, Eq)]
pub enum PortableError {
    /// The package is not what a portable release contains.
    Invalid,
    Io,
}
const TOP: &str = "SantaCita/";
const PROGRAM: &str = "SantaCita/SantaCita.exe";
const LICENCE: &str = "SantaCita/LICENSE.txt";
const FLAG: &str = "portable.flag";
const MAX_PROGRAM: u64 = 300 * 1024 * 1024;
const MAX_LICENCE: u64 = 1024 * 1024;

/// A plain `portable.flag` file in the program's folder: a link or a folder of that name does not count.
pub fn portable_flag_in(dir: &Path) -> bool {
    fs::symlink_metadata(dir.join(FLAG)).is_ok_and(|meta| meta.file_type().is_file())
}
pub fn extract_program(zip_path: &Path, staging: &Path, exe_name: &str) -> Result<PathBuf, PortableError> {
    extract_program_limited(zip_path, staging, exe_name, MAX_PROGRAM)
}
/// Take the program and its licence out of a portable ZIP into `staging`; returns the new program file, named
/// `exe_name`. Every entry must be one of the known files of the single `SantaCita/` folder.
pub fn extract_program_limited(zip_path: &Path, staging: &Path, exe_name: &str, limit: u64) -> Result<PathBuf, PortableError> {
    if exe_name.is_empty() || exe_name.contains(['/', '\\']) {
        return Err(PortableError::Invalid);
    }
    let mut archive = zip::ZipArchive::new(fs::File::open(zip_path).map_err(|_| PortableError::Io)?).map_err(|_| PortableError::Invalid)?;
    let mut seen: Vec<String> = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|_| PortableError::Invalid)?;
        let name = entry.name().to_owned();
        let known = [TOP, PROGRAM, LICENCE, &format!("{TOP}{FLAG}")].contains(&name.as_str());
        if !known || seen.contains(&name) || entry.is_symlink() {
            return Err(PortableError::Invalid);
        }
        seen.push(name);
    }
    if !seen.iter().any(|name| name == PROGRAM) {
        return Err(PortableError::Invalid);
    }
    let created = !staging.exists();
    fs::create_dir_all(staging).map_err(|_| PortableError::Io)?;
    let mut written: Vec<PathBuf> = Vec::new();
    let result = (|| {
        let mut take = |entry_name: &str, target: &Path, max: u64| -> Result<(), PortableError> {
            let Ok(mut entry) = archive.by_name(entry_name) else { return Ok(()) };
            let mut data = Vec::new();
            entry.by_ref().take(max + 1).read_to_end(&mut data).map_err(|_| PortableError::Invalid)?;
            if data.len() as u64 > max {
                return Err(PortableError::Invalid);
            }
            written.push(target.to_owned());
            fs::File::create(target).and_then(|mut file| file.write_all(&data)).map_err(|_| PortableError::Io)
        };
        let exe = staging.join(exe_name);
        take(PROGRAM, &exe, limit)?;
        take(LICENCE, &staging.join("LICENSE.txt"), MAX_LICENCE)?;
        Ok(exe)
    })();
    if result.is_err() {
        for file in &written {
            let _ = fs::remove_file(file);
        }
        if created {
            let _ = fs::remove_dir(staging);
        }
    }
    result
}
fn old_copy(current: &Path) -> PathBuf {
    let mut name = current.as_os_str().to_owned();
    name.push(".old");
    PathBuf::from(name)
}
/// Put `new` in the place of `current`: the running program is renamed to `<name>.old` first. A failure puts the old
/// program back.
pub fn swap_program(current: &Path, new: &Path) -> Result<(), PortableError> {
    if !new.is_file() || !current.is_file() {
        return Err(PortableError::Io);
    }
    let old = old_copy(current);
    if old.exists() {
        fs::remove_file(&old).map_err(|_| PortableError::Io)?;
    }
    fs::rename(current, &old).map_err(|_| PortableError::Io)?;
    if fs::rename(new, current).is_err() {
        let _ = fs::rename(&old, current);
        return Err(PortableError::Io);
    }
    Ok(())
}
/// The `.old` copy left by the previous update, removed at the next start.
pub fn remove_old(current: &Path) {
    let _ = fs::remove_file(old_copy(current));
}
