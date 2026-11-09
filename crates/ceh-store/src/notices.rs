//! Recovering applicant "no appointments" notices from presentation journals (Python 0.3.3+
//! `ProfileStatistics.restore_applicant_notices`). Journals are append-only JSON lines under
//! `<root>/sessions/<profile>-*/journal.jsonl`; documents are kept only as SHA-256 digests.
use crate::migrate::StoreError;
use chrono::{DateTime, NaiveDateTime, Utc};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
};

fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

/// Python `_stamp`: ISO 8601 (a trailing `Z` or a missing offset means UTC) as `isoformat()` in UTC.
fn stamp(value: &str) -> Option<String> {
    let time = DateTime::parse_from_rfc3339(value)
        .map(|time| time.to_utc())
        .or_else(|_| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f").map(|naive| naive.and_utc())
        })
        .ok()?;
    Some(if time.timestamp_subsec_micros() == 0 {
        time.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
    } else {
        time.format("%Y-%m-%dT%H:%M:%S%.6f+00:00").to_string()
    })
}

fn journals(root: &Path, profile: &str) -> Vec<PathBuf> {
    let prefix = format!("{profile}-");
    let mut found: Vec<PathBuf> = std::fs::read_dir(root.join("sessions"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
        .map(|entry| entry.path().join("journal.jsonl"))
        .filter(|path| path.is_file())
        .collect();
    found.sort();
    found
}

/// Read the complete new lines of one journal; returns the visits and the new cursor.
fn read_visits(path: &Path, mut offset: u64) -> std::io::Result<Option<(Vec<(String, String)>, u64)>> {
    let size = path.metadata()?.len();
    if size == offset {
        return Ok(None);
    }
    if size < offset {
        offset = 0;
    }
    let mut reader = BufReader::new(File::open(path)?);
    reader.seek(SeekFrom::Start(offset))?;
    let mut rows = Vec::new();
    let mut line = Vec::new();
    while offset < size {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 || !line.ends_with(b"\n") {
            break;
        }
        offset += line.len() as u64;
        let Ok(row) = serde_json::from_slice::<Value>(&line) else { continue };
        let (Some("visit"), Some("applicant_no_slots"), Some(document), Some(time)) = (
            row["event"].as_str(),
            row["stage"].as_str(),
            row["document"].as_str().filter(|d| !d.is_empty()),
            row["time"].as_str(),
        ) else {
            continue;
        };
        if let Some(time) = stamp(time) {
            rows.push((digest(document), time));
        }
    }
    Ok(Some((rows, offset)))
}

/// Import journal visits, then match every pending visit with a recorded session of the profile.
/// Returns how many notices were recorded in this call. Safe to repeat; unmatched visits stay pending.
pub fn restore_applicant_notices(db: &mut Connection, profile: &str, root: &Path) -> Result<usize, StoreError> {
    for path in journals(root, profile) {
        let key = format!(
            "applicant-notices-v2:{}",
            digest(&path.canonicalize().unwrap_or_else(|_| path.clone()).to_string_lossy())
        );
        let saved: Option<String> = db
            .query_row("SELECT value FROM metadata WHERE key=?1", [&key], |r| r.get(0))
            .optional()
            .map_err(|_| StoreError::Unavailable)?;
        let offset = saved.and_then(|value| value.parse::<u64>().ok()).unwrap_or(0);
        let Ok(Some((rows, offset))) = read_visits(&path, offset) else { continue };
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| StoreError::Unavailable)?;
        for (document, time) in rows {
            tx.execute("INSERT OR IGNORE INTO pending_applicant_notices VALUES (?1,?2,?3)", params![profile, document, time])
                .map_err(|_| StoreError::Unavailable)?;
        }
        tx.execute(
            "INSERT INTO metadata VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, offset.to_string()],
        )
        .map_err(|_| StoreError::Unavailable)?;
        tx.commit().map_err(|_| StoreError::Unavailable)?;
    }
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| StoreError::Unavailable)?;
    let waiting: Vec<(String, String)> = {
        let mut statement = tx
            .prepare("SELECT document,start FROM pending_applicant_notices WHERE profile=?1")
            .map_err(|_| StoreError::Unavailable)?;
        statement
            .query_map([profile], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|_| StoreError::Unavailable)?
            .collect::<Result<_, _>>()
            .map_err(|_| StoreError::Unavailable)?
    };
    let mut restored = 0;
    for (document, time) in waiting {
        let session: Option<String> = tx
            .query_row(
                "SELECT session FROM sessions WHERE profile=?1 AND start<=?2 AND (end IS NULL OR end>=?2) ORDER BY start DESC LIMIT 1",
                params![profile, time],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| StoreError::Unavailable)?;
        if let Some(session) = session {
            tx.execute(
                "INSERT INTO applicant_notices VALUES (?1,?2,?3,?4) ON CONFLICT(profile,session,document) DO UPDATE SET start=MIN(start,excluded.start)",
                params![profile, session, document, time],
            )
            .map_err(|_| StoreError::Unavailable)?;
            tx.execute(
                "DELETE FROM pending_applicant_notices WHERE profile=?1 AND document=?2 AND start=?3",
                params![profile, document, time],
            )
            .map_err(|_| StoreError::Unavailable)?;
            restored += 1;
        }
    }
    tx.commit().map_err(|_| StoreError::Unavailable)?;
    Ok(restored)
}

/// Restore into an existing, migrated statistics file; a missing file or a database that predates the
/// notice tables is left untouched (reads never create or migrate storage).
pub fn restore_applicant_notices_file(database: &Path, profile: &str, root: &Path) -> Result<usize, StoreError> {
    use rusqlite::OpenFlags;
    if !database.is_file() {
        return Ok(0);
    }
    let mut db = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|_| StoreError::Unavailable)?;
    let ready: bool = db
        .query_row(
            "SELECT COUNT(*) = 3 FROM sqlite_master WHERE type='table' AND name IN ('applicant_notices','pending_applicant_notices','sessions')",
            [],
            |r| r.get(0),
        )
        .map_err(|_| StoreError::Unavailable)?;
    if !ready {
        return Ok(0);
    }
    restore_applicant_notices(&mut db, profile, root)
}
