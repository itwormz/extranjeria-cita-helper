//! Shared transport receipt handling for typed form operations; no page-provided code.
use crate::{
    ActionPermit, BrowserError, DispatchOutcome,
    bidi::{BidiConnection, ProtocolError},
    dom::INSPECT_DOCUMENT,
    observer::ObservedPage,
};
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};
const GUARD: &str = include_str!("mutation-guard.js");
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Receipt {
    Changed,
    Unchanged,
    Refused,
    Unknown,
}
impl Receipt {
    pub(crate) fn outcome(self) -> DispatchOutcome {
        match self {
            Self::Changed => DispatchOutcome::Dispatched,
            Self::Unchanged | Self::Refused => DispatchOutcome::NotDispatched,
            Self::Unknown => DispatchOutcome::Unknown,
        }
    }
    pub(crate) fn completed(self) -> bool {
        matches!(self, Self::Changed | Self::Unchanged)
    }
}
pub(crate) fn perform(
    connection: &mut BidiConnection,
    permit: &mut ActionPermit,
    fresh: &ObservedPage,
    mut payload: Value,
    script: &str,
) -> Result<Receipt, BrowserError> {
    let frame = fresh
        .page
        .snapshot
        .frames
        .first()
        .ok_or(BrowserError::NoObservation)?;
    payload["expectedText"] = json!(frame.text);
    payload["expectedVisible"] = json!(fresh.page.snapshot.visible);
    payload["expectedEndpoint"] = json!(fresh.page.snapshot.endpoint);
    let declaration = format!(
        "raw => {{const inspect={INSPECT_DOCUMENT};const validate={GUARD};const step={script};return validate(raw,inspect,step);}}"
    );
    let mut ticket = None;
    let sent=permit.send(&fresh.observation,|| {
  let Some(now)=SystemTime::now().duration_since(UNIX_EPOCH).ok().and_then(|d|u64::try_from(d.as_millis()).ok()) else {return DispatchOutcome::NotDispatched;};
  payload["issuedAt"]=json!(now);
  let params=json!({"target":{"realm":fresh.observation.document.as_str()},"awaitPromise":false,"functionDeclaration":declaration,"arguments":[{"type":"string","value":payload.to_string()}]});
  match connection.send("script.callFunction",params) {Ok(value)=>{ticket=Some(value);DispatchOutcome::Unknown},Err(ProtocolError::InvalidData|ProtocolError::PendingCommand)=>DispatchOutcome::NotDispatched,Err(_)=>DispatchOutcome::Unknown}
 })?;
    let Some(ticket) = ticket else {
        if sent == DispatchOutcome::Unknown {
            permit.complete_step(DispatchOutcome::Unknown)?;
            return Ok(Receipt::Unknown);
        }
        return Ok(Receipt::Refused);
    };
    let result = connection
        .receive(ticket)
        .map(reply)
        .unwrap_or(Receipt::Unknown);
    permit.complete_step(result.outcome())?;
    Ok(result)
}
fn reply(value: Value) -> Receipt {
    if value["type"] != "success" || value["result"]["type"] != "string" {
        return Receipt::Unknown;
    }
    match value["result"]["value"].as_str() {
        Some("changed") => Receipt::Changed,
        Some("unchanged") => Receipt::Unchanged,
        Some("refused") => Receipt::Refused,
        _ => Receipt::Unknown,
    }
}
