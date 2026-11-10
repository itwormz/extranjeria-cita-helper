//! One text field entered the way a person does: focus by pointer, select-all and delete by keyboard,
//! then one real key press per character. Page scripts only read: they verify the field and resolve it.
use crate::{
    BrowserError, DispatchGate, DispatchOutcome,
    bidi::BidiConnection,
    mutation::{self, Receipt},
    observer::{ObservedPage, PageObserver},
    trusted_input::{self, Target},
};
use ceh_core::{
    flow::PreparationOutcome,
    model::{ActionKind, DocumentGuard},
};
use serde_json::Value;
use std::time::{Duration, Instant};

/// The step script understands the operations `verify`, `target`, `empty` and `ready_key`.
pub(crate) struct Field<'a> {
    pub action: ActionKind,
    pub script: &'static str,
    pub value: &'a str,
    /// `(operation, prefix typed so far)` -> script payload.
    pub payload: &'a dyn Fn(&str, &str) -> Value,
}
pub(crate) type FreshPage<'a> =
    &'a dyn Fn(&mut BidiConnection, &mut PageObserver) -> Result<ObservedPage, BrowserError>;

pub(crate) fn enter(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    field: &Field<'_>,
    fresh_page: FreshPage<'_>,
    typing_delay: Duration,
    deadline: Instant,
) -> Result<PreparationOutcome, BrowserError> {
    if Instant::now() >= deadline {
        return Ok(PreparationOutcome::Failed);
    }
    let mut fresh = fresh_page(connection, observer)?;
    let mut permit = gate.begin_preparation(guard, &fresh.observation, field.action)?;
    let verified = mutation::perform(connection, &mut permit, &fresh, (field.payload)("verify", ""), field.script)?;
    if verified == Receipt::Unknown {
        return Ok(PreparationOutcome::Unknown);
    }
    if verified == Receipt::Unchanged {
        return Ok(PreparationOutcome::Completed);
    }
    drop(permit);
    let focused = trusted_input::focus(
        connection,
        observer,
        gate,
        guard,
        fresh,
        Target::icp(field.action, (field.payload)("target", ""), field.script),
    )?;
    match focused {
        DispatchOutcome::Unknown => return Ok(PreparationOutcome::Unknown),
        DispatchOutcome::NotDispatched => return Ok(PreparationOutcome::Failed),
        DispatchOutcome::Dispatched => {}
    }
    fresh = fresh_page(connection, observer)?;
    let mut permit = gate.begin_preparation(guard, &fresh.observation, field.action)?;
    match trusted_input::clear_field(connection, &mut permit, &fresh)? {
        DispatchOutcome::Unknown => return Ok(PreparationOutcome::Unknown),
        DispatchOutcome::NotDispatched => return Ok(PreparationOutcome::Failed),
        DispatchOutcome::Dispatched => {}
    }
    let emptied = mutation::perform(connection, &mut permit, &fresh, (field.payload)("empty", ""), field.script)?;
    if emptied == Receipt::Unknown {
        return Ok(PreparationOutcome::Unknown);
    }
    if emptied != Receipt::Unchanged {
        return Ok(PreparationOutcome::Failed);
    }
    let mut prefix = String::new();
    for character in field.value.chars() {
        if Instant::now() >= deadline {
            return Ok(PreparationOutcome::Failed);
        }
        let ready = mutation::perform(connection, &mut permit, &fresh, (field.payload)("ready_key", &prefix), field.script)?;
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
            std::thread::sleep(typing_delay.min(deadline.saturating_duration_since(Instant::now())));
        }
    }
    let verified = mutation::perform(connection, &mut permit, &fresh, (field.payload)("verify", ""), field.script)?;
    if verified == Receipt::Unknown {
        return Ok(PreparationOutcome::Unknown);
    }
    if verified != Receipt::Unchanged {
        return Ok(PreparationOutcome::Failed);
    }
    Ok(PreparationOutcome::Completed)
}
