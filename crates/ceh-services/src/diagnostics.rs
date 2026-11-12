//! Passive, bounded local evidence. No network fetch or browser action exists here.
use ceh_core::control::ControlMode;
use fs2::FileExt;
use reqwest::Url;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
pub const MAX_SCRIPT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_RESPONSE_TASKS: usize = 8;
const HOST: &str = "icp.administracionelectronica.gob.es";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticError {
    Unavailable,
    Invalid,
    Busy,
    TooLarge,
}
impl From<std::io::Error> for DiagnosticError {
    fn from(_: std::io::Error) -> Self {
        Self::Unavailable
    }
}
pub struct CapturePolicy {
    pub enabled: bool,
    pub mode: ControlMode,
    pub screen_locked: bool,
    pub first_party: bool,
}
impl CapturePolicy {
    pub fn screenshot(&self) -> bool {
        self.dom_snapshot() && !self.screen_locked
    }
    pub fn dom_snapshot(&self) -> bool {
        self.enabled && self.first_party && self.mode == ControlMode::Automatic
    }
    /// Already-observed HTTP responses can be archived without inspecting or controlling a manual page.
    pub fn passive_response(&self) -> bool {
        self.enabled && self.first_party
    }
}
pub fn script_url(raw: &str) -> Option<String> {
    let url = Url::parse(raw).ok()?;
    if url.scheme() != "https"
        || url.host_str() != Some(HOST)
        || url.port_or_known_default() != Some(443)
    {
        return None;
    }
    let path = url
        .path()
        .split('/')
        .map(|part| part.split(';').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("/");
    if !path.starts_with("/icpplus/") && !path.starts_with("/icpplustieb/") {
        return None;
    }
    Some(format!("https://{HOST}{path}"))
}
pub fn script_reference(url: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(url.split('#').next().unwrap_or("").as_bytes())
    )
}
fn directory(path: &Path) -> Result<(), DiagnosticError> {
    if path
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err(DiagnosticError::Invalid);
    }
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
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), DiagnosticError> {
    let parent = path.parent().ok_or(DiagnosticError::Invalid)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path)
        .map_err(|_| DiagnosticError::Unavailable)?;
    Ok(())
}
fn manifest(directory: &Path) -> Result<Vec<Value>, DiagnosticError> {
    let path = directory.join("resources.json");
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(DiagnosticError::Unavailable),
    };
    let mut bytes = Vec::new();
    file.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(DiagnosticError::TooLarge);
    }
    let records: Vec<Value> =
        serde_json::from_slice(&bytes).map_err(|_| DiagnosticError::Invalid)?;
    for record in &records {
        for field in ["reference", "sha256"] {
            if !record[field].as_str().is_some_and(|v| {
                v.len() == 64
                    && v.bytes()
                        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            }) {
                return Err(DiagnosticError::Invalid);
            }
        }
    }
    Ok(records)
}
pub struct ScriptSources {
    pub url: String,
    pub base: Option<String>,
    pub sources: Vec<String>,
}
pub struct ScriptArchive {
    root: PathBuf,
    redactor: Sanitizer,
}
impl ScriptArchive {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.into(),
            redactor: Sanitizer::default(),
        }
    }
    pub fn register_sensitive(&mut self, values: impl IntoIterator<Item = String>) {
        self.redactor.register(values);
    }
    fn lock(&self) -> Result<File, DiagnosticError> {
        directory(&self.root)?;
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(self.root.join(".resources.lock"))?;
        let deadline = Instant::now() + Duration::from_millis(100);
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => return Ok(file),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.raw_os_error() == fs2::lock_contended_error().raw_os_error() => {
                    if Instant::now() >= deadline {
                        return Err(DiagnosticError::Busy);
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => return Err(DiagnosticError::Unavailable),
            }
        }
    }
    fn own_session(&self, path: &Path) -> bool {
        path.parent() == Some(self.root.join("sessions").as_path())
            && path
                .file_name()
                .is_some_and(|s| s.to_str().is_some_and(valid_session))
    }
    pub fn prepare_session(
        &self,
        name: &str,
        retention_days: u32,
        now: f64,
        cleanup: bool,
    ) -> Result<PathBuf, DiagnosticError> {
        if !valid_session(name) || !now.is_finite() || !(0.0..1e13).contains(&now) {
            return Err(DiagnosticError::Invalid);
        }
        let _lock = self.lock()?;
        directory(&self.root.join("sessions"))?;
        let current = self.root.join("sessions").join(name);
        directory(&current)?;
        filetime::set_file_mtime(&current, filetime::FileTime::from_unix_time(now as i64, 0))?;
        if !cleanup {
            return Ok(current);
        }
        let cutoff = now - f64::from(retention_days) * 86400.0;
        for entry in fs::read_dir(self.root.join("sessions"))? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if entry.path() != current && kind.is_dir() && !kind.is_symlink() {
                let modified = entry
                    .metadata()?
                    .modified()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| DiagnosticError::Invalid)?
                    .as_secs_f64();
                if modified < cutoff {
                    fs::remove_dir_all(entry.path())?;
                }
            }
        }
        let mut referenced = std::collections::BTreeSet::new();
        for entry in fs::read_dir(self.root.join("sessions"))? {
            let entry = entry?;
            if entry.file_type()?.is_dir() && !entry.file_type()?.is_symlink() {
                let records = match manifest(&entry.path()) {
                    Ok(records) => records,
                    Err(_) => return Ok(current),
                };
                for record in records {
                    referenced.insert(record["sha256"].as_str().unwrap().to_owned());
                }
            }
        }
        let resources = self.root.join("resources");
        if resources.exists() {
            directory(&resources)?;
            for entry in fs::read_dir(resources)? {
                let entry = entry?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if entry.file_type()?.is_file()
                    && name.ends_with(".js.gz")
                    && !referenced.contains(name.trim_end_matches(".js.gz"))
                {
                    fs::remove_file(entry.path())?;
                }
            }
        }
        Ok(current)
    }
    pub fn save_script(
        &self,
        session: &Path,
        url: &str,
        body: &[u8],
        status: u16,
        content_type: &str,
        now: f64,
    ) -> Result<Option<String>, DiagnosticError> {
        if !self.own_session(session) || !now.is_finite() || !(0.0..1e13).contains(&now) {
            return Err(DiagnosticError::Invalid);
        }
        if body.is_empty() || body.len() > MAX_SCRIPT_BYTES {
            return Ok(None);
        }
        let Some(safe) = script_url(url) else {
            return Ok(None);
        };
        if self.redactor.contains_secret(&safe)
            || self
                .redactor
                .contains_secret(&String::from_utf8_lossy(body))
        {
            return Ok(None);
        }
        let mime = content_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if ![
            "application/javascript",
            "text/javascript",
            "application/x-javascript",
            "application/ecmascript",
            "text/ecmascript",
        ]
        .contains(&mime.as_str())
        {
            return Ok(None);
        }
        if body
            .iter()
            .find(|c| !c.is_ascii_whitespace())
            .is_some_and(|c| *c == b'<')
            || String::from_utf8_lossy(&body[..body.len().min(4096)])
                .replace(' ', "")
                .to_ascii_lowercase()
                .contains("requestrejected")
        {
            return Ok(None);
        }
        let _lock = self.lock()?;
        directory(&self.root.join("sessions"))?;
        directory(session)?;
        let objects = self.root.join("resources");
        directory(&objects)?;
        let digest = format!("{:x}", Sha256::digest(body));
        let target = objects.join(format!("{digest}.js.gz"));
        if !target.exists() {
            let mut gzip = flate2::GzBuilder::new()
                .mtime(0)
                .write(Vec::new(), flate2::Compression::default());
            gzip.write_all(body)?;
            atomic_write(&target, &gzip.finish()?)?;
        }
        let mut records = manifest(session)?;
        let reference = script_reference(url);
        let index=records.iter().position(|r|r["reference"]==reference&&r["sha256"]==digest).unwrap_or_else(||{records.push(json!({"url":safe,"reference":reference,"sha256":digest,"first_seen":now,"load_count":0}));records.len()-1});
        let record = &mut records[index];
        let count = record["load_count"]
            .as_u64()
            .ok_or(DiagnosticError::Invalid)?
            .checked_add(1)
            .ok_or(DiagnosticError::Invalid)?;
        record["last_seen"] = json!(now);
        record["load_count"] = json!(count);
        record["status"] = json!(status);
        record["content_type"] = json!(mime);
        atomic_write(
            &session.join("resources.json"),
            &serde_json::to_vec_pretty(&records).map_err(|_| DiagnosticError::Invalid)?,
        )?;
        filetime::set_file_mtime(session, filetime::FileTime::from_unix_time(now as i64, 0))?;
        Ok(Some(digest))
    }
    pub fn references(
        &self,
        session: &Path,
        documents: &[ScriptSources],
    ) -> Result<Value, DiagnosticError> {
        if !self.own_session(session) {
            return Err(DiagnosticError::Invalid);
        }
        let _lock = self.lock()?;
        let mut records = manifest(session)?;
        records.sort_by(|a, b| {
            a["last_seen"]
                .as_f64()
                .unwrap_or(0.0)
                .total_cmp(&b["last_seen"].as_f64().unwrap_or(0.0))
        });
        let mut latest = std::collections::BTreeMap::new();
        for record in records {
            latest.insert(
                record["reference"].as_str().unwrap().to_owned(),
                record["sha256"].clone(),
            );
        }
        let mut result = Vec::new();
        for document in documents {
            let Ok(url) = Url::parse(&document.url) else {
                continue;
            };
            let base = document
                .base
                .as_ref()
                .and_then(|base| url.join(base).ok())
                .unwrap_or(url);
            for source in &document.sources {
                let Ok(url) = base.join(source) else { continue };
                if let Some(safe) = script_url(url.as_str()) {
                    let reference = script_reference(url.as_str());
                    let digest = latest.get(&reference);
                    result.push(json!({"url":safe,"reference":reference,"sha256":digest,"state":if digest.is_some(){"captured"}else{"missing"}}));
                }
            }
        }
        Ok(json!(result))
    }
}
fn valid_session(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 160
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}

