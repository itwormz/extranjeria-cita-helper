//! Known no-slots exits, activated only after the core's cooldown effect.
use crate::{
    BrowserError, DispatchGate, DispatchOutcome,
    bidi::BidiConnection,
    observer::PageObserver,
    trusted_input::{self, Target},
};
use ceh_core::model::{ActionKind, DocumentGuard, PageKind};
use serde_json::json;
const SCRIPT: &str = include_str!("exit-step.js");
pub struct ExitAction {
    action: ActionKind,
    page: PageKind,
    mode: &'static str,
}
impl ExitAction {
    pub fn from_action(action: ActionKind) -> Result<Self, BrowserError> {
        let (page, mode) = match action {
            ActionKind::NoSlotsRestart => (PageKind::RequestNoSlotsRestart, "restart"),
            ActionKind::NoSlotsSalir => (PageKind::RequestNoSlots, "salir"),
            ActionKind::PostContactRestart => (PageKind::PostContactNoSlots, "post_contact"),
            _ => return Err(BrowserError::Unsupported),
        };
        Ok(Self { action, page, mode })
    }
}
pub fn activate_exit(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    exit: &ExitAction,
) -> Result<DispatchOutcome, BrowserError> {
    let fresh = observer
        .observe(connection, "manual")
        .map_err(|_| BrowserError::NoObservation)?;
    if !fresh.page.snapshot.official {
        return Err(BrowserError::Unsupported);
    }
    if fresh.observation.kind != exit.page {
        return Err(BrowserError::StaleDocument);
    }
    trusted_input::click(
        connection,
        observer,
        gate,
        guard,
        fresh,
        Target::icp(exit.action, json!({"mode":exit.mode}), SCRIPT),
    )
}

/// Read-only fact for core recovery/cooldown decisions. Never scrolls or activates.
pub fn control_available(
    connection: &mut BidiConnection,
    fresh: &crate::observer::ObservedPage,
    exit: &ExitAction,
) -> Result<bool, BrowserError> {
    if !fresh.page.snapshot.official || fresh.observation.kind != exit.page {
        return Ok(false);
    }
    let script = format!(
        "raw=>{{if(location.origin!=='https://icp.administracionelectronica.gob.es')return false;const operation={SCRIPT};return operation(JSON.parse(raw)) instanceof Element;}}"
    );
    let result=connection.call("script.callFunction",json!({"target":{"realm":fresh.observation.document.as_str()},"functionDeclaration":script,"awaitPromise":false,"arguments":[{"type":"string","value":json!({"mode":exit.mode}).to_string()}]})).map_err(|_|BrowserError::NoObservation)?;
    Ok(result["type"] == "success"
        && result["result"]["type"] == "boolean"
        && result["result"]["value"] == true)
}
