//! Transactional migration of the existing statistics schema. Paths are explicitly selected.
use chrono::{DateTime, Datelike, Duration as TimeDelta, NaiveDate, NaiveDateTime, Timelike, Utc};
use chrono_tz::Europe::Madrid;
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
const MAX_LEGACY: usize = 16 * 1024 * 1024;
const COUNTERS: &[&str] = &[
    "checks",
    "offices_seen",
    "server_errors",
    "rate_limited",
    "blocked",
    "booking_screens",
    "expired_sessions",
    "flow_passes",
    "actions_performed",
    "pages_processed",
];
const READY: &str = "rust_core_ready_v1";
static BACKUP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    Unavailable,
    InvalidLegacy,
    UnsupportedSchema,
    BackupConflict,
    Busy,
    Full,
    Corrupt,
    InvalidTimestamp,
}
impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "statistics storage: {self:?}")
    }
}
impl std::error::Error for StoreError {}
impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        if let rusqlite::Error::SqliteFailure(code, _) = error {
            match code.code {
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked => {
                    Self::Busy
                }
                rusqlite::ErrorCode::DiskFull => Self::Full,
                rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase => {
                    Self::Corrupt
                }
                _ => Self::Unavailable,
            }
        } else {
            Self::Unavailable
        }
    }
}
impl From<std::io::Error> for StoreError {
    fn from(_: std::io::Error) -> Self {
        Self::Unavailable
    }
}
pub struct MigrationOptions {
    pub database: PathBuf,
    pub legacy_json: Option<PathBuf>,
    pub backup_directory: PathBuf,
}
pub struct MigrationResult {
    pub already_ready: bool,
    pub backup: Option<PathBuf>,
    pub imported_profiles: usize,
}
impl std::fmt::Debug for MigrationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MigrationResult")
            .field("already_ready", &self.already_ready)
            .field("imported_profiles", &self.imported_profiles)
            .finish_non_exhaustive()
    }
}
fn private_directory(path: &Path) -> Result<(), StoreError> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    Ok(())
}
fn new_private_file(path: &Path) -> Result<File, std::io::Error> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true).read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
fn immutable(path: &Path) -> Result<(), StoreError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o400))?;
    }
    #[cfg(windows)]
    {
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_readonly(true);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}
fn optional_bytes(path: &Path) -> Result<Option<Vec<u8>>, StoreError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(StoreError::Unavailable),
    };
    let mut bytes = vec![];
    file.take((MAX_LEGACY + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_LEGACY {
        return Err(StoreError::InvalidLegacy);
    }
    Ok(Some(bytes))
}
pub(crate) fn parse_time(value: &str) -> Result<DateTime<Utc>, StoreError> {
    let time = if let Ok(v) = DateTime::parse_from_rfc3339(value) {
        v.with_timezone(&Utc)
    } else if let Ok(v) = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f"))
    {
        v.and_utc()
    } else if let Ok(v) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        v.and_hms_opt(0, 0, 0)
            .ok_or(StoreError::InvalidTimestamp)?
            .and_utc()
    } else {
        return Err(StoreError::InvalidTimestamp);
    };
    if time.year() < 1 || time.year() > 9999 || time.nanosecond() >= 1_000_000_000 {
        return Err(StoreError::InvalidTimestamp);
    }
    time.with_nanosecond(time.nanosecond() / 1000 * 1000)
        .ok_or(StoreError::InvalidTimestamp)
}
pub(crate) fn stamp(time: DateTime<Utc>) -> String {
    let mut text = time.format("%Y-%m-%dT%H:%M:%S").to_string();
    let micros = time.nanosecond() / 1000;
    if micros != 0 {
        text.push_str(&format!(".{micros:06}"));
    }
    text.push_str("+00:00");
    text
}
struct LegacyRow {
    baseline: Value,
    provenance: Value,
}
fn normalized_legacy(bytes: Option<&[u8]>) -> Result<BTreeMap<String, LegacyRow>, StoreError> {
    let value = match bytes {
        None => json!({}),
        Some(bytes) => serde_json::from_slice(bytes).map_err(|_| StoreError::InvalidLegacy)?,
    };
    let object = value.as_object().ok_or(StoreError::InvalidLegacy)?;
    let mut result = BTreeMap::new();
    for (profile, value) in object {
        let mut row = serde_json::Map::new();
        for field in COUNTERS {
            row.insert((*field).into(), json!(0));
        }
        row.extend(serde_json::from_value::<serde_json::Map<String,Value>>(json!({"active_seconds":0,"started_at":null,"first_available_at":null,"first_available_seconds":null,"_offices":[],"_observations":[],"_last_office":""})).expect("fixed baseline defaults"));
        let (known, mut positive, active) = if let Some(checks) = value.as_u64() {
            row.insert("checks".into(), json!(checks));
            (vec!["checks".to_owned()], vec![], Value::Null)
        } else if let Some(raw) = value.as_object() {
            for name in COUNTERS {
                if let Some(v) = raw.get(*name) {
                    if v.as_u64().is_none() {
                        return Err(StoreError::InvalidLegacy);
                    }
                    row.insert((*name).into(), v.clone());
                }
            }
            for name in ["active_seconds", "first_available_seconds"] {
                if let Some(v) = raw.get(name) {
                    if !(name == "first_available_seconds" && v.is_null())
                        && !v.as_f64().is_some_and(|n| n.is_finite() && n >= 0.0)
                    {
                        return Err(StoreError::InvalidLegacy);
                    }
                    row.insert(name.into(), v.clone());
                }
            }
            for name in ["_offices", "_observations"] {
                if let Some(v) = raw.get(name) {
                    if !v.as_array().is_some_and(|a| a.iter().all(Value::is_string)) {
                        return Err(StoreError::InvalidLegacy);
                    }
                    row.insert(name.into(), v.clone());
                }
            }
            if let Some(v) = raw.get("_last_office") {
                if !v.is_string() {
                    return Err(StoreError::InvalidLegacy);
                }
                row.insert("_last_office".into(), v.clone());
            }
            for name in ["started_at", "first_available_at"] {
                if let Some(v) = raw.get(name) {
                    let normalized = if v.is_null() {
                        Value::Null
                    } else {
                        Value::String(stamp(
                            parse_time(v.as_str().ok_or(StoreError::InvalidLegacy)?)
                                .map_err(|_| StoreError::InvalidLegacy)?,
                        ))
                    };
                    row.insert(name.into(), normalized);
                }
            }
            let mut fields = COUNTERS
                .iter()
                .copied()
                .chain([
                    "active_seconds",
                    "started_at",
                    "first_available_at",
                    "first_available_seconds",
                    "_offices",
                ])
                .filter(|k| raw.contains_key(*k))
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let positive = ["offices_seen", "booking_screens"]
                .into_iter()
                .filter(|k| raw.get(*k).is_some_and(|v| v.as_u64().unwrap_or(0) > 0))
                .map(str::to_owned)
                .collect::<Vec<_>>();
            (
                std::mem::take(&mut fields),
                positive,
                raw.get("active_seconds").cloned().unwrap_or(Value::Null),
            )
        } else {
            return Err(StoreError::InvalidLegacy);
        };
        if row["_offices"].as_array().is_some_and(|v| !v.is_empty())
            && !positive.iter().any(|v| v == "offices_seen")
        {
            positive.push("offices_seen".into());
        }
        if !row["first_available_at"].is_null() && positive.is_empty() {
            positive = vec!["offices_seen".into(), "booking_screens".into()];
        }
        let provenance = json!({"known":known,"positive":positive,"active_seconds":active,"legacy_first_available_at":row["first_available_at"],"legacy_first_available_elapsed_seconds":row["first_available_seconds"]});
        let hashes = row["_observations"]
            .as_array()
            .expect("validated observations")
            .iter()
            .filter(|v| {
                v.as_str().is_some_and(|s| {
                    s.len() == 64
                        && s.bytes()
                            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        row.insert("_observations".into(), json!(hashes));
        result.insert(
            profile.clone(),
            LegacyRow {
                baseline: Value::Object(row),
                provenance,
            },
        );
    }
    Ok(result)
}
fn has_table(db: &Connection, table: &str) -> Result<bool, StoreError> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |r| r.get(0),
    )?)
}
fn marker(db: &Connection, key: &str) -> Result<Option<String>, StoreError> {
    if !has_table(db, "metadata")? {
        return Ok(None);
    }
    Ok(db
        .query_row("SELECT value FROM metadata WHERE key=?1", [key], |r| {
            r.get(0)
        })
        .optional()?)
}
fn validate_schema(db: &Connection, complete: bool) -> Result<(), StoreError> {
    let unsafe_schema: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type IN ('view','trigger'))",
        [],
        |r| r.get(0),
    )?;
    if unsafe_schema {
        return Err(StoreError::UnsupportedSchema);
    }
    if marker(db, "schema_version")?.is_some_and(|v| v != "1") {
        return Err(StoreError::UnsupportedSchema);
    }
    let required = [
        ("metadata", vec!["key", "value"]),
        ("baseline", vec!["profile", "data"]),
        (
            "baseline_provenance",
            vec!["profile", "data", "migrated_at"],
        ),
        (
            "observed_active",
            vec!["profile", "session", "start", "end"],
        ),
        (
            "events",
            vec![
                "id", "profile", "identity", "kind", "time", "session", "stage", "mode", "counted",
                "offices", "evidence",
            ],
        ),
        (
            "active",
            vec!["id", "profile", "session", "start", "end", "seconds"],
        ),
        (
            "sessions",
            vec![
                "profile",
                "session",
                "start",
                "end",
                "last_seen",
                "reason",
                "context",
            ],
        ),
        (
            "episodes",
            vec![
                "id",
                "profile",
                "session",
                "kind",
                "stage",
                "mode",
                "document",
                "start",
                "end",
                "last_seen",
                "confirmed",
            ],
        ),
        ("activity_days", vec!["profile", "day"]),
    ];
    for (table, fields) in required {
        if !has_table(db, table)? {
            if complete {
                return Err(StoreError::Corrupt);
            }
            continue;
        }
        let mut stmt = db.prepare(&format!("PRAGMA table_info({table})"))?;
        let names = stmt
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if fields.iter().any(|field| !names.iter().any(|v| v == field)) {
            return Err(StoreError::UnsupportedSchema);
        }
    }
    Ok(())
}
fn sqlite_backup(database: &Path, directory: &Path, at: i64) -> Result<PathBuf, StoreError> {
    private_directory(directory)?;
    let (path, file) = loop {
        let next = BACKUP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(
            "pre-rust-{at}-{}-{next}.sqlite3",
            std::process::id()
        ));
        match new_private_file(&path) {
            Ok(file) => break (path, file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(StoreError::Unavailable),
        }
    };
    let copied = (|| -> Result<(), StoreError> {
        let source = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut destination = Connection::open(&path)?;
        {
            let backup = rusqlite::backup::Backup::new(&source, &mut destination)?;
            backup.run_to_completion(256, Duration::from_millis(10), None)?;
        }
        destination.execute_batch("PRAGMA journal_mode=DELETE")?;
        let check: String = destination.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if check != "ok" {
            return Err(StoreError::Corrupt);
        }
        destination.close().map_err(|(_, e)| StoreError::from(e))?;
        file.sync_all()?;
        immutable(&path)?;
        Ok(())
    })();
    if let Err(error) = copied {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(error);
    }
    Ok(path)
}
fn legacy_backup_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".bak");
    PathBuf::from(name)
}
fn legacy_backup(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let backup = legacy_backup_path(path);
    match new_private_file(&backup) {
        Ok(mut file) => {
            if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
                drop(file);
                let _ = fs::remove_file(&backup);
                return Err(error.into());
            }
            immutable(&backup)?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if optional_bytes(&backup)?.as_deref() != Some(bytes) {
                return Err(StoreError::BackupConflict);
            }
            Ok(())
        }
        Err(_) => Err(StoreError::Unavailable),
    }
}
fn activity_days(db: &Connection) -> Result<(), StoreError> {
    let mut stmt=db.prepare("SELECT profile,time FROM events WHERE kind NOT IN ('finish','start','pause','resume','handoff','manual_required','fatal_error')")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (profile, time) = row?;
        let day = parse_time(&time)?
            .with_timezone(&Madrid)
            .date_naive()
            .to_string();
        db.execute(
            "INSERT OR IGNORE INTO activity_days VALUES (?1,?2)",
            params![profile, day],
        )?;
    }
    let mut stmt = db.prepare("SELECT profile,start,end FROM active")?;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (profile, start, end) = row?;
        let start = parse_time(&start)?;
        let end = parse_time(&end)?;
        if start >= end {
            continue;
        }
        let mut day = start.with_timezone(&Madrid).date_naive();
        let last = (end - TimeDelta::microseconds(1))
            .with_timezone(&Madrid)
            .date_naive();
        while day <= last {
            db.execute(
                "INSERT OR IGNORE INTO activity_days VALUES (?1,?2)",
                params![profile, day.to_string()],
            )?;
            day = day.succ_opt().ok_or(StoreError::InvalidTimestamp)?;
        }
    }
    Ok(())
}
pub fn migrate_paths(
    options: &MigrationOptions,
    now_millis: i64,
) -> Result<MigrationResult, StoreError> {
    let now =
        DateTime::<Utc>::from_timestamp_millis(now_millis).ok_or(StoreError::InvalidTimestamp)?;
    if now.year() < 1 || now.year() > 9999 {
        return Err(StoreError::InvalidTimestamp);
    }
    if let Some(parent) = options.database.parent() {
        private_directory(parent)?;
    }
    let existed = options.database.exists();
    let mut db = Connection::open(&options.database)?;
    db.busy_timeout(Duration::from_secs(30))?;
    db.execute_batch("PRAGMA trusted_schema=OFF")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&options.database, fs::Permissions::from_mode(0o600))?;
    }
    migrate_connection(options, now_millis, &mut db, existed)
}
fn migrate_connection(
    options: &MigrationOptions,
    now_millis: i64,
    db: &mut Connection,
    existed: bool,
) -> Result<MigrationResult, StoreError> {
    let now =
        DateTime::<Utc>::from_timestamp_millis(now_millis).ok_or(StoreError::InvalidTimestamp)?;
    if now.year() < 1 || now.year() > 9999 {
        return Err(StoreError::InvalidTimestamp);
    }
    let now = stamp(now);
    validate_schema(db, false)?;
    if let Some(ready) = marker(db, READY)? {
        if ready != "1" {
            return Err(StoreError::UnsupportedSchema);
        }
        validate_schema(db, true)?;
        return Ok(MigrationResult {
            already_ready: true,
            backup: None,
            imported_profiles: 0,
        });
    }
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let backup = if existed {
        Some(sqlite_backup(
            &options.database,
            &options.backup_directory,
            now_millis,
        )?)
    } else {
        None
    };
    tx.execute_batch(include_str!("schema.sql"))?;
    let mut columns = tx.prepare("PRAGMA table_info(episodes)")?;
    let names = columns
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(columns);
    for (name, default) in [
        ("recognition", "'known'"),
        ("evidence", "'[]'"),
        ("variant", "''"),
    ] {
        if !names.iter().any(|v| v == name) {
            tx.execute_batch(&format!(
                "ALTER TABLE episodes ADD COLUMN {name} TEXT DEFAULT {default}"
            ))?;
        }
    }
    let mut imported_profiles = 0;
    if marker(&tx, "migrated")?.is_none() {
        let bytes = match &options.legacy_json {
            Some(path) => optional_bytes(path)?,
            None => None,
        };
        let rows = normalized_legacy(bytes.as_deref())?;
        if let (Some(path), Some(bytes)) = (&options.legacy_json, &bytes) {
            if optional_bytes(path)?.as_deref() != Some(bytes.as_slice()) {
                return Err(StoreError::BackupConflict);
            }
            legacy_backup(path, bytes)?;
        }
        for (profile, row) in rows {
            tx.execute(
                "INSERT INTO baseline VALUES (?1,?2)",
                params![profile, row.baseline.to_string()],
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO baseline_provenance VALUES (?1,?2,?3)",
                params![profile, row.provenance.to_string(), now],
            )?;
            imported_profiles += 1;
        }
        tx.execute("INSERT INTO metadata VALUES ('migrated','1')", [])?;
    }
    if marker(&tx, "baseline_provenance_ready")?.is_none()
        || marker(&tx, "baseline_provenance_v2_ready")?.is_none()
    {
        let original = options.legacy_json.as_ref().and_then(|path| {
            let backup = legacy_backup_path(path);
            optional_bytes(if backup.exists() { &backup } else { path })
                .ok()
                .flatten()
        });
        let original = normalized_legacy(original.as_deref()).unwrap_or_default();
        let mut stmt = tx.prepare("SELECT profile FROM baseline")?;
        let profiles = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for profile in profiles {
            let provenance = original
                .get(&profile)
                .map(|r| r.provenance.clone())
                .unwrap_or_else(|| json!({"known":[],"positive":[],"active_seconds":null}));
            tx.execute(
                "INSERT OR IGNORE INTO baseline_provenance VALUES (?1,?2,?3)",
                params![profile, provenance.to_string(), now],
            )?;
            let old: String = tx.query_row(
                "SELECT data FROM baseline_provenance WHERE profile=?1",
                [&profile],
                |r| r.get(0),
            )?;
            let mut old: Value = serde_json::from_str(&old).map_err(|_| StoreError::Corrupt)?;
            if old.get("legacy_first_available_at").is_none() {
                old["legacy_first_available_at"] = provenance
                    .get("legacy_first_available_at")
                    .cloned()
                    .unwrap_or(Value::Null);
                old["legacy_first_available_elapsed_seconds"] = provenance
                    .get("legacy_first_available_elapsed_seconds")
                    .cloned()
                    .unwrap_or(Value::Null);
                let mut known = old["known"]
                    .as_array()
                    .cloned()
                    .ok_or(StoreError::Corrupt)?;
                for field in provenance["known"].as_array().ok_or(StoreError::Corrupt)? {
                    if !known.contains(field) {
                        known.push(field.clone());
                    }
                }
                old["known"] = json!(known);
                tx.execute(
                    "UPDATE baseline_provenance SET data=?1 WHERE profile=?2",
                    params![old.to_string(), profile],
                )?;
            }
        }
        tx.execute(
            "INSERT OR IGNORE INTO metadata VALUES ('baseline_provenance_ready','1')",
            [],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO metadata VALUES ('baseline_provenance_v2_ready','1')",
            [],
        )?;
    }
    if marker(&tx, "activity_days_ready")?.is_none() {
        activity_days(&tx)?;
        tx.execute(
            "INSERT INTO metadata VALUES ('activity_days_ready','1')",
            [],
        )?;
    }
    tx.execute(
        "INSERT OR IGNORE INTO metadata VALUES ('schema_version','1')",
        [],
    )?;
    tx.execute("INSERT INTO metadata VALUES (?1,'1')", [READY])?;
    #[cfg(test)]
    if std::env::var_os("CEH_RUST_MIGRATION_CRASH_TEST")
        .is_some_and(|path| Path::new(&path) == options.database)
    {
        std::process::exit(93);
    }
    tx.commit()?;
    db.execute_batch("PRAGMA journal_mode=WAL")?;
    Ok(MigrationResult {
        already_ready: false,
        backup,
        imported_profiles,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../.local/builds/work/rust-sqlite-fault-tests");
            fs::create_dir_all(&base).unwrap();
            loop {
                let path = base.join(format!(
                    "{}-{}",
                    std::process::id(),
                    TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => panic!("scratch directory: {e}"),
                }
            }
        }
        fn options(&self) -> MigrationOptions {
            MigrationOptions {
                database: self.0.join("stats.sqlite3"),
                legacy_json: Some(self.0.join("stats.json")),
                backup_directory: self.0.join("backups"),
            }
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn original(path: &Path) -> Connection {
        let db = Connection::open(path).unwrap();
        db.execute_batch("CREATE TABLE metadata(key TEXT PRIMARY KEY,value TEXT);INSERT INTO metadata VALUES ('schema_version','1');CREATE TABLE retained(value TEXT);INSERT INTO retained VALUES ('keep');").unwrap();
        db
    }
    #[test]
    fn backup_name_appends_suffix_without_changing_extensionless_names() {
        assert_eq!(
            legacy_backup_path(Path::new("stats")),
            PathBuf::from("stats.bak")
        );
        assert_eq!(
            legacy_backup_path(Path::new("stats.json")),
            PathBuf::from("stats.json.bak")
        );
    }
    #[test]
    fn sqlite_full_rolls_back_partial_schema_and_keeps_original_data() {
        let scratch = Scratch::new();
        let options = scratch.options();
        let mut db = original(&options.database);
        let count: i64 = db.query_row("PRAGMA page_count", [], |r| r.get(0)).unwrap();
        db.execute_batch(&format!("PRAGMA max_page_count={}", count + 3))
            .unwrap();
        let error = migrate_connection(&options, 1793275200000, &mut db, true)
            .expect_err("SQLITE_FULL not triggered");
        assert_eq!(error, StoreError::Full);
        assert_eq!(
            db.query_row("SELECT value FROM retained", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "keep"
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
        assert!(marker(&db, READY).unwrap().is_none());
    }
    #[test]
    fn crash_worker() {
        let Some(path) = std::env::var_os("CEH_RUST_MIGRATION_CRASH_TEST") else {
            return;
        };
        let database = PathBuf::from(path);
        let options = MigrationOptions {
            legacy_json: Some(database.with_extension("json")),
            backup_directory: database.parent().unwrap().join("backups"),
            database,
        };
        migrate_paths(&options, 1793275200000).unwrap();
        panic!("crash hook was not reached");
    }
    #[test]
    fn process_exit_before_commit_rolls_back_and_retry_imports_once() {
        let scratch = Scratch::new();
        let options = scratch.options();
        drop(original(&options.database));
        fs::write(options.legacy_json.as_ref().unwrap(), b"{\"fixture\":9}").unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "migrate::tests::crash_worker", "--nocapture"])
            .env("CEH_RUST_MIGRATION_CRASH_TEST", &options.database)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(93),
            "crash helper did not exit at migration boundary"
        );
        let db = Connection::open(&options.database).unwrap();
        assert_eq!(
            db.query_row("SELECT value FROM retained", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "keep"
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='baseline'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert!(marker(&db, READY).unwrap().is_none());
        drop(db);
        let result = migrate_paths(&options, 1793275200000).unwrap();
        assert_eq!(result.imported_profiles, 1);
        let again = migrate_paths(&options, 1793275200000).unwrap();
        assert!(again.already_ready);
        let db = Connection::open(&options.database).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM baseline", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}
