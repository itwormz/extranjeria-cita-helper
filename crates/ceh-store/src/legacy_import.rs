//! Import of the Python application's data (profiles, statistics, achievements, budget files, session
//! journals) into this app's runtime. Read-only on the source; never overwrites existing target files
//! (a differing profile list is moved aside as `tasks.yml.before-import` first, the only exception);
//! notification secrets stay in the system vault, which both apps use under the same names.
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

/// Application-support folder names the Python application has used.
pub const SOURCE_NAMES: [&str; 2] = ["Cita Extranjeria Helper", "SantaCita"];
const MAX_FILE: u64 = 1024 * 1024 * 1024;
const MAX_JOURNAL: u64 = 64 * 1024 * 1024;
const MAX_TOTAL: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub name: String,
    path: PathBuf,
}
impl Candidate {
    pub fn path(&self) -> &Path {
        &self.path
    }
}
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    /// Relative paths copied into the new runtime.
    pub copied: Vec<String>,
    /// Relative paths that already existed there and were left untouched.
    pub kept: Vec<String>,
    /// Backups made of files the import replaced (the profile list of the new app).
    pub backed_up: Vec<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportError {
    /// The source has no `tasks.yml` profile file.
    NoProfiles,
    /// The old application still uses its statistics database.
    InUse,
    Invalid,
    Io,
}
/// Python data folders found under the given application-support directories (`tasks.yml` required).
pub fn candidates(support_dirs: &[PathBuf]) -> Vec<Candidate> {
    let mut found = Vec::new();
    for dir in support_dirs {
        for name in SOURCE_NAMES {
            let path = dir.join(name);
            if regular(&path.join("tasks.yml")) && !found.iter().any(|c: &Candidate| c.name == name) {
                found.push(Candidate { name: name.to_owned(), path });
            }
        }
    }
    found
}
fn regular(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file())
}
fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::metadata(a), fs::metadata(b)) {
        (Ok(x), Ok(y)) if x.len() == y.len() && x.len() <= MAX_FILE => fs::read(a).ok() == fs::read(b).ok(),
        _ => false,
    }
}
fn plain_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_dir())
}
/// Copy `from` to `to` through a temporary file; `false` when the target exists (left untouched).
fn copy_new(from: &Path, to: &Path, limit: u64, total: &mut u64) -> Result<bool, ImportError> {
    if fs::symlink_metadata(to).is_ok() {
        return Ok(false);
    }
    let size = fs::metadata(from).map_err(|_| ImportError::Io)?.len();
    *total += size;
    if size > limit || *total > MAX_TOTAL {
        return Err(ImportError::Invalid);
    }
    let parent = to.parent().ok_or(ImportError::Io)?;
    fs::create_dir_all(parent).map_err(|_| ImportError::Io)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|_| ImportError::Io)?;
    let mut source = fs::File::open(from).map_err(|_| ImportError::Io)?.take(limit + 1);
    let copied = std::io::copy(&mut source, &mut temp).map_err(|_| ImportError::Io)?;
    if copied > limit {
        return Err(ImportError::Invalid);
    }
    temp.flush().map_err(|_| ImportError::Io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o600)).map_err(|_| ImportError::Io)?;
    }
    temp.persist_noclobber(to).map_err(|_| ImportError::Io)?;
    Ok(true)
}
/// Import into `target`; `source` must be a folder returned by [`candidates`] (or an equivalent copy).
pub fn import(source: &Path, target: &Path) -> Result<Report, ImportError> {
    if !plain_dir(source) || !regular(&source.join("tasks.yml")) {
        return Err(ImportError::NoProfiles);
    }
    for sidecar in ["data/profile-statistics.sqlite3-wal", "data/profile-statistics.sqlite3-journal"] {
        if fs::metadata(source.join(sidecar)).is_ok_and(|m| m.len() > 0) {
            return Err(ImportError::InUse);
        }
    }
    fs::create_dir_all(target).map_err(|_| ImportError::Io)?;
    let mut report = Report::default();
    let mut total = 0u64;
    // The new app writes its own profile list as soon as one profile is saved, so a plain "keep what is
    // there" would hide every old profile. A differing list is moved aside, never deleted.
    let profiles = target.join("tasks.yml");
    let mut backup: Option<PathBuf> = None;
    if regular(&profiles) && !same_file(&profiles, &source.join("tasks.yml")) {
        let name = (1..)
            .map(|n| if n == 1 { "tasks.yml.before-import".to_owned() } else { format!("tasks.yml.before-import.{n}") })
            .find(|name| fs::symlink_metadata(target.join(name)).is_err())
            .ok_or(ImportError::Io)?;
        fs::rename(&profiles, target.join(&name)).map_err(|_| ImportError::Io)?;
        report.backed_up.push(name.clone());
        backup = Some(target.join(name));
    }
    match copy_new(&source.join("tasks.yml"), &profiles, MAX_FILE, &mut total) {
        Ok(true) => report.copied.push("tasks.yml".to_owned()),
        Ok(false) => report.kept.push("tasks.yml".to_owned()),
        Err(error) => {
            if let Some(backup) = backup {
                let _ = fs::rename(backup, &profiles);
            }
            return Err(error);
        }
    }
    let mut take = |relative: String, limit: u64, report: &mut Report| -> Result<(), ImportError> {
        let from = source.join(&relative);
        if !regular(&from) {
            return Ok(());
        }
        if copy_new(&from, &target.join(&relative), limit, &mut total)? {
            report.copied.push(relative);
        } else {
            report.kept.push(relative);
        }
        Ok(())
    };
    for file in ["data/achievements.json", "data/profile-statistics.sqlite3", "data/state.db"] {
        take(file.to_owned(), MAX_FILE, &mut report)?;
    }
    // Hashed budget files plus the salt that names them.
    if plain_dir(&source.join("data/appointment-budget")) {
        let mut names: Vec<_> = fs::read_dir(source.join("data/appointment-budget"))
            .map_err(|_| ImportError::Io)?
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n == "key" || (n.ends_with(".json") && n.len() <= 80))
            .collect();
        names.sort();
        for name in names {
            take(format!("data/appointment-budget/{name}"), 1024 * 1024, &mut report)?;
        }
    }
    // Session journals feed the statistics restore; screenshots and page snapshots stay behind.
    if plain_dir(&source.join("debug/sessions")) {
        let mut sessions: Vec<_> = fs::read_dir(source.join("debug/sessions"))
            .map_err(|_| ImportError::Io)?
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.len() <= 200 && !n.contains(['/', '\\']))
            .collect();
        sessions.sort();
        for session in sessions {
            take(format!("debug/sessions/{session}/journal.jsonl"), MAX_JOURNAL, &mut report)?;
        }
    }
    Ok(report)
}
