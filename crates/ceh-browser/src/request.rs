//! Request menu submission is a single guarded core activation, not preparation.
use crate::{
    BrowserError, DispatchGate, DispatchOutcome, bidi::BidiConnection, observer::PageObserver,
};
use ceh_core::model::{ActionKind, DocumentGuard, PageKind};
const SCRIPT: &str = include_str!("request-step.js");
pub fn activate_request(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
) -> Result<DispatchOutcome, BrowserError> {
    let fresh = observer
        .observe(connection, "manual")
        .map_err(|_| BrowserError::NoObservation)?;
    if !fresh.page.snapshot.official {
        return Err(BrowserError::Unsupported);
    }
    if fresh.observation.kind != PageKind::RequestMenu {
        return Err(BrowserError::StaleDocument);
    }
    crate::trusted_input::click(
        connection,
        observer,
        gate,
        guard,
        fresh,
        crate::trusted_input::Target::icp(ActionKind::Request, serde_json::json!({}), SCRIPT),
    )
}
