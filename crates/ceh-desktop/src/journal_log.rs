//! The work log kept between runs (Python `journal.jsonl`): every search appends the rows of its journal to
//! `<root>/data/journal/<profile>/<start>.jsonl`, and the Work tab shows the latest 250 rows of the profile
//! before the new search's own rows. Rows are public page facts (stage, status, office names), never data
//! of the applicant.
use serde::Serialize;
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

const KEPT_SEARCHES: usize = 20;

fn directory(root: &Path, profile: &str) -> Option<PathBuf> {
    let safe = !profile.is_empty()
        && profile.len() <= 128
        && profile.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    safe.then(|| root.join("data/journal").join(profile))
}
fn files_newest_first(directory: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = fs::read_dir(directory)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect();
    files.sort();
    files.reverse();
    files
}

pub struct JournalLog {
    directory: PathBuf,
    started_ms: u64,
    file: Option<File>,
    /// The last stored text of each row, by its id (the journal drops its oldest rows, so positions move).
    written: std::collections::HashMap<u64, String>,
}
impl JournalLog {
    /// A log for a search started at `started_ms`. Its file is created with the first row, so searches that
    /// never start (a busy profile, a missing browser) leave nothing behind and prune no history.
    pub fn open(root: &Path, profile: &str, started_ms: u64) -> Option<Self> {
        let directory = directory(root, profile)?;
        fs::create_dir_all(&directory).ok()?;
        Some(Self { directory, started_ms, file: None, written: Default::default() })
    }
    fn file(&mut self) -> Option<&mut File> {
        if self.file.is_none() {
            let mut options = OpenOptions::new();
            options.create(true).append(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
            self.file = Some(options.open(self.directory.join(format!("{:020}.jsonl", self.started_ms))).ok()?);
            for old in files_newest_first(&self.directory).into_iter().skip(KEPT_SEARCHES) {
                let _ = fs::remove_file(old);
            }
        }
        self.file.as_mut()
    }
    /// Append the rows that are new or changed since the last call.
    pub fn record<T: Serialize>(&mut self, rows: &[T]) {
        for row in rows {
            let Ok(value) = serde_json::to_value(row) else { continue };
            let Some(id) = value["id"].as_u64() else { continue };
            let line = value.to_string();
            if self.written.get(&id) == Some(&line) {
                continue;
            }
            let Some(file) = self.file() else { return };
            if writeln!(file, "{line}").is_err() {
                return;
            }
            self.written.insert(id, line);
        }
    }
}

/// The latest `limit` rows of the profile across its searches, oldest first. A row stored several times
/// (it changed) counts once, as its last version. Row ids are made unique across searches.
pub fn load_history(root: &Path, profile: &str, limit: usize) -> Vec<Value> {
    let Some(directory) = directory(root, profile) else { return Vec::new() };
    let mut history: Vec<Value> = Vec::new();
    for file in files_newest_first(&directory) {
        let Ok(stream) = File::open(&file) else { continue };
        // Ids restart at 1 in every search; the start (in seconds) keeps them unique and stable across searches.
        let seconds = file
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u64>().ok())
            .map_or(0, |ms| ms / 1000);
        let mut rows: Vec<(u64, Value)> = Vec::new();
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            let Ok(row) = serde_json::from_str::<Value>(&line) else { continue };
            let Some(id) = row["id"].as_u64() else { continue };
            match rows.iter().position(|(known, _)| *known == id) {
                Some(at) => rows[at].1 = row,
                None => rows.push((id, row)),
            }
        }
        let mut older: Vec<Value> = rows
            .into_iter()
            .map(|(id, mut row)| {
                row["id"] = Value::from(seconds * 100_000 + id);
                row
            })
            .collect();
        older.append(&mut history);
        history = older;
        if history.len() >= limit {
            break;
        }
    }
    let excess = history.len().saturating_sub(limit);
    history.drain(..excess);
    history
}
