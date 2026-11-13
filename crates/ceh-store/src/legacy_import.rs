//! Import of the Python application's data (profiles, statistics, achievements, budget files, session
//! journals) into this app's runtime. Read-only on the source; never overwrites existing target files
//! (the profile list, statistics and achievements are merged: what the new app already has stays, the old
//! app's missing entries are added, and the new file is backed up first);
//! notification secrets stay in the system vault, which both apps use under the same names.
use rusqlite::{OptionalExtension, params};
use sha2_budget::{Digest, Sha256};
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
    // File content, not size/second-resolution mtime: a source may grow or close an episode
    // without changing either old marker component.
    let mut hash = Sha256::new();
    let mut source_file = fs::File::open(from).map_err(|_| ImportError::Io)?;
    let mut block = [0u8; 65536];
    loop { let n = source_file.read(&mut block).map_err(|_| ImportError::Io)?; if n == 0 { break; } hash.update(&block[..n]); }
    let marker = format!("legacy_statistics_content:{}", hex(&hash.finalize()));
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
        ] {
            db.execute(&format!("INSERT OR IGNORE INTO main.{table}({columns}) SELECT {columns} FROM old.{table}"), [])?;
        }
        merge_episodes(&db)?;
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
// Episode identity is the original fact, independent of end/last_seen enrichment and row ids
// assigned by each database. Old sources lack the three additive recognition columns.
fn merge_episodes(db: &rusqlite::Connection) -> rusqlite::Result<()> {
    let columns = |schema: &str| -> rusqlite::Result<Vec<String>> {
        db.prepare(&format!("PRAGMA {schema}.table_info(episodes)"))?.query_map([], |r| r.get(1))?.collect()
    };
    let target_columns = columns("main")?;
    for (name, default) in [("recognition", "'known'"), ("evidence", "'[]'"), ("variant", "''")] {
        if !target_columns.iter().any(|c| c == name) {
            db.execute_batch(&format!("ALTER TABLE main.episodes ADD COLUMN {name} TEXT DEFAULT {default}"))?;
        }
    }
    let old_columns = columns("old")?;
    let field = |name: &str, default: &str| if old_columns.iter().any(|c| c == name) { name.to_owned() } else { default.to_owned() };
    let sql = format!("SELECT profile,session,kind,stage,mode,document,start,end,last_seen,confirmed,{},{},{} FROM old.episodes", field("recognition", "'known'"), field("evidence", "'[]'"), field("variant", "''"));
    let mut rows = db.prepare(&sql)?;
    let mapped = rows.query_map([], |r| Ok((
        (0..9).map(|i|r.get::<_,Option<String>>(i)).collect::<rusqlite::Result<Vec<_>>>()?,
        r.get::<_,Option<i64>>(9)?.unwrap_or(0), r.get::<_,Option<String>>(10)?.unwrap_or_default(),
        r.get::<_,Option<String>>(11)?.unwrap_or_else(||"[]".into()), r.get::<_,Option<String>>(12)?.unwrap_or_default()
    )))?;
    for row in mapped {
        let (v, confirmed, recognition, evidence, variant) = row?;
        let identity = params![v[0],v[1],v[2],v[3],v[4],v[5],v[6]];
        let old: Option<(i64,String)> = db.query_row("SELECT id,COALESCE(evidence,'[]') FROM main.episodes WHERE profile IS ?1 AND session IS ?2 AND kind IS ?3 AND stage IS ?4 AND mode IS ?5 AND document IS ?6 AND start IS ?7 ORDER BY id LIMIT 1", identity, |r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((id, current_evidence)) = old {
            let merged = merge_evidence(&current_evidence, &evidence);
            db.execute("UPDATE main.episodes SET end=CASE WHEN ?1 IS NOT NULL AND (end IS NULL OR ?1>end) THEN ?1 ELSE end END,last_seen=CASE WHEN ?2 IS NOT NULL AND (last_seen IS NULL OR ?2>last_seen) THEN ?2 ELSE last_seen END,confirmed=MAX(COALESCE(confirmed,0),?3),recognition=CASE WHEN recognition IS NULL OR recognition='' OR recognition='known' THEN ?4 ELSE recognition END,evidence=?5,variant=CASE WHEN variant IS NULL OR variant='' THEN ?6 ELSE variant END WHERE id=?7",params![v[7],v[8],confirmed,recognition,merged,variant,id])?;
        } else {
            db.execute("INSERT INTO main.episodes(profile,session,kind,stage,mode,document,start,end,last_seen,confirmed,recognition,evidence,variant) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",params![v[0],v[1],v[2],v[3],v[4],v[5],v[6],v[7],v[8],confirmed,recognition,evidence,variant])?;
        }
    }
    Ok(())
}
fn hex(bytes:&[u8])->String {bytes.iter().map(|b|format!("{b:02x}")).collect()}
fn merge_evidence(current: &str, incoming: &str) -> String {
    let mut values: Vec<serde_json::Value> = serde_json::from_str(current).unwrap_or_default();
    for value in serde_json::from_str::<Vec<serde_json::Value>>(incoming).unwrap_or_default() {
        if !values.contains(&value) { values.push(value); }
    }
    serde_json::to_string(&values).unwrap_or_else(|_|"[]".into())
}

/// Check bounded file sizes before any profile/database mutation. Copying still checks again,
/// because an external process can change the source between preflight and the actual read.
pub fn preflight(source: &Path) -> Result<(), ImportError> {
    if !plain_dir(source) || !regular(&source.join("tasks.yml")) { return Err(ImportError::NoProfiles); }
    for sidecar in ["data/profile-statistics.sqlite3-wal", "data/profile-statistics.sqlite3-journal"] {
        if fs::metadata(source.join(sidecar)).is_ok_and(|m|m.len()>0) { return Err(ImportError::InUse); }
    }
    let mut files = vec![(source.join("tasks.yml"),MAX_FILE)];
    for name in ["data/achievements.json","data/profile-statistics.sqlite3","data/state.db"] { files.push((source.join(name),MAX_FILE)); }
    let budgets=source.join("data/appointment-budget");
    if plain_dir(&budgets) { for entry in fs::read_dir(budgets).map_err(|_|ImportError::Io)? { let entry=entry.map_err(|_|ImportError::Io)?; let name=entry.file_name(); let Some(name)=name.to_str() else {continue}; if name=="key" || name.ends_with(".json") || crate::appointment_budget::is_import_key(name) {files.push((entry.path(),if name=="key" || crate::appointment_budget::is_import_key(name) {32} else {1024*1024}));} } }
    let sessions=source.join("debug/sessions");
    if plain_dir(&sessions) { for entry in fs::read_dir(sessions).map_err(|_|ImportError::Io)? { let path=entry.map_err(|_|ImportError::Io)?.path(); if plain_dir(&path) { files.push((path.join("journal.jsonl"),MAX_JOURNAL)); } } }
    let mut total=0;
    for (path,limit) in files { if regular(&path) { let size=fs::metadata(path).map_err(|_|ImportError::Io)?.len();total+=size;if size>limit || total>MAX_TOTAL {return Err(ImportError::Invalid);} } }
    Ok(())
}
// Keep archive growth bounded before any target mutations. Imported archives are secrets,
// not arbitrary file references: their digest/name must agree and links are never followed.
fn validate_budget_keys(source:&Path,target:&Path)->Result<(),ImportError> {
    let mut archives=std::collections::BTreeSet::new();
    for root in [source,target] {
        if !plain_dir(root) {continue;}
        for entry in fs::read_dir(root).map_err(|_|ImportError::Io)? {
            let entry=entry.map_err(|_|ImportError::Io)?;let name=entry.file_name();let Some(name)=name.to_str() else {continue};
            if name=="key" || crate::appointment_budget::is_import_key(name) {
                if !regular(&entry.path()) || fs::metadata(entry.path()).map_err(|_|ImportError::Io)?.len()!=32 {return Err(ImportError::Invalid);}
                if name!="key" {let bytes=fs::read(entry.path()).map_err(|_|ImportError::Io)?;if name!=format!("import-key-{}.key",hex(&Sha256::digest(bytes))){return Err(ImportError::Invalid);}archives.insert(name.to_owned());}
            }
        }
    }
    if regular(&source.join("key")) && regular(&target.join("key")) {
        let old=fs::read(source.join("key")).map_err(|_|ImportError::Io)?;let current=fs::read(target.join("key")).map_err(|_|ImportError::Io)?;
        if old!=current {archives.insert(format!("import-key-{}.key",hex(&Sha256::digest(old))));}
    }
    if archives.len()>32 {return Err(ImportError::Invalid);}Ok(())
}
/// Import into `target`; `source` must be a folder returned by [`candidates`] (or an equivalent copy).
pub fn import(source: &Path, target: &Path, catalog: &ceh_core::profiles::ProfileCatalog) -> Result<Report, ImportError> {
    preflight(source)?;
    validate_budget_keys(&source.join("data/appointment-budget"),&target.join("data/appointment-budget"))?;
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
    // A salt cannot be replaced under the target's existing hashes. Retain extra salts so
    // BudgetStore can resolve imported hashes when the document is next known in memory.
    let budget_source = source.join("data/appointment-budget");
    let budget_target = target.join("data/appointment-budget");
    if regular(&budget_source.join("key")) && regular(&budget_target.join("key")) {
        let imported = fs::read(budget_source.join("key")).map_err(|_|ImportError::Io)?;
        let current = fs::read(budget_target.join("key")).map_err(|_|ImportError::Io)?;
        if imported.len()!=32 || current.len()!=32 {return Err(ImportError::Invalid);}
        if imported!=current {
            let name=format!("import-key-{}.key",hex(&Sha256::digest(&imported)));
            let relative=format!("data/appointment-budget/{name}");
            if copy_new(&budget_source.join("key"),&budget_target.join(name),32,&mut total)? {report.copied.push(relative);} else {report.kept.push(relative);}
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
            .filter(|n| n == "key" || crate::appointment_budget::is_import_key(n) || (n.ends_with(".json") && n.len() <= 80))
            .collect();
        names.sort();
        for name in names {
            let relative=format!("data/appointment-budget/{name}");
            let (from,to)=(budget_source.join(&name),budget_target.join(&name));
            let budget_key=name.strip_suffix(".json").filter(|key|key.len()==64&&key.bytes().all(|c|c.is_ascii_digit()||(b'a'..=b'f').contains(&c)));
            if let Some(key)=budget_key && regular(&from) && regular(&to) && !same_file(&from,&to) {
                let backup=backup_of(&to)?;
                if crate::appointment_budget::BudgetStore::merge_imported_snapshot(&from,&to,&target.join("data"),key,&backup).map_err(|_|ImportError::Io)? {
                    report.backed_up.push(backup.file_name().and_then(|n|n.to_str()).unwrap_or("").to_owned());report.merged.push(relative);
                } else {report.kept.push(relative);}
            } else {take(relative,1024*1024,&mut report)?;}
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
