use ceh_browser::{bidi::BidiConnection, passive, session_driver::{NativeStep, SessionDriver}};
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
}

impl SearchDiagnostics {
    pub(crate) fn new(root: &Path, session: String, profile: &ProfileDraft) -> Self {
        let mut archive = ScriptArchive::new(&root.join("data/diagnostics"));
        archive.register_sensitive([
            profile.document_number.clone(), profile.full_name.clone(),
            profile.phone.clone(), profile.email.clone(),
        ]);
        Self { archive, session, directory: None, collector: None, enabled: false, documents: HashSet::new(), index: 0 }
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
            return;
        }
        let Ok(now) = crate::current_time() else { return };
        self.directory = self.archive.prepare_session(&self.session, 3, now.timestamp() as f64, false).ok();
        if self.directory.is_none() {
            return;
        }
        self.collector = connection.call("network.addDataCollector", json!({
            "dataTypes":["response"],"maxEncodedDataSize":passive::MAX_RESPONSE_BYTES,
            "collectorType":"blob","contexts":[context]
        })).ok().and_then(|result| result["collector"].as_str().map(str::to_owned));
        let _ = driver.set_passive_diagnostics(self.collector.is_some());
    }

    pub(crate) fn stop_collector(&mut self, connection: &mut BidiConnection, driver: &mut SessionDriver) {
        let _ = driver.set_passive_diagnostics(false);
        if let Some(collector) = self.collector.take() {
            let _ = connection.call("network.removeDataCollector", json!({"collector":collector}));
        }
    }

    pub(crate) fn observe(&mut self, step: &NativeStep, connection: &mut BidiConnection, driver: &mut SessionDriver) {
        let Some(directory) = self.directory.as_ref().filter(|_| self.enabled) else { return };
        let Ok(now) = crate::current_time() else { return };
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
                    let _ = self.archive.save_script(directory, response.url(), body.as_bytes(), response.status(), response.mime(), now.timestamp() as f64);
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
