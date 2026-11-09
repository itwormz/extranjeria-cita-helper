//! Contact fields require trusted keyboard input, never paste or synthetic text insertion.
use crate::{
    BrowserError, DispatchGate, DispatchOutcome,
    bidi::BidiConnection,
    mutation::{self, Receipt},
    observer::{ObservedPage, PageObserver},
    trusted_input::{self, Target},
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
        if Instant::now() >= deadline {
            return Ok(PreparationOutcome::Failed);
        }
        let mut fresh = fresh_page(connection, observer)?;
        let mut permit = gate.begin_preparation(guard, &fresh.observation, ActionKind::Contact)?;
        let verified = mutation::perform(
            connection,
            &mut permit,
            &fresh,
            input.payload("verify", field, ""),
            SCRIPT,
        )?;
        if verified == Receipt::Unknown {
            return Ok(PreparationOutcome::Unknown);
        }
        if verified == Receipt::Unchanged {
            continue;
        }
        drop(permit);
        let focused = trusted_input::focus(
            connection,
            observer,
            gate,
            guard,
            fresh,
            Target::icp(
                ActionKind::Contact,
                input.payload("target", field, ""),
                SCRIPT,
            ),
        )?;
        match focused {
            DispatchOutcome::Unknown => return Ok(PreparationOutcome::Unknown),
            DispatchOutcome::NotDispatched => return Ok(PreparationOutcome::Failed),
            DispatchOutcome::Dispatched => {}
        }
        fresh = fresh_page(connection, observer)?;
        let mut permit = gate.begin_preparation(guard, &fresh.observation, ActionKind::Contact)?;
        let cleared = mutation::perform(
            connection,
            &mut permit,
            &fresh,
            input.payload("clear", field, ""),
            SCRIPT,
        )?;
        if cleared == Receipt::Unknown {
            return Ok(PreparationOutcome::Unknown);
        }
        if !cleared.completed() {
            return Ok(PreparationOutcome::Failed);
        }
        let mut prefix = String::new();
        for character in value.chars() {
            if Instant::now() >= deadline {
                return Ok(PreparationOutcome::Failed);
            }
            let ready = mutation::perform(
                connection,
                &mut permit,
                &fresh,
                input.payload("ready_key", field, &prefix),
                SCRIPT,
            )?;
            if ready == Receipt::Unknown {
                return Ok(PreparationOutcome::Unknown);
            }
            if ready != Receipt::Unchanged {
                return Ok(PreparationOutcome::Failed);
            }
            match trusted_input::key(connection, &mut permit, &fresh, character)? {
                DispatchOutcome::Unknown => return Ok(PreparationOutcome::Unknown),
                DispatchOutcome::NotDispatched => return Ok(PreparationOutcome::Failed),
                DispatchOutcome::Dispatched => {}
            }
            prefix.push(character);
            if !typing_delay.is_zero() {
                std::thread::sleep(
                    typing_delay.min(deadline.saturating_duration_since(Instant::now())),
                );
            }
        }
        let verified = mutation::perform(
            connection,
            &mut permit,
            &fresh,
            input.payload("verify", field, ""),
            SCRIPT,
        )?;
        if verified == Receipt::Unknown {
            return Ok(PreparationOutcome::Unknown);
        }
        if verified != Receipt::Unchanged {
            return Ok(PreparationOutcome::Failed);
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
