//! `timeline.jsonl` of a diagnostics session: what the app did and saw, one JSON line per event, with
//! Madrid times. Rows carry names, counts, durations and short hashes only: never typed values, page text,
//! document or session ids, cookie or header values (see the builders below and their contract test).
use ceh_core::{
    flow::FlowEffect,
    model::{ActionKind, DispatchOutcome, DocumentId, PageKind, RequestMethod},
    recovery::RecoveryAction,
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Madrid ISO time with milliseconds for an event instant (ms since the epoch); the clock when absent.
pub fn madrid_text(epoch_ms: Option<i64>) -> String {
    let instant = epoch_ms.and_then(chrono::DateTime::from_timestamp_millis).or_else(|| crate::current_time().ok());
    instant
        .map(|at| at.with_timezone(&chrono_tz::Europe::Madrid).to_rfc3339_opts(chrono::SecondsFormat::Millis, false))
        .unwrap_or_default()
}
fn snake<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
}
fn action_name(action: ActionKind) -> String {
    snake(&action)
}
fn effect_name(effect: &FlowEffect) -> String {
    match effect {
        FlowEffect::Prepare { action, .. } => format!("prepare:{}", action_name(*action)),
        FlowEffect::Activate { action, .. } => format!("activate:{}", action_name(*action)),
        FlowEffect::Recover { action, .. } => match action {
            RecoveryAction::Restart { clear_profile: true, .. } => "recover:restart(clear_profile)".into(),
            RecoveryAction::Restart { reopen: true, .. } => "recover:restart(reopen)".into(),
            RecoveryAction::Restart { .. } => "recover:restart(new_pass)".into(),
            RecoveryAction::Hold => "recover:hold".into(),
            RecoveryAction::Reload => "recover:reload".into(),
            RecoveryAction::ActivateOwnControl => "recover:own_control".into(),
            RecoveryAction::GetCurrent => "recover:get_current".into(),
            RecoveryAction::GetEntry => "recover:get_entry".into(),
        },
        FlowEffect::Silence => "silence".into(),
        FlowEffect::AcceptCookies { .. } => "accept_cookies".into(),
        FlowEffect::SelectCertificate { .. } => "select_certificate".into(),
        FlowEffect::SelectClaveMethod { .. } => "select_clave_method".into(),
        FlowEffect::FetchApplicant { .. } => "fetch_applicant".into(),
        FlowEffect::EnterAuthentication { .. } => "enter_authentication".into(),
        FlowEffect::Reload { .. } => "reload".into(),
        FlowEffect::WaitUntil(_) => "wait_until".into(),
        FlowEffect::ReobserveAt(_) => "reobserve".into(),
        FlowEffect::Hold => "hold".into(),
        FlowEffect::Handoff => "handoff".into(),
        FlowEffect::Alert => "alert".into(),
    }
}
/// A short stable tag of a document id: repeated pages are recognizable, the id itself is not recorded.
fn short_hash(text: &str) -> String {
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    format!("{:08x}", hash >> 32)
}

pub fn step_row(effects: &[FlowEffect], delay_ms: Option<u64>, receipts: &[(ActionKind, DispatchOutcome)], elapsed_ms: u64) -> Value {
    json!({
        "effects": effects.iter().map(effect_name).collect::<Vec<_>>(),
        "delay_ms": delay_ms,
        "elapsed_ms": elapsed_ms,
        "receipts": receipts.iter().map(|(action, outcome)| json!({"action": action_name(*action), "outcome": snake(outcome)})).collect::<Vec<_>>(),
    })
}
/// A step that ended in an error: the effects and the pause are what forensics needs most.
pub fn step_error_row(effects: &[FlowEffect], delay_ms: Option<u64>, elapsed_ms: u64, error: &str) -> Value {
    let mut row = step_row(effects, delay_ms, &[], elapsed_ms);
    row["error"] = json!(error);
    row
}
pub fn page_row(kind: PageKind, method: RequestMethod, document: &DocumentId) -> Value {
    json!({"page": snake(&kind), "method": snake(&method), "doc": short_hash(document.as_str())})
}
#[derive(Clone)]
pub struct SessionHeader {
    pub app_version: String,
    pub firefox_version: String,
    pub os: String,
    pub profile_persistent: bool,
    pub profile_age_days: Option<u64>,
    pub certificate_cache: String,
    pub action_delay_ms: [u64; 2],
    pub first_bootstrap_ms: u64,
    pub typing_ms: u64,
    pub contact_typing_ms: u64,
}
pub fn header_row(header: &SessionHeader) -> Value {
    json!({
        "app": header.app_version, "firefox": header.firefox_version, "os": header.os,
        "profile": {"persistent": header.profile_persistent, "age_days": header.profile_age_days},
        "certificate_cache": header.certificate_cache,
        "timings": {"action_delay_ms": header.action_delay_ms, "first_bootstrap_ms": header.first_bootstrap_ms,
                    "typing_ms": header.typing_ms, "contact_typing_ms": header.contact_typing_ms},
    })
}
/// A sanitized transport command (`BidiConnection::take_command_log`) as a timeline row.
pub fn command_row(raw: &Value) -> Option<Value> {
    let at = raw["t_ms"].as_i64()?;
    Some(json!({
        "t": madrid_text(Some(at)), "kind": "cmd", "method": raw["method"], "summary": raw["summary"],
        "ok": raw["ok"], "duration_ms": raw["duration_ms"],
    }))
}
pub fn block_row(kind: PageKind, method: RequestMethod, marker: Option<&str>, screenshot: &str) -> Value {
    json!({"page": snake(&kind), "method": snake(&method), "marker": marker, "screenshot": screenshot})
}

/// Appends rows to `<session directory>/timeline.jsonl`; a timeline without a directory writes nothing.
pub struct SessionTimeline {
    path: Option<PathBuf>,
}
impl SessionTimeline {
    pub fn new(directory: Option<&Path>) -> Self {
        Self { path: directory.map(|d| d.join("timeline.jsonl")) }
    }
    pub fn is_enabled(&self) -> bool {
        self.path.is_some()
    }
    pub fn append(&self, kind: &str, data: Value) {
        let Some(path) = &self.path else { return };
        let mut row = serde_json::Map::new();
        row.insert("t".into(), json!(madrid_text(None)));
        row.insert("kind".into(), json!(kind));
        if let Value::Object(fields) = data {
            row.extend(fields);
        }
        self.write(&Value::Object(row), path);
    }
    /// A row that already has its own `t` and `kind` (commands carry the instant they were sent).
    pub fn append_row(&self, row: Value) {
        if let Some(path) = &self.path {
            self.write(&row, path);
        }
    }
    fn write(&self, row: &Value, path: &Path) {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        if let Ok(mut file) = options.open(path) {
            let _ = writeln!(file, "{row}");
        }
    }
}
