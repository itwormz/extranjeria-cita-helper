//! Navigation recovery of Python's `assist_flow.recover`: one GET of the start page or of the current
//! official page, or a reload, issued by the browser itself (`page.goto` / `page.reload`), never by page
//! script. A POST is never replayed by a GET: the core asks for `GetCurrent` only after a GET response.
use crate::{BrowserError, DispatchGate, DispatchOutcome, bidi::BidiConnection, observer::PageObserver};
use ceh_core::{
    model::{ActionKind, DocumentGuard, Observation, PageKind, RequestMethod},
    recognition::PageSnapshot,
    recovery::RecoveryAction,
};
use serde_json::json;

pub const ENTRY_URL: &str = "https://icp.administracionelectronica.gob.es/icpplus/index.html";
const OFFICIAL_HOST: &str = "icp.administracionelectronica.gob.es";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigationRecovery {
    Reload,
    Get(String),
}

fn official_url(url: &str) -> bool {
    url.strip_prefix("https://")
        .and_then(|rest| rest.split(['/', '?', '#']).next())
        .is_some_and(|authority| authority == OFFICIAL_HOST)
}
/// What the browser may do for `action` on the page just read; everything else belongs to the core
/// (`Hold`, `ActivateOwnControl`) or to the lifecycle owner (`Restart`).
pub fn plan_navigation(
    action: RecoveryAction,
    observed: &Observation,
    snapshot: &PageSnapshot,
    current_url: Option<&str>,
) -> Result<NavigationRecovery, BrowserError> {
    match action {
        RecoveryAction::GetEntry if !matches!(observed.kind, PageKind::Booking | PageKind::Captcha) => {
            Ok(NavigationRecovery::Get(ENTRY_URL.to_owned()))
        }
        RecoveryAction::GetCurrent if snapshot.official && observed.method == RequestMethod::Get => current_url
            .filter(|url| official_url(url))
            .map(|url| NavigationRecovery::Get(url.to_owned()))
            .ok_or(BrowserError::Unsupported),
        RecoveryAction::Reload if snapshot.official => {
            let previous = snapshot.pending_action.as_deref().or(snapshot.last_action.as_deref());
            let repeatable_search = observed.method == RequestMethod::Post
                && snapshot.endpoint == "acCitar"
                && previous == Some("request");
            if observed.method == RequestMethod::Get || repeatable_search {
                Ok(NavigationRecovery::Reload)
            } else {
                Err(BrowserError::Unsupported)
            }
        }
        _ => Err(BrowserError::Unsupported),
    }
}

fn context_url(tree: &serde_json::Value, context: &str) -> Option<String> {
    fn walk(nodes: &serde_json::Value, wanted: &str) -> Option<String> {
        nodes.as_array()?.iter().find_map(|node| {
            if node["context"] == wanted {
                node["url"].as_str().map(str::to_owned)
            } else {
                walk(&node["children"], wanted)
            }
        })
    }
    walk(&tree["contexts"], context)
}

pub fn execute_navigation(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    action: RecoveryAction,
) -> Result<DispatchOutcome, BrowserError> {
    let fresh = observer.observe(connection, "manual").map_err(|_| BrowserError::NoObservation)?;
    let context = fresh.page.sources.first().ok_or(BrowserError::NoObservation)?.context.clone();
    let url = if action == RecoveryAction::GetCurrent {
        let tree = connection.call("browsingContext.getTree", json!({})).map_err(|_| BrowserError::NoObservation)?;
        context_url(&tree, &context)
    } else {
        None
    };
    let plan = plan_navigation(action, &fresh.observation, &fresh.page.snapshot, url.as_deref())?;
    let (method, params) = match plan {
        NavigationRecovery::Reload => ("browsingContext.reload", json!({"context":context,"wait":"none"})),
        NavigationRecovery::Get(url) => ("browsingContext.navigate", json!({"context":context,"url":url,"wait":"none"})),
    };
    let mut permit = gate.begin(guard, &fresh.observation, ActionKind::Recovery)?;
    let mut ticket = None;
    let sent = permit.send(&fresh.observation, || match connection.send(method, params) {
        Ok(value) => {
            ticket = Some(value);
            DispatchOutcome::Unknown
        }
        Err(crate::bidi::ProtocolError::InvalidData | crate::bidi::ProtocolError::PendingCommand) => DispatchOutcome::NotDispatched,
        Err(_) => DispatchOutcome::Unknown,
    })?;
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
