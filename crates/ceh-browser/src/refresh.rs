//! Refresh only the known no-slots states, using actual loaded-navigation facts.
use crate::{
    BrowserError, DispatchGate, DispatchOutcome, bidi::BidiConnection, mutation,
    observer::PageObserver,
};
use ceh_core::{
    model::{ActionKind, DocumentGuard, Observation, PageKind, RequestMethod},
    recognition::PageSnapshot,
};
use serde_json::json;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshAction {
    ReloadApplicant,
    FetchApplicant,
    ReloadAvailability,
}
/// This is a transport safety check, not the core's timing/profile policy.
pub fn plan_refresh(
    observed: &Observation,
    snapshot: &PageSnapshot,
) -> Result<RefreshAction, BrowserError> {
    if !snapshot.official {
        return Err(BrowserError::Unsupported);
    }
    let previous = snapshot
        .pending_action
        .as_deref()
        .or(snapshot.last_action.as_deref());
    match (observed.kind, observed.method) {
        (PageKind::ApplicantNoSlots, RequestMethod::Get) => Ok(RefreshAction::ReloadApplicant),
        (PageKind::ApplicantNoSlots, RequestMethod::Post)
            if snapshot.endpoint == "acEntrada"
                && !matches!(
                    previous,
                    Some("applicant" | "contact" | "request" | "office")
                ) =>
        {
            Ok(RefreshAction::FetchApplicant)
        }
        (PageKind::RequestNoSlots, RequestMethod::Get) => Ok(RefreshAction::ReloadAvailability),
        (PageKind::RequestNoSlots, RequestMethod::Post)
            if snapshot.endpoint == "acCitar" && previous == Some("request") =>
        {
            Ok(RefreshAction::ReloadAvailability)
        }
        _ => Err(BrowserError::Unsupported),
    }
}
pub fn execute_refresh(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    requested: RefreshAction,
) -> Result<DispatchOutcome, BrowserError> {
    let fresh = observer
        .observe(connection, "manual")
        .map_err(|_| BrowserError::NoObservation)?;
    if plan_refresh(&fresh.observation, &fresh.page.snapshot)? != requested {
        return Err(BrowserError::Unsupported);
    }
    // Bare JS/standard BiDi POST reload did not issue another request in native
    // Camoufox. Keep this opt-in branch blocked until a verified transport exists.
    if requested == RefreshAction::ReloadAvailability
        && fresh.observation.method == RequestMethod::Post
    {
        return Err(BrowserError::Unsupported);
    }
    let action = if requested == RefreshAction::ReloadAvailability {
        ActionKind::RequestReload
    } else {
        ActionKind::ApplicantReload
    };
    let mut permit = gate.begin(guard, &fresh.observation, action)?;
    mutation::perform(
        connection,
        &mut permit,
        &fresh,
        json!({"fetchApplicant":requested==RefreshAction::FetchApplicant}),
        include_str!("refresh-step.js"),
    )?;
    Ok(permit.finish())
}

/// Shared startup capability: never replay a POST or refresh a manual/form page.
pub fn entry_reload_allowed(observed:&Observation,snapshot:&PageSnapshot)->bool{
 snapshot.official&&observed.kind==PageKind::Province&&observed.method==RequestMethod::Get
}
pub fn reload_entry(connection:&mut BidiConnection,observer:&mut PageObserver,gate:&DispatchGate,guard:&DocumentGuard)->Result<DispatchOutcome,BrowserError>{
 let fresh=observer.observe(connection,"manual").map_err(|_|BrowserError::NoObservation)?;
 if !entry_reload_allowed(&fresh.observation,&fresh.page.snapshot){return Err(BrowserError::StaleDocument);}
 let mut permit=gate.begin(guard,&fresh.observation,ActionKind::Recovery)?;
 mutation::perform(connection,&mut permit,&fresh,json!({}),"() => { let sent=false;try{sent=true;location.reload();return 'changed';}catch(_){return sent?'unknown':'refused';}}")?;
 Ok(permit.finish())
}


/// Navigate to the fixed entry URL without replaying the current request.
pub fn start_fresh_pass(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
) -> Result<DispatchOutcome, BrowserError> {
    let fresh = observer.observe(connection, "manual").map_err(|_| BrowserError::NoObservation)?;
    if !fresh.page.snapshot.official || matches!(fresh.observation.kind, PageKind::Booking | PageKind::Captcha) {
        return Err(BrowserError::Unsupported);
    }
    let mut permit = gate.begin(guard, &fresh.observation, ActionKind::Recovery)?;
    mutation::perform(connection, &mut permit, &fresh, json!({}),
        "() => { location.assign('https://icp.administracionelectronica.gob.es/icpplus/index.html'); return 'changed'; }")?;
    Ok(permit.finish())
}
