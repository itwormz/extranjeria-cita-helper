//! Read-only ownership facts for recovery. No click, scroll, focus or private form values.
use crate::{BrowserError, bidi::BidiConnection, dom::FrameSource, observer::ObservedPage};
use ceh_core::{
    model::{ActionKind, PageKind},
    recognition::recovery_message_kind,
    recovery::{RecoveryEndpoint, RecoveryFacts},
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
/// No Debug/Serialize: the selected message's text remains transient.
pub struct RecoveryRegion {
    index: usize,
    size: usize,
    text: String,
    label: String,
}
impl RecoveryRegion {
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn decode(raw: &str, kind: PageKind) -> Result<Option<Self>, BrowserError> {
        if !matches!(kind, PageKind::Expired | PageKind::ServerError) {
            return Err(BrowserError::Unsupported);
        }
        if raw.len() > 128 * 1024 {
            return Err(BrowserError::NoObservation);
        }
        let value: Value = serde_json::from_str(raw).map_err(|_| BrowserError::NoObservation)?;
        let rows = value
            .as_array()
            .filter(|r| r.len() <= 256)
            .ok_or(BrowserError::NoObservation)?;
        let mut best: Option<Self> = None;
        let mut seen = std::collections::HashSet::new();
        for row in rows {
            let map = row
                .as_object()
                .filter(|m| m.len() == 4)
                .ok_or(BrowserError::NoObservation)?;
            let index = map
                .get("index")
                .and_then(Value::as_u64)
                .filter(|v| *v < 256)
                .ok_or(BrowserError::NoObservation)? as usize;
            let size = map
                .get("size")
                .and_then(Value::as_u64)
                .filter(|v| *v <= 4096)
                .ok_or(BrowserError::NoObservation)? as usize;
            if !seen.insert(index) {
                return Err(BrowserError::NoObservation);
            }
            let text = map
                .get("text")
                .and_then(Value::as_str)
                .filter(|s| s.len() <= 65536)
                .ok_or(BrowserError::NoObservation)?;
            let label = map
                .get("label")
                .and_then(Value::as_str)
                .filter(|s| s.len() <= 32)
                .ok_or(BrowserError::NoObservation)?;
            if ![
                "aceptar",
                "volver",
                "salir",
                "reiniciar",
                "reiniciar sesión",
                "reiniciar sesion",
                "iniciar",
                "restart",
                "return",
            ]
            .contains(&label.to_lowercase().as_str())
            {
                return Err(BrowserError::NoObservation);
            }
            if recovery_message_kind(text) == Some(kind)
                && best
                    .as_ref()
                    .is_none_or(|old| (size, index) < (old.size, old.index))
            {
                best = Some(Self {
                    index,
                    size,
                    text: text.into(),
                    label: label.into(),
                });
            }
        }
        Ok(best)
    }
    /// Browser-only exact region text, never telemetry. Retained for later guarded activation.
    pub fn matches_text(&self, text: &str) -> bool {
        self.text == text
    }
}
pub struct RecoveryRead {
    pub facts: RecoveryFacts,
    pub source: FrameSource,
    region: Option<RecoveryRegion>,
}
impl RecoveryRead {
    pub fn label(&self) -> Option<&str> {
        self.region.as_ref().map(RecoveryRegion::label)
    }
}
const INFO: &str = r#"()=>{
 const official=location.origin==='https://icp.administracionelectronica.gob.es';
 const provider=location.protocol==='https:'&&(!location.port||location.port==='443')&&['pasarela.clave.gob.es','pasarela-ident.clave.gob.es'].includes(location.hostname);
 const path=location.pathname.split('/').pop().split(';')[0];
 const endpoint=provider?'clave_portal':!official?'other':({'acCitar':'availability','acInfo':'auth_info','procesarRespuestaClave':'auth_return','acEntrada':'applicant_entry','index':'index','index.html':'index','':'index'}[path]||'other');
 return JSON.stringify({official,endpoint});
}"#;
fn string_result(result: Value) -> Result<String, BrowserError> {
    if result["type"] != "success" || result["result"]["type"] != "string" {
        return Err(BrowserError::NoObservation);
    }
    result["result"]["value"]
        .as_str()
        .filter(|s| s.len() <= 128 * 1024)
        .map(str::to_owned)
        .ok_or(BrowserError::NoObservation)
}
pub fn read(
    connection: &mut BidiConnection,
    page: &ObservedPage,
    last_action: Option<ActionKind>,
    pending_action: Option<ActionKind>,
    expiry_already_attempted: bool,
) -> Result<RecoveryRead, BrowserError> {
    if !matches!(
        page.observation.kind,
        PageKind::Expired | PageKind::ServerError | PageKind::Blocked | PageKind::RateLimited
    ) {
        return Err(BrowserError::Unsupported);
    }
    let deadline = Instant::now() + Duration::from_secs(1);
    let info=string_result(connection.call_before("script.callFunction",json!({"target":{"realm":page.observation.document.as_str()},"functionDeclaration":INFO,"awaitPromise":false}),deadline).map_err(|_|BrowserError::NoObservation)?)?;
    let info: Value = serde_json::from_str(&info).map_err(|_| BrowserError::NoObservation)?;
    let info = info
        .as_object()
        .filter(|m| m.len() == 2)
        .ok_or(BrowserError::NoObservation)?;
    let official_site = info
        .get("official")
        .and_then(Value::as_bool)
        .ok_or(BrowserError::NoObservation)?;
    let endpoint = match info.get("endpoint").and_then(Value::as_str) {
        Some("availability") => RecoveryEndpoint::Availability,
        Some("auth_info") => RecoveryEndpoint::AuthInfo,
        Some("auth_return") => RecoveryEndpoint::AuthReturn,
        Some("applicant_entry") => RecoveryEndpoint::ApplicantEntry,
        Some("index") => RecoveryEndpoint::Index,
        Some("clave_portal") => RecoveryEndpoint::ClavePortal,
        Some("other") => RecoveryEndpoint::Other,
        _ => return Err(BrowserError::NoObservation),
    };
    let source = page
        .page
        .sources
        .get(page.recognition.source_frame)
        .ok_or(BrowserError::NoObservation)?;
    let region = if official_site
        && matches!(
            page.observation.kind,
            PageKind::Expired | PageKind::ServerError
        ) {
        let raw=string_result(connection.call_before("script.callFunction",json!({"target":{"realm":source.realm.as_str()},"functionDeclaration":include_str!("recovery-regions.js"),"awaitPromise":false}),deadline).map_err(|_|BrowserError::NoObservation)?)?;
        RecoveryRegion::decode(&raw, page.observation.kind)?
    } else {
        None
    };
    let facts = RecoveryFacts {
        official_site,
        endpoint,
        last_action,
        pending_action,
        own_control: region.is_some(),
        certificate_validation_error: page.recognition.certificate_validation_error,
        immediate_expiry_control: region.is_some()
            && page.recognition.recovery_action_first
            && !expiry_already_attempted,
    };
    Ok(RecoveryRead {
        facts,
        source: FrameSource {
            context: source.context.clone(),
            realm: source.realm.clone(),
        },
        region,
    })
}

