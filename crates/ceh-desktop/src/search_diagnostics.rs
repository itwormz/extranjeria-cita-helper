use ceh_browser::{bidi::BidiConnection, passive, session_driver::{NativeStep, SessionDriver}};
use crate::session_timeline::{self, SessionHeader, SessionTimeline};
use ceh_core::profiles::ProfileDraft;
use ceh_services::diagnostics::ScriptArchive;
use serde_json::json;
use std::{collections::HashSet, path::{Path, PathBuf}};

pub(crate) struct SearchDiagnostics {
    archive: ScriptArchive,
    session: String,
    directory: Option<PathBuf>,
    collector: Option<String>,
    enabled: bool,
    documents: HashSet<ceh_core::model::DocumentId>,
    index: u32,
    /// What the app did and saw, for reconstructing the history of a block (`timeline.jsonl`).
    timeline: SessionTimeline,
    /// The page last written to the timeline: a document is written again when its kind changes (it may
    /// be seen as `loading` first and as a block page a moment later).
    last_page: Option<(ceh_core::model::DocumentId, ceh_core::model::PageKind)>,
    blocked_documents: HashSet<ceh_core::model::DocumentId>,
    blocks: u32,
    header: Option<SessionHeader>,
}

impl SearchDiagnostics {
    pub(crate) fn new(root: &Path, session: String, profile: &ProfileDraft) -> Self {
        let mut archive = ScriptArchive::new(&root.join("data/diagnostics"));
        archive.register_sensitive([
            profile.document_number.clone(), profile.full_name.clone(),
            profile.phone.clone(), profile.email.clone(),
        ]);
        Self { archive, session, directory: None, collector: None, enabled: false, documents: HashSet::new(), index: 0,
            timeline: SessionTimeline::new(None), last_page: None,
            blocked_documents: HashSet::new(), blocks: 0, header: None }
    }

    pub(crate) fn set_enabled(
        &mut self,
        enabled: bool,
        connection: &mut BidiConnection,
        driver: &mut SessionDriver,
        context: &str,
    ) {
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        if !enabled {
            self.stop_collector(connection, driver);
            self.finish(driver);
            // Only now: `finish` writes the end row and the last commands.
            self.timeline = SessionTimeline::new(None);
            return;
        }
        let Ok(now) = crate::current_time() else { return };
        self.directory = self.archive.prepare_session(&self.session, 3, now.timestamp() as f64, false).ok();
        if self.directory.is_none() {
            return;
        }
        self.timeline = SessionTimeline::new(self.directory.as_deref());
        // Commands collected while diagnostics were off are not history of this timeline.
        let _ = connection.take_command_log();
        if let Some(header) = self.header.clone() {
            self.session_start(&header, connection, context);
        }
        self.collector = connection.call("network.addDataCollector", json!({
            "dataTypes":["response"],"maxEncodedDataSize":passive::MAX_RESPONSE_BYTES,
            "collectorType":"blob","contexts":[context]
        })).ok().and_then(|result| result["collector"].as_str().map(str::to_owned));
        let _ = driver.set_passive_diagnostics(self.collector.is_some());
        // Python `icp-network.jsonl`: traffic to the appointment site, written next to the session snapshots.
        driver.set_network_journal(Some(Box::new(madrid_text)));
    }

    /// A new pass builds a new driver in the same browser: give it the collection that is already running.
    pub(crate) fn rebind(&mut self, driver: &mut SessionDriver) {
        if self.enabled && self.directory.is_some() {
            let _ = driver.set_passive_diagnostics(self.collector.is_some());
            driver.set_network_journal(Some(Box::new(madrid_text)));
        }
    }

