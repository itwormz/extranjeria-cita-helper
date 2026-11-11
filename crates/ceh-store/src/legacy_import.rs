//! Import of the Python application's data (profiles, statistics, achievements, budget files, session
//! journals) into this app's runtime. Read-only on the source; never overwrites existing target files
//! (the profile list, statistics and achievements are merged: what the new app already has stays, the old
//! app's missing entries are added, and the new file is backed up first);
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
    /// Backups made of files the import replaced or merged into (the profile list, statistics, achievements).
    pub backed_up: Vec<String>,
    /// Relative paths whose old content was merged into the new app's existing file.
    pub merged: Vec<String>,
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
/// A backup name next to `file` that does not exist yet (`name.before-import`, `.2`, ...).
fn backup_of(file: &Path) -> Result<PathBuf, ImportError> {
    let name = file.file_name().and_then(|n| n.to_str()).ok_or(ImportError::Io)?;
    (1..)
        .map(|n| if n == 1 { format!("{name}.before-import") } else { format!("{name}.before-import.{n}") })
        .map(|candidate| file.with_file_name(candidate))
        .find(|path| fs::symlink_metadata(path).is_err())
        .ok_or(ImportError::Io)
}
/// Achievements: the old app's records are added to the new file by record id (an existing id keeps the new
/// app's record). The new file is backed up first.
fn merge_achievements(from: &Path, to: &Path, report: &mut Report) -> Result<(), ImportError> {
    let read = |path: &Path| -> Result<serde_json::Map<String, serde_json::Value>, ImportError> {
        serde_json::from_slice::<serde_json::Value>(&fs::read(path).map_err(|_| ImportError::Io)?)
            .ok()
            .and_then(|v| match v { serde_json::Value::Object(map) => Some(map), _ => None })
            .ok_or(ImportError::Invalid)
    };
    let (old, mut current) = (read(from)?, read(to)?);
    let mut changed = false;
    for (person, records) in old {
        let Some(records) = records.as_array() else { continue };
        let entry = current.entry(person).or_insert_with(|| serde_json::Value::Array(Vec::new()));
        let Some(list) = entry.as_array_mut() else { continue };
        for record in records {
            let id = record["id"].as_str();
            if id.is_none() || list.iter().any(|r| r["id"].as_str() == id) {
                continue;
            }
            list.push(record.clone());
            changed = true;
        }
    }
    if changed {
        let backup = backup_of(to)?;
        fs::copy(to, &backup).map_err(|_| ImportError::Io)?;
        report.backed_up.push(backup.file_name().and_then(|n| n.to_str()).unwrap_or("").to_owned());
        let temporary = to.with_extension("json.import-tmp");
        fs::write(&temporary, serde_json::to_vec(&serde_json::Value::Object(current)).map_err(|_| ImportError::Invalid)?).map_err(|_| ImportError::Io)?;
        fs::rename(&temporary, to).map_err(|_| ImportError::Io)?;
    }
    report.merged.push("data/achievements.json".to_owned());
    Ok(())
}
/// Statistics: the old database's rows are added to the new one (same schema). Rows with a natural key are
/// ignored when they exist; the rest are guarded by a marker naming the imported database, so the same folder is
/// merged once. The new database is backed up first.
fn merge_statistics(from: &Path, to: &Path, report: &mut Report) -> Result<(), ImportError> {
    let marker = {
        let meta = fs::metadata(from).map_err(|_| ImportError::Io)?;
        let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
        format!("legacy_statistics_merged:{}:{}", meta.len(), modified)
    };
    let db = rusqlite::Connection::open(to).map_err(|_| ImportError::Io)?;
    db.busy_timeout(std::time::Duration::from_secs(10)).map_err(|_| ImportError::Io)?;
    let done: bool = db
        .query_row("SELECT EXISTS(SELECT 1 FROM metadata WHERE key=?1)", [&marker], |r| r.get(0))
        .unwrap_or(false);
    if done {
        report.kept.push("data/profile-statistics.sqlite3".to_owned());
        return Ok(());
    }
    let backup = backup_of(to)?;
    fs::copy(to, &backup).map_err(|_| ImportError::Io)?;
    report.backed_up.push(backup.file_name().and_then(|n| n.to_str()).unwrap_or("").to_owned());
    let source = from.to_str().ok_or(ImportError::Invalid)?;
    db.execute("ATTACH DATABASE ?1 AS old", [format!("file:{source}?mode=ro")]).or_else(|_| db.execute("ATTACH DATABASE ?1 AS old", [source])).map_err(|_| ImportError::Invalid)?;
    let merge = || -> rusqlite::Result<()> {
        db.execute_batch("BEGIN IMMEDIATE")?;
        for (table, columns) in [
            ("baseline", "profile,data"),
            ("baseline_provenance", "profile,data,migrated_at"),
            ("observed_active", "profile,session,start,end"),
            ("sessions", "profile,session,start,end,last_seen,reason,context"),
            ("activity_days", "profile,day"),
            ("applicant_notices", "profile,session,document,start"),
            ("pending_applicant_notices", "profile,document,start"),
            ("events", "profile,identity,kind,time,session,stage,mode,counted,offices,evidence"),
            ("active", "profile,session,start,end,seconds"),
            ("episodes", "profile,session,kind,stage,mode,document,start,end,last_seen,confirmed"),
        ] {
            db.execute(&format!("INSERT OR IGNORE INTO main.{table}({columns}) SELECT {columns} FROM old.{table}"), [])?;
        }
        db.execute("INSERT OR IGNORE INTO metadata(key,value) VALUES (?1,'1')", [&marker])?;
        db.execute_batch("COMMIT")
    };
    let result = merge();
    if result.is_err() {
        let _ = db.execute_batch("ROLLBACK");
    }
    let _ = db.execute_batch("DETACH DATABASE old");
    result.map_err(|_| ImportError::Invalid)?;
    report.merged.push("data/profile-statistics.sqlite3".to_owned());
    Ok(())
}
/// Import into `target`; `source` must be a folder returned by [`candidates`] (or an equivalent copy).
pub fn import(source: &Path, target: &Path, catalog: &ceh_core::profiles::ProfileCatalog) -> Result<Report, ImportError> {
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
    // Profiles: the new app writes its own list as soon as one profile is saved, so the old profiles are
    // added to it by id (never replacing the new app's own); an unreadable list is left alone.
    let profiles = target.join("tasks.yml");
    let old_profiles = source.join("tasks.yml");
    if regular(&profiles) && !same_file(&profiles, &old_profiles) {
        match crate::profile_write::profiles_to_add(&profiles, &old_profiles) {
            Ok(0) | Err(_) => report.kept.push("tasks.yml".to_owned()),
            Ok(_) => {
                let backup = backup_of(&profiles)?;
                fs::copy(&profiles, &backup).map_err(|_| ImportError::Io)?;
                match crate::profile_write::merge_profiles(&profiles, &old_profiles, catalog) {
                    Ok(_) => {
                        report.backed_up.push(backup.file_name().and_then(|n| n.to_str()).unwrap_or("").to_owned());
                        report.merged.push("tasks.yml".to_owned());
                    }
                    Err(_) => {
                        let _ = fs::remove_file(&backup);
                        report.kept.push("tasks.yml".to_owned());
                    }
                }
            }
        }
    } else {
        match copy_new(&old_profiles, &profiles, MAX_FILE, &mut total) {
            Ok(true) => report.copied.push("tasks.yml".to_owned()),
            Ok(false) => report.kept.push("tasks.yml".to_owned()),
            Err(error) => return Err(error),
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
        let (from, to) = (source.join(file), target.join(file));
        if regular(&from) && regular(&to) && !same_file(&from, &to) {
            match file {
                "data/achievements.json" => merge_achievements(&from, &to, &mut report)?,
                "data/profile-statistics.sqlite3" => merge_statistics(&from, &to, &mut report)?,
                _ => take(file.to_owned(), MAX_FILE, &mut report)?,
            }
        } else {
            take(file.to_owned(), MAX_FILE, &mut report)?;
        }
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