/// Activate only the exact recovery message/source that the core scheduled.
/// Root authority and the error frame's realm/DOM are both rechecked; no fallback.
pub fn activate_own_control(
    connection: &mut BidiConnection,
    observer: &mut crate::observer::PageObserver,
    gate: &crate::DispatchGate,
    guard: &ceh_core::model::DocumentGuard,
    issued: &RecoveryRead,
) -> Result<crate::DispatchOutcome, BrowserError> {
    let region = issued.region.as_ref().ok_or(BrowserError::Unsupported)?;
    let fresh = observer
        .observe(connection, "manual")
        .map_err(|_| BrowserError::NoObservation)?;
    if !fresh.page.snapshot.official
        || !matches!(
            fresh.observation.kind,
            PageKind::Expired | PageKind::ServerError
        )
    {
        return Err(BrowserError::Unsupported);
    }
    let actual = fresh
        .page
        .sources
        .get(fresh.recognition.source_frame)
        .ok_or(BrowserError::NoObservation)?;
    if actual.context != issued.source.context || actual.realm != issued.source.realm {
        return Err(BrowserError::StaleDocument);
    }
    let source = FrameSource {
        context: issued.source.context.clone(),
        realm: issued.source.realm.clone(),
    };
    crate::trusted_input::click(
        connection,
        observer,
        gate,
        guard,
        fresh,
        crate::trusted_input::Target::recovery(
            json!({"region":region.index,"regionText":region.text}),
            source,
        ),
    )
}