static WORD: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"^\w$").unwrap());
static BOT: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"[0-9]{5,12}:[A-Za-z0-9_-]{20,}").unwrap());
static NIE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"(?i)[XYZ]\d{7}[A-Z]").unwrap());
static URL: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r#"https?://[^\s"'<>]+"#).unwrap());
const PRIVATE_KEYS: &[&str] = &[
    "authorization",
    "bot_token",
    "chat_id",
    "cookie",
    "set-cookie",
    "contact_phone",
    "contact_email",
    "phone",
    "email",
    "csrf",
    "document_number",
    "full_name",
    "token",
    "password",
    "pin",
    "access_token",
    "refresh_token",
    "sessionid",
];
#[derive(Default)]
pub struct Sanitizer {
    known: Vec<zeroize::Zeroizing<String>>,
}
impl Sanitizer {
    pub fn contains_secret(&self, value: &str) -> bool {
        self.known
            .iter()
            .any(|secret| value.contains(secret.as_str()))
            || replace_bounded(value, &BOT, true) != value
            || replace_bounded(value, &NIE, false) != value
    }
    pub fn register(&mut self, values: impl IntoIterator<Item = String>) {
        for value in values {
            if value.chars().count() >= 3 && !self.known.iter().any(|s| **s == value) {
                self.known.push(zeroize::Zeroizing::new(value));
            }
        }
        self.known
            .sort_by_key(|v| std::cmp::Reverse(v.chars().count()));
    }
    pub fn sanitize(&self, value: &str) -> String {
        let mut value = value.replace('\r', "\\r").replace('\n', "\\n");
        for secret in &self.known {
            value = value.replace(secret.as_str(), "***");
        }
        value = replace_bounded(&value, &BOT, true);
        value = replace_bounded(&value, &NIE, false);
        URL.replace_all(&value, |capture: &regex::Captures<'_>| {
            let raw = &capture[0];
            let trimmed = raw.trim_end_matches(['.', ',', ')', ';', ']']);
            let trailing = &raw[trimmed.len()..];
            if let Ok(mut url) = Url::parse(trimmed) {
                url.set_query(None);
                url.set_fragment(None);
                let _ = url.set_username("");
                let _ = url.set_password(None);
                format!("{url}{trailing}")
            } else {
                format!("[invalid-url]{trailing}")
            }
        })
        .into_owned()
    }
    pub fn field(&self, key: &str, value: &Value) -> String {
        let key = key
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || "_.-".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        let raw = if PRIVATE_KEYS.contains(&key.to_ascii_lowercase().as_str()) {
            Value::String("***".into())
        } else {
            value.clone()
        };
        let clean = match raw {
            Value::String(text) => self.sanitize(&text),
            value => self.sanitize(&value.to_string()),
        };
        format!(
            "{key}={}",
            serde_json::to_string(&clean).expect("string JSON")
        )
    }
    pub fn clean_startup(&self, value: &str, home: &str) -> String {
        let mut value = if home.is_empty() {
            value.to_owned()
        } else {
            value.replace(home, "~")
        };
        for (pattern, replacement) in [
            (r"/Users/[^/\s]+", "/Users/[user]"),
            (r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+", "[email]"),
            (
                r"(?im)\b(?:cookie|set-cookie|authorization)\s*:[^\r\n]*",
                "[private header]",
            ),
            (
                r"(?i)\b(?:access_token|refresh_token|password|pin|sessionid|csrf)\s*[=:]\s*[^\s;]+",
                "[private value]",
            ),
        ] {
            value = regex::Regex::new(pattern)
                .expect("fixed scrub pattern")
                .replace_all(&value, replacement)
                .into_owned();
        }
        self.sanitize(&value).chars().take(16000).collect()
    }
    fn scrub(&self, value: &mut Value, home: &str) {
        match value {
            Value::String(text) => *text = self.clean_startup(text, home),
            Value::Array(values) => values.iter_mut().for_each(|value| self.scrub(value, home)),
            Value::Object(values) => {
                for (key, value) in values {
                    if PRIVATE_KEYS.contains(&key.to_ascii_lowercase().as_str()) {
                        *value = json!("***");
                    } else {
                        self.scrub(value, home);
                    }
                }
            }
            _ => {}
        }
    }
}
fn replace_bounded(value: &str, regex: &regex::Regex, word_boundary: bool) -> String {
    let mut output = String::new();
    let mut end = 0;
    for found in regex.find_iter(value) {
        let before = value[..found.start()].chars().next_back();
        let after = value[found.end()..].chars().next();
        let blocked = |c: Option<char>| {
            c.is_some_and(|c| {
                if word_boundary {
                    WORD.is_match(&c.to_string())
                } else {
                    c.is_ascii_alphanumeric()
                }
            })
        };
        if blocked(before) || blocked(after) {
            continue;
        }
        output.push_str(&value[end..found.start()]);
        output.push_str("***");
        end = found.end();
    }
    output.push_str(&value[end..]);
    output
}
static REPORT_ID: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"^[0-9]{8}T[0-9]{6}-[a-f0-9]{8}$").unwrap());
pub struct StartupReports {
    root: PathBuf,
    sanitizer: Sanitizer,
    home: String,
}
impl StartupReports {
    pub fn new(root: &Path, sanitizer: Sanitizer, home: &str) -> Self {
        Self {
            root: root.into(),
            sanitizer,
            home: home.into(),
        }
    }
    fn scrub_report(&self, report: &mut Value) -> Result<(), DiagnosticError> {
        let allowed = [
            "version",
            "time",
            "system",
            "release",
            "macos",
            "architecture",
            "frozen",
            "executable",
            "phase",
            "browser",
            "bundle_checks",
            "error_type",
            "message",
            "traceback",
        ];
        report
            .as_object_mut()
            .ok_or(DiagnosticError::Invalid)?
            .retain(|key, _| allowed.contains(&key.as_str()));
        self.sanitizer.scrub(report, &self.home);
        Ok(())
    }
    pub fn save(
        &self,
        mut report: Value,
        when: chrono::DateTime<chrono::Utc>,
    ) -> Result<String, DiagnosticError> {
        self.scrub_report(&mut report)?;
        let bytes = serde_json::to_vec_pretty(&report).map_err(|_| DiagnosticError::Invalid)?;
        if bytes.len() > 128 * 1024 {
            return Err(DiagnosticError::TooLarge);
        }
        directory(&self.root)?;
        let mut old = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_file()
                && path.extension().is_some_and(|v| v == "json")
                && path
                    .file_stem()
                    .and_then(|v| v.to_str())
                    .is_some_and(|v| REPORT_ID.is_match(v))
            {
                old.push((entry.metadata()?.modified()?, path));
            }
        }
        old.sort_by_key(|a| std::cmp::Reverse(a.0));
        for (_, path) in old.into_iter().skip(9) {
            fs::remove_file(path)?;
        }
        let id = format!(
            "{}-{}",
            when.format("%Y%m%dT%H%M%S"),
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        );
        if !REPORT_ID.is_match(&id) {
            return Err(DiagnosticError::Invalid);
        }
        let mut file = tempfile::NamedTempFile::new_in(&self.root)?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        file.persist_noclobber(self.root.join(format!("{id}.json")))
            .map_err(|_| DiagnosticError::Unavailable)?;
        Ok(id)
    }
    pub fn read(&self, id: &str) -> Result<Option<Value>, DiagnosticError> {
        if !REPORT_ID.is_match(id) {
            return Ok(None);
        }
        let path = self.root.join(format!("{id}.json"));
        if path
            .symlink_metadata()
            .is_ok_and(|v| v.file_type().is_symlink())
        {
            return Ok(None);
        }
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(DiagnosticError::Unavailable),
        };
        let mut bytes = Vec::new();
        file.take(128 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 128 * 1024 {
            return Ok(None);
        }
        let mut value: Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        if !value.is_object() {
            return Ok(None);
        }
        self.scrub_report(&mut value)?;
        Ok(Some(value))
    }
    pub fn clipboard(&self, id: &str, mut report: Value) -> Result<String, DiagnosticError> {
        if !REPORT_ID.is_match(id) {
            return Err(DiagnosticError::Invalid);
        }
        self.scrub_report(&mut report)?;
        Ok(format!(
            "SantaCita — CEH-{id}\n{}",
            serde_json::to_string_pretty(&report).map_err(|_| DiagnosticError::Invalid)?
        ))
    }
}

