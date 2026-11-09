//! Internal typed-action primitive. Never accepts UI/page-provided selectors or scripts.
use crate::{
    BrowserError, DispatchGate, DispatchOutcome,
    bidi::{BidiConnection, ProtocolError},
    dom::{FrameSource, INSPECT_DOCUMENT, read_document},
    observer::{ObservedPage, PageObserver},
};
use ceh_core::model::{ActionKind, DocumentGuard};
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};
const GUARD: &str = include_str!("mutation-guard.js");
/// Resolve an element in the guarded realm, reobserve, then publish one trusted
/// pointer sequence bound to that element reference. No coordinate or JS fallback.
/// Acknowledgement is not evidence of a page transition; the core observes next.
#[derive(Clone, Copy)]
enum TargetOrigin {
    Icp,
    Provider,
}
impl TargetOrigin {
    fn predicate(self) -> &'static str {
        match self {
            Self::Icp => "origin=>origin==='https://icp.administracionelectronica.gob.es'",
            Self::Provider => {
                "origin=>origin==='https://pasarela.clave.gob.es'||origin==='https://pasarela-ident.clave.gob.es'"
            }
        }
    }
}
pub(crate) struct Target<'a> {
    action: ActionKind,
    payload: Value,
    script: &'static str,
    origin: TargetOrigin,
    source: Option<FrameSource>,
    published: Option<&'a mut (dyn FnMut() + Send)>,
}
impl Target<'static> {
    pub fn recovery(payload: Value, source: FrameSource) -> Self {
        Self {
            action: ActionKind::Recovery,
            payload,
            script: include_str!("recovery-step.js"),
            origin: TargetOrigin::Icp,
            source: Some(source),
            published: None,
        }
    }
    pub fn icp(action: ActionKind, payload: Value, script: &'static str) -> Self {
        Self {
            action,
            payload,
            script,
            origin: TargetOrigin::Icp,
            source: None,
            published: None,
        }
    }
}
impl<'a> Target<'a> {
    pub fn provider(
        payload: Value,
        script: &'static str,
        published: Option<&'a mut (dyn FnMut() + Send)>,
    ) -> Self {
        Self {
            action: ActionKind::Auth,
            payload,
            script,
            origin: TargetOrigin::Provider,
            source: None,
            published,
        }
    }
}
pub(crate) fn click(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    fresh: ObservedPage,
    target: Target<'_>,
) -> Result<DispatchOutcome, BrowserError> {
    activate(connection, observer, gate, guard, fresh, target, true)
}
pub(crate) fn focus(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    fresh: ObservedPage,
    target: Target<'_>,
) -> Result<DispatchOutcome, BrowserError> {
    activate(connection, observer, gate, guard, fresh, target, false)
}
fn activate(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    fresh: ObservedPage,
    target: Target<'_>,
    submission: bool,
) -> Result<DispatchOutcome, BrowserError> {
    let Target {
        action,
        mut payload,
        script: target_script,
        origin,
        source,
        mut published,
    } = target;
    let mut permit = gate.begin_preparation(guard, &fresh.observation, action)?;
    let fill_mode = if payload["fillMode"] == "copy" {
        "copy"
    } else {
        "manual"
    };
    let scoped_document = if let Some(source) = &source {
        if !fresh
            .page
            .sources
            .iter()
            .any(|s| s.context == source.context && s.realm == source.realm)
        {
            return Err(BrowserError::StaleDocument);
        }
        let document = read_document(connection, &source.context, fill_mode)
            .map_err(|_| BrowserError::NoObservation)?;
        if document.realm != source.realm {
            return Err(BrowserError::StaleDocument);
        }
        Some(document)
    } else {
        None
    };
    let snapshot = scoped_document
        .as_ref()
        .map(|d| &d.snapshot)
        .unwrap_or(&fresh.page.snapshot);
    let frame = snapshot.frames.first().ok_or(BrowserError::NoObservation)?;
    let context = source
        .as_ref()
        .or_else(|| fresh.page.sources.first())
        .ok_or(BrowserError::NoObservation)?
        .context
        .clone();
    let realm = source
        .as_ref()
        .map(|s| &s.realm)
        .unwrap_or(&fresh.observation.document);
    let allowed = origin.predicate();
    let declaration = format!(
        "raw=>{{const inspect={INSPECT_DOCUMENT};const validate={GUARD};const operation={target_script};const allowed={allowed};const target=validate(raw,inspect,operation,allowed);if(target instanceof Element)target.scrollIntoView({{block:'center',inline:'nearest',behavior:'instant'}});return target;}}"
    );
    payload["expectedText"] = json!(frame.text);
    payload["expectedVisible"] = json!(snapshot.visible);
    payload["expectedEndpoint"] = json!(snapshot.endpoint);
    let mut ticket = None;
    let sent=permit.send(&fresh.observation,||{
  let Some(now)=SystemTime::now().duration_since(UNIX_EPOCH).ok().and_then(|d|u64::try_from(d.as_millis()).ok()) else{return DispatchOutcome::NotDispatched;};
  payload["issuedAt"]=json!(now);
  let params=json!({"target":{"realm":realm.as_str()},"awaitPromise":false,"functionDeclaration":declaration,"arguments":[{"type":"string","value":payload.to_string()}]});
  send(connection,"script.callFunction",params,&mut ticket)
 })?;
    let Some(ticket) = ticket else {
        if sent == DispatchOutcome::Unknown {
            permit.complete_step(sent)?;
        }
        return Ok(permit.finish());
    };
    let result = match connection.receive(ticket) {
        Ok(result) => result,
        Err(_) => {
            permit.complete_step(DispatchOutcome::Unknown)?;
            return Ok(permit.finish());
        }
    };
    if result["type"] != "success" {
        permit.complete_step(DispatchOutcome::Unknown)?;
        return Ok(permit.finish());
    }
    if result["result"]["type"] != "node" {
        permit.complete_step(DispatchOutcome::NotDispatched)?;
        return Ok(permit.finish());
    }
    // Resolving an actionable node also scrolls it into view in this automatic
    // phase. Preserve that acknowledged change even if the later click is refused.
    permit.complete_step(DispatchOutcome::Dispatched)?;
    let Some(shared) = result["result"]["sharedId"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 1024)
    else {
        return Ok(DispatchOutcome::NotDispatched);
    };
    let shared = shared.to_owned();
    permit.finish();
    let current = observer
        .observe(connection, fill_mode)
        .map_err(|_| BrowserError::NoObservation)?;
    if current.page.snapshot.endpoint != fresh.page.snapshot.endpoint
        || current.page.snapshot.visible != fresh.page.snapshot.visible
        || current.page.snapshot.frames.first().map(|f| &f.text)
            != fresh.page.snapshot.frames.first().map(|f| &f.text)
    {
        return Err(BrowserError::StaleDocument);
    }
    if let Some(source) = &source {
        if current.observation.document != guard.document
            || current.observation.kind != guard.expected_page
            || !current
                .page
                .sources
                .iter()
                .any(|s| s.context == source.context && s.realm == source.realm)
        {
            return Err(BrowserError::StaleDocument);
        }
        let document = read_document(connection, &source.context, fill_mode)
            .map_err(|_| BrowserError::NoObservation)?;
        if document.realm != source.realm
            || document.snapshot.endpoint != snapshot.endpoint
            || document.snapshot.visible != snapshot.visible
            || document.snapshot.frames.first().map(|f| &f.text)
                != snapshot.frames.first().map(|f| &f.text)
        {
            return Err(BrowserError::StaleDocument);
        }
    }
    let mut permit = if submission {
        gate.begin(guard, &current.observation, action)?
    } else {
        gate.begin_preparation(guard, &current.observation, action)?
    };
    let params = json!({"context":context,"actions":[{"type":"pointer","id":"ceh-pointer","parameters":{"pointerType":"mouse"},"actions":[{"type":"pointerMove","duration":ceh_core::input::POINTER_TIMING.move_ms,"x":0,"y":0,"origin":{"type":"element","element":{"sharedId":shared}}},{"type":"pointerDown","button":0},{"type":"pause","duration":ceh_core::input::POINTER_TIMING.press_ms},{"type":"pointerUp","button":0}]}]});
    let mut ticket = None;
    let sent = permit.send(&current.observation, || {
        send(connection, "input.performActions", params, &mut ticket)
    })?;
    // Notify outside the authority lock and before waiting for a possible native
    // certificate dialog. Hook is trusted, bounded controller bookkeeping only.
    if sent == DispatchOutcome::Unknown
        && let Some(hook) = published.as_mut()
    {
        hook();
    }
    let Some(ticket) = ticket else {
        if sent == DispatchOutcome::Unknown {
            permit.complete_step(sent)?;
        }
        return Ok(permit.finish());
    };
    let outcome = match connection.receive(ticket) {
        Ok(_) => DispatchOutcome::Dispatched,
        Err(_) => DispatchOutcome::Unknown,
    };
    permit.complete_step(outcome)?;
    Ok(permit.finish())
}
fn send(
    connection: &mut BidiConnection,
    method: &str,
    params: Value,
    ticket: &mut Option<crate::bidi::Ticket>,
) -> DispatchOutcome {
    match connection.send(method, params) {
        Ok(value) => {
            *ticket = Some(value);
            DispatchOutcome::Unknown
        }
        Err(ProtocolError::InvalidData | ProtocolError::PendingCommand) => {
            DispatchOutcome::NotDispatched
        }
        Err(_) => DispatchOutcome::Unknown,
    }
}

pub(crate) fn key(
    connection: &mut BidiConnection,
    permit: &mut crate::ActionPermit,
    fresh: &ObservedPage,
    value: char,
) -> Result<DispatchOutcome, BrowserError> {
    let context = &fresh
        .page
        .sources
        .first()
        .ok_or(BrowserError::NoObservation)?
        .context;
    let params = json!({"context":context,"actions":[{"type":"key","id":"ceh-keyboard","actions":[{"type":"keyDown","value":value.to_string()},{"type":"keyUp","value":value.to_string()}]}]});
    let mut ticket = None;
    let sent = permit.send(&fresh.observation, || {
        send(connection, "input.performActions", params, &mut ticket)
    })?;
    let Some(ticket) = ticket else {
        if sent == DispatchOutcome::Unknown {
            permit.complete_step(sent)?;
        }
        return Ok(sent);
    };
    let outcome = match connection.receive(ticket) {
        Ok(_) => DispatchOutcome::Dispatched,
        Err(_) => DispatchOutcome::Unknown,
    };
    permit.complete_step(outcome)?;
    Ok(outcome)
}