    /// The facts of the session header; written when diagnostics are (or become) enabled.
    pub(crate) fn set_header(&mut self, header: SessionHeader) {
        self.header = Some(header);
    }
    /// Session header, the page environment and the site's cookie names at the start of the session.
    pub(crate) fn session_start(&self, header: &SessionHeader, connection: &mut BidiConnection, context: &str) {
        if !self.timeline.is_enabled() {
            return;
        }
        self.timeline.append("session", session_timeline::header_row(header));
        self.environment("start", connection, context);
        self.cookies("start", connection);
    }
    pub(crate) fn environment(&self, why: &str, connection: &mut BidiConnection, context: &str) {
        if self.timeline.is_enabled() {
            let environment = ceh_browser::evidence::read_environment(connection, context);
            self.timeline.append("environment", json!({"why": why, "environment": environment}));
        }
    }
    pub(crate) fn cookies(&self, why: &str, connection: &mut BidiConnection) {
        if self.timeline.is_enabled() {
            let cookies = ceh_browser::evidence::read_cookie_names(connection);
            self.timeline.append("cookies", json!({"why": why, "cookies": cookies}));
        }
    }
    /// One executed step: effects, the pause before it, receipts and how long the browser took.
    pub(crate) fn step(&self, row: serde_json::Value) {
        self.timeline.append("step", row);
    }
    /// Commands sent to the browser and header/cookie-name rows of the site's traffic since the last call.
    pub(crate) fn flush_timeline(&self, connection: &mut BidiConnection, driver: &mut SessionDriver) {
        if !self.timeline.is_enabled() {
            return;
        }
        for raw in connection.take_command_log() {
            if let Some(row) = session_timeline::command_row(&raw) {
                self.timeline.append_row(row);
            }
        }
        for mut row in driver.take_network_meta() {
            if let Some(object) = row.as_object_mut() {
                object.insert("kind".into(), json!("net_meta"));
                let at = object.get("time").cloned().unwrap_or(json!(null));
                object.insert("t".into(), at);
                self.timeline.append_row(row);
            }
        }
    }
    /// A block or rate-limit page: marker name, screenshot, environment and cookie names at that moment.
    fn record_block(&mut self, step: &NativeStep, connection: &mut BidiConnection, context: &str) {
        let observation = &step.update().observation;
        self.blocks += 1;
        let screenshot = match (console_is_unlocked(), step.block_image(connection)) {
            (true, Some(image)) => {
                let name = format!("block-{}.png", self.blocks);
                match self.directory.as_ref().map(|d| std::fs::write(d.join(&name), image)) {
                    Some(Ok(())) => name,
                    _ => "write_failed".to_owned(),
                }
            }
            (false, _) => "console_locked".to_owned(),
            (true, None) => "capture_failed".to_owned(),
        };
        self.timeline.append("block", session_timeline::block_row(observation.kind, observation.method, step.block_marker(), &screenshot));
        self.environment("block", connection, context);
        self.cookies("block", connection);
    }

    pub(crate) fn stop_collector(&mut self, connection: &mut BidiConnection, driver: &mut SessionDriver) {
        let _ = driver.set_passive_diagnostics(false);
        if let Some(collector) = self.collector.take() {
            let _ = connection.call("network.removeDataCollector", json!({"collector":collector}));
        }
    }

    /// Write the rows still queued and stop the traffic journal (disabled, or the search ends): the last
    /// requests before a stop or an error are often the ones that matter.
    pub(crate) fn finish(&mut self, driver: &mut SessionDriver) {
        if let Some(directory) = &self.directory {
            append_network_rows(&directory.join("icp-network.jsonl"), driver.take_network_rows());
        }
        self.timeline.append("end", json!({}));
        driver.set_network_journal(None);
    }