#[derive(Clone, Copy)]
pub enum ResponseKind {
    Navigation,
    Script,
}
#[derive(Default)]
pub struct ResponseBudget {
    navigation: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    script: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
pub struct ResponsePermit(std::sync::Arc<std::sync::atomic::AtomicUsize>);
impl ResponseBudget {
    pub fn acquire(&self, kind: ResponseKind) -> Option<ResponsePermit> {
        let counter = match kind {
            ResponseKind::Navigation => &self.navigation,
            ResponseKind::Script => &self.script,
        };
        let mut value = counter.load(std::sync::atomic::Ordering::Acquire);
        loop {
            if value >= MAX_RESPONSE_TASKS {
                return None;
            }
            match counter.compare_exchange_weak(
                value,
                value + 1,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(actual) => value = actual,
            }
        }
        Some(ResponsePermit(counter.clone()))
    }
}
impl Drop for ResponsePermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}
pub struct CaptureTicket {
    guard: ceh_core::model::DocumentGuard,
    fingerprint: String,
    deadline: u64,
    image: bool,
}
#[derive(Default)]
pub struct CaptureTracker {
    entries: std::collections::VecDeque<(ceh_core::model::DocumentGuard, String, u8, bool)>,
}
impl CaptureTracker {
    pub fn begin(
        &mut self,
        guard: &ceh_core::model::DocumentGuard,
        fingerprint: &str,
        snapshot: &ceh_core::control::ControlSnapshot,
        policy: &CapturePolicy,
        now: u64,
        image: bool,
    ) -> Option<CaptureTicket> {
        if fingerprint.len() != 64
            || !fingerprint
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return None;
        }
        if !policy.dom_snapshot()
            || (image && !policy.screenshot())
            || snapshot.mode() != ControlMode::Automatic
            || snapshot.session() != &guard.session
            || snapshot.epoch() != guard.epoch
        {
            return None;
        }
        let index = if let Some(index) = self
            .entries
            .iter()
            .position(|(old, hash, _, _)| old == guard && hash == fingerprint)
        {
            index
        } else {
            if self.entries.len() >= 2048 {
                self.entries.pop_front();
            }
            self.entries
                .push_back((guard.clone(), fingerprint.into(), 0, false));
            self.entries.len() - 1
        };
        let entry = &mut self.entries[index];
        if entry.2 >= 2 || entry.3 {
            return None;
        }
        entry.2 += 1;
        Some(CaptureTicket {
            guard: guard.clone(),
            fingerprint: fingerprint.into(),
            deadline: now.checked_add(10_000)?,
            image,
        })
    }
    pub fn finish(
        &mut self,
        ticket: CaptureTicket,
        current: &ceh_core::model::DocumentGuard,
        snapshot: &ceh_core::control::ControlSnapshot,
        policy: &CapturePolicy,
        now: u64,
        saved: bool,
    ) -> bool {
        let accepted = saved
            && now <= ticket.deadline
            && policy.dom_snapshot()
            && (!ticket.image || policy.screenshot())
            && snapshot.mode() == ControlMode::Automatic
            && snapshot.session() == &ticket.guard.session
            && snapshot.epoch() == ticket.guard.epoch
            && current == &ticket.guard;
        if accepted
            && let Some(entry) = self
                .entries
                .iter_mut()
                .find(|(guard, hash, _, _)| guard == &ticket.guard && hash == &ticket.fingerprint)
        {
            entry.3 = true;
        }
        accepted
    }
}

