//! Contact fields require trusted keyboard input, never paste or synthetic text insertion.
use crate::{
    BrowserError, DispatchGate, DispatchOutcome,
    bidi::BidiConnection,
    observer::{ObservedPage, PageObserver},
    trusted_input::{self, Target},
    typed_field,
};
use ceh_core::{
    flow::PreparationOutcome,
    model::{ActionKind, DocumentGuard, PageKind},
    profiles::ProfileDraft,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
const SCRIPT: &str = include_str!("contact-step.js");
/// Intentionally no Debug/Serialize: only this browser receives contact data.
pub struct ContactInput {
    phone: String,
    email: String,
}
impl ContactInput {
    pub fn from_profile(profile: &ProfileDraft) -> Result<Self, BrowserError> {
        let bounded = |s: &str, max: usize| {
            !s.trim().is_empty() && s.len() <= max && !s.chars().any(char::is_control)
        };
        if !bounded(&profile.phone, 32) || !bounded(&profile.email, 254) {
            return Err(BrowserError::Unsupported);
        }
        Ok(Self {
            phone: profile.phone.clone(),
            email: profile.email.clone(),
        })
    }
    fn payload(&self, operation: &str, field: &str, prefix: &str) -> Value {
        json!({"operation":operation,"field":field,"prefix":prefix,"phone":self.phone,"email":self.email})
    }
}
fn fresh_page(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
) -> Result<ObservedPage, BrowserError> {
    let fresh = observer
        .observe(connection, "manual")
        .map_err(|_| BrowserError::NoObservation)?;
    if !fresh.page.snapshot.official {
        return Err(BrowserError::Unsupported);
    }
    if fresh.observation.kind != PageKind::Contact {
        return Err(BrowserError::StaleDocument);
    }
    Ok(fresh)
}
pub fn prepare_contact(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    input: &ContactInput,
    typing_delay: Duration,
    deadline: Instant,
) -> Result<PreparationOutcome, BrowserError> {
    if typing_delay > Duration::from_millis(250) {
        return Err(BrowserError::Unsupported);
    }
    for (field, value) in [
        ("phone", input.phone.as_str()),
        ("email1", input.email.as_str()),
        ("email2", input.email.as_str()),
    ] {
        let payload = |operation: &str, prefix: &str| input.payload(operation, field, prefix);
        let spec = typed_field::Field {
            action: ActionKind::Contact,
            script: SCRIPT,
            value,
            payload: &payload,
        };
        let outcome = typed_field::enter(
            connection,
            observer,
            gate,
            guard,
            &spec,
            &|connection, observer| fresh_page(connection, observer),
            typing_delay,
            deadline,
        )?;
        if outcome != PreparationOutcome::Completed {
            return Ok(outcome);
        }
    }
    Ok(PreparationOutcome::Completed)
}
pub fn activate_contact(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    input: &ContactInput,
) -> Result<DispatchOutcome, BrowserError> {
    let fresh = fresh_page(connection, observer)?;
    trusted_input::click(
        connection,
        observer,
        gate,
        guard,
        fresh,
        Target::icp(ActionKind::Contact, input.payload("submit", "", ""), SCRIPT),
    )
}
