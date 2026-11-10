//! Small local startup-failure reports (Python `app/desktop/startup_report.py`): never page content or
//! profile data, scrubbed again when read and when copied, at most ten kept.
use ceh_services::diagnostics::{Sanitizer, StartupReports};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn directory(root: &Path) -> PathBuf {
    root.join("data/startup-errors")
}
fn reports(root: &Path, home: &str) -> StartupReports {
    StartupReports::new(&directory(root), Sanitizer::default(), home)
}
/// Phases the window distinguishes: the browser could not start, or the search failed afterwards.
pub fn phase_of(code: &str) -> &'static str {
    match code {
        "firefox_not_installed" | "browser_profile_unavailable" | "browser_start_failed" | "browser_certificate_cache_failed" => "browser_launch",
        _ => "worker",
    }
}
/// Save a report for a failed search start and return its id; `None` when nothing could be written.
pub fn save_failure(root: &Path, home: &str, code: &str, firefox_found: bool, now: chrono::DateTime<chrono::Utc>) -> Option<String> {
    let report = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "time": now.to_rfc3339(),
        "system": std::env::consts::OS,
        "architecture": std::env::consts::ARCH,
        "frozen": false,
        "executable": std::env::current_exe().ok().map(|p| p.display().to_string()).unwrap_or_default(),
        "phase": phase_of(code),
        "browser": {"family": "firefox", "exists": firefox_found},
        "bundle_checks": {},
        "error_type": code,
        "message": code,
        "traceback": [],
    });
    reports(root, home).save(report, now).ok()
}
/// The text copied to the clipboard for report `id`, scrubbed again; `Err` when it is gone or invalid.
pub fn clipboard_text(root: &Path, home: &str, id: &str) -> Result<String, &'static str> {
    let store = reports(root, home);
    let report: Value = store.read(id).map_err(|_| "report_unavailable")?.ok_or("report_unavailable")?;
    store.clipboard(id, report).map_err(|_| "report_unavailable")
}