    pub(crate) fn observe(&mut self, step: &NativeStep, connection: &mut BidiConnection, driver: &mut SessionDriver) {
        let Some(directory) = self.directory.clone().filter(|_| self.enabled) else { return };
        let Ok(now) = crate::current_time() else { return };
        append_network_rows(&directory.join("icp-network.jsonl"), driver.take_network_rows());
        self.flush_timeline(connection, driver);
        let observation = &step.update().observation;
        let page = (observation.document.clone(), observation.kind);
        if self.last_page.as_ref() != Some(&page) {
            self.last_page = Some(page);
            self.timeline.append("page", session_timeline::page_row(observation.kind, observation.method, &observation.document));
        }
        if matches!(observation.kind, ceh_core::model::PageKind::Blocked | ceh_core::model::PageKind::RateLimited)
            && self.blocked_documents.insert(observation.document.clone())
        {
            let context = step.context().unwrap_or_default().to_owned();
            self.record_block(step, connection, &context);
        }
        let batch = driver.take_diagnostic_responses();
        if batch.overflowed {
            self.stop_collector(connection, driver);
            return;
        }
        if let Some(collector) = &self.collector {
            for response in batch.responses {
                if response.kind() != passive::ResponseKind::Script {
                    let _ = connection.call("network.disownData", json!({
                        "request":response.request(),"dataType":"response","collector":collector
                    }));
                    continue;
                }
                if let Ok(body) = passive::read_response(connection, collector, &response) {
                    let _ = self.archive.save_script(&directory, response.url(), body.as_bytes(), response.status(), response.mime(), now.timestamp() as f64);
                }
            }
        }
        let observation = &step.update().observation;
        if self.index >= 512 || !self.documents.insert(observation.document.clone()) {
            return;
        }
        let Some(html) = step.diagnostic_snapshot(connection) else {
            self.documents.remove(&observation.document);
            return;
        };
        self.index += 1;
        let stem = format!("{:04}", self.index);
        let _ = std::fs::write(directory.join(format!("{stem}.html")), html);
        if console_is_unlocked()
            && let Some(image) = step.diagnostic_image(connection)
            && console_is_unlocked()
        {
            let _ = std::fs::write(directory.join(format!("{stem}.png")), image);
        }
        let metadata = json!({
            "at": now, "page": observation.kind, "method": observation.method,
            "operation_code": observation.operation_code,
        });
        let _ = std::fs::write(directory.join(format!("{stem}.json")), metadata.to_string());
    }
}

pub(crate) use crate::session_timeline::madrid_text;

#[cfg(target_os = "macos")]
fn console_is_unlocked() -> bool {
    use core_foundation::{
        base::{CFType, TCFType},
        boolean::CFBoolean,
        dictionary::CFDictionary,
        string::CFString,
    };
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGSessionCopyCurrentDictionary() -> core_foundation::dictionary::CFDictionaryRef;
    }
    // Core Graphics returns an owned dictionary for the caller's GUI session.
    let raw = unsafe { CGSessionCopyCurrentDictionary() };
    if raw.is_null() { return false; }
    let info = unsafe { CFDictionary::<CFString, CFType>::wrap_under_create_rule(raw) };
    let boolean = |key: &str| info.find(CFString::new(key)).and_then(|value| value.downcast::<CFBoolean>()).map(bool::from);
    boolean("kCGSSessionOnConsoleKey") == Some(true) && boolean("CGSSessionScreenIsLocked") != Some(true)
}

#[cfg(not(target_os = "macos"))]
fn console_is_unlocked() -> bool {
    false
}

/// Append rows as JSON lines; the file is private to the user, like the rest of the diagnostics.
pub(crate) fn append_network_rows(path: &Path, rows: Vec<serde_json::Value>) {
    use std::io::Write;
    if rows.is_empty() {
        return;
    }
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let Ok(mut file) = options.open(path) else { return };
    for row in rows {
        let _ = writeln!(file, "{row}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn network_rows_are_appended_as_private_json_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("icp-network.jsonl");
        append_network_rows(&path, vec![]);
        assert!(!path.exists(), "no file for no rows");
        append_network_rows(&path, vec![json!({"event":"request"}), json!({"event":"response","status":429})]);
        append_network_rows(&path, vec![json!({"event":"failed"})]);
        let lines: Vec<_> = std::fs::read_to_string(&path).unwrap().lines().map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap()).collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1]["status"], 429);
        #[cfg(unix)]
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&path).unwrap().permissions()) & 0o777, 0o600);
    }
}
