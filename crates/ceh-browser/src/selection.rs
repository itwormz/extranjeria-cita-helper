//! Province/procedure/sole-office steps aligned with core's separate Prepare and Activate effects.
use crate::{
    BrowserError, DispatchGate, DispatchOutcome,
    bidi::BidiConnection,
    observer::{ObservedPage, PageObserver},
    typed_select,
};
use ceh_core::{
    flow::PreparationOutcome,
    model::{ActionKind, DocumentGuard, PageKind},
};
use serde_json::json;
use std::time::{Duration, Instant};
const HUELLAS: &str =
    "POLICÍA-TOMA DE HUELLAS (EXPEDICIÓN DE TARJETA) INICIAL, RENOVACIÓN, DUPLICADO Y LEY 14/2013";
const SCRIPT: &str = include_str!("selection-step.js");
/// Data only, never executable selectors/scripts. No Debug: later profile steps are private.
pub struct Selection {
    action: ActionKind,
    labels: Vec<String>,
}
impl Selection {
    pub fn province(label: &str) -> Result<Self, BrowserError> {
        if label.is_empty() || label.len() > 256 || label.chars().any(char::is_control) {
            return Err(BrowserError::Unsupported);
        }
        Ok(Self {
            action: ActionKind::Province,
            labels: vec![label.to_owned()],
        })
    }
    pub fn procedure(service: &str) -> Result<Self, BrowserError> {
        let labels = match service {
            "huellas" => vec![HUELLAS.to_owned()],
            "regreso" => vec![
                "AUTORIZACIÓN DE REGRESO".to_owned(),
                "POLICIA-AUTORIZACIÓN DE REGRESO".to_owned(),
            ],
            _ => return Err(BrowserError::Unsupported),
        };
        Ok(Self {
            action: ActionKind::Procedure,
            labels,
        })
    }
    pub fn single_office() -> Self {
        Self {
            action: ActionKind::Office,
            labels: Vec::new(),
        }
    }
    pub fn action(&self) -> ActionKind {
        self.action
    }
    fn payload(&self, operation: &str) -> serde_json::Value {
        json!({"operation":operation,"province":self.action==ActionKind::Province,"office":self.action==ActionKind::Office,"labels":self.labels})
    }
    fn page_kind(&self) -> PageKind {
        match self.action {
            ActionKind::Province => PageKind::Province,
            ActionKind::Office => PageKind::Office,
            _ => PageKind::Procedure,
        }
    }
}
fn fresh_page(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    selection: &Selection,
) -> Result<ObservedPage, BrowserError> {
    let fresh = observer
        .observe(connection, "manual")
        .map_err(|_| BrowserError::NoObservation)?;
    if !fresh.page.snapshot.official {
        return Err(BrowserError::Unsupported);
    }
    if fresh.observation.kind != selection.page_kind() {
        return Err(BrowserError::StaleDocument);
    }
    Ok(fresh)
}
/// Choose the option with the browser's own keyboard input. Waits only for proven non-mutating
/// readiness failures, within the caller's profile timing budget; unknown delivery is never retried.
pub fn prepare_selection(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    selection: &Selection,
    deadline: Instant,
) -> Result<PreparationOutcome, BrowserError> {
    let payload = |operation: &str| selection.payload(operation);
    let choice = typed_select::Choice { action: selection.action, script: SCRIPT, payload: &payload };
    while Instant::now() < deadline {
        match typed_select::choose(connection, observer, gate, guard, &choice, &|connection, observer| {
            fresh_page(connection, observer, selection)
        })? {
            typed_select::Chosen::Done => return Ok(PreparationOutcome::Completed),
            typed_select::Chosen::Unknown => return Ok(PreparationOutcome::Unknown),
            typed_select::Chosen::Mismatch => return Ok(PreparationOutcome::Failed),
            typed_select::Chosen::NotReady => {
                std::thread::sleep(Duration::from_millis(100).min(deadline.saturating_duration_since(Instant::now())));
            }
        }
    }
    Ok(PreparationOutcome::Failed)
}

pub fn activate_selection(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    selection: &Selection,
) -> Result<DispatchOutcome, BrowserError> {
    for attempt in 0..3 {
        let fresh = fresh_page(connection, observer, selection)?;
        if fresh.observation.document != guard.document {
            return Err(BrowserError::StaleDocument);
        }
        let payload = selection.payload("submit");
        match crate::trusted_input::click(
            connection,
            observer,
            gate,
            guard,
            fresh,
            crate::trusted_input::Target::icp(selection.action, payload, SCRIPT),
        ) {
            Ok(DispatchOutcome::NotDispatched) | Err(BrowserError::StaleDocument)
                if attempt < 2 =>
            {
                std::thread::sleep(Duration::from_millis(150));
            }
            result => return result,
        }
    }
    Ok(DispatchOutcome::NotDispatched)
}
