//! Known first-party banner pre-action; never a manual-page action.
use crate::{
    BrowserError, DispatchGate, DispatchOutcome,
    bidi::BidiConnection,
    observer::{ObservedPage, PageObserver},
    trusted_input::{self, Target},
};
use ceh_core::model::{ActionKind, DocumentGuard, PageKind};
use serde_json::json;
fn allowed(page: &ObservedPage) -> bool {
    page.page.snapshot.official
        && matches!(
            page.observation.kind,
            PageKind::Province
                | PageKind::Procedure
                | PageKind::Auth
                | PageKind::Applicant
                | PageKind::ApplicantNoSlots
                | PageKind::Contact
                | PageKind::RequestMenu
                | PageKind::RequestNoSlots
                | PageKind::RequestNoSlotsRestart
                | PageKind::PostContactNoSlots
        )
}
/// Read-only fact; no cookie values, private data, focus or scroll are accessed.
pub fn control_available(
    connection: &mut BidiConnection,
    page: &ObservedPage,
) -> Result<bool, BrowserError> {
    if !allowed(page) {
        return Ok(false);
    }
    let operation = include_str!("cookie-target.js");
    let declaration = format!(
        "()=>{{if(location.origin!=='https://icp.administracionelectronica.gob.es')return false;const find={operation};return find() instanceof Element;}}"
    );
    let response=connection.call("script.callFunction",json!({"target":{"realm":page.observation.document.as_str()},"functionDeclaration":declaration,"awaitPromise":false})).map_err(|_|BrowserError::NoObservation)?;
    if response["type"] != "success" || response["result"]["type"] != "boolean" {
        return Err(BrowserError::NoObservation);
    }
    response["result"]["value"]
        .as_bool()
        .ok_or(BrowserError::NoObservation)
}
pub fn accept(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
) -> Result<DispatchOutcome, BrowserError> {
    let fresh = observer
        .observe(connection, "manual")
        .map_err(|_| BrowserError::NoObservation)?;
    if !allowed(&fresh) {
        return Err(BrowserError::Unsupported);
    }
    trusted_input::click(
        connection,
        observer,
        gate,
        guard,
        fresh,
        Target::icp(
            ActionKind::Cookie,
            json!({}),
            include_str!("cookie-target.js"),
        ),
    )
}