pub fn inspect_path(path: &Path, sanitizer: &Sanitizer, home: &str) -> Value {
    let clean = |path: &Path| sanitizer.clean_startup(&path.to_string_lossy(), home);
    let mut result = json!({"path":clean(path),"exists":false,"executable":false});
    let error = |error: &std::io::Error| json!({"type":"IoError","errno":error.raw_os_error(),"reason":format!("{:?}",error.kind())});
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            result["is_symlink"] = json!(metadata.file_type().is_symlink());
            if metadata.file_type().is_symlink() {
                match fs::read_link(path) {
                    Ok(target) => result["link_target"] = json!(clean(&target)),
                    Err(error_value) => result["lstat_error"] = error(&error_value),
                }
            }
        }
        Err(value) => result["lstat_error"] = error(&value),
    }
    match fs::metadata(path) {
        Ok(metadata) => {
            result["exists"] = json!(true);
            result["is_file"] = json!(metadata.is_file());
            #[cfg(unix)]
            {
                use std::os::unix::{ffi::OsStrExt, fs::PermissionsExt};
                result["mode"] = json!(format!("{:#o}", metadata.permissions().mode() & 0o7777));
                let executable =
                    std::ffi::CString::new(path.as_os_str().as_bytes()).is_ok_and(|value| {
                        // CString guarantees a valid, NUL-terminated path for this read-only OS check.
                        unsafe { libc::access(value.as_ptr(), libc::X_OK) == 0 }
                    });
                result["executable"] = json!(executable);
            }
            #[cfg(not(unix))]
            {
                result["executable"] = json!(true);
            }
        }
        Err(value) => result["stat_error"] = error(&value),
    }
    match path.canonicalize() {
        Ok(value) => result["resolved"] = json!(clean(&value)),
        Err(value) => result["resolve_error"] = error(&value),
    }
    result
}
