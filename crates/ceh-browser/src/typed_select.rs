//! A `<select>` chosen the way a person does with the keyboard: focus by pointer, close the list, then
//! type the option's label (the browser's own type-ahead commits it). Page scripts only read: they
//! resolve the single matching option, report its label and verify the result. Arrow keys, Space and
//! Enter do not change a select through WebDriver input (measured), so they are never used.
use crate::{
    BrowserError, DispatchGate, DispatchOutcome,
    bidi::BidiConnection,
    mutation::{self, Receipt},
    observer::PageObserver,
    trusted_input::{self, Target},
    typed_field::FreshPage,
};
use ceh_core::model::{ActionKind, DocumentGuard};
use serde_json::Value;

/// The step script understands `verify`, `target` and `label`.
pub(crate) struct Choice<'a> {
    pub action: ActionKind,
    pub script: &'static str,
    pub payload: &'a dyn Fn(&str) -> Value,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Chosen {
    Done,
    /// The page is not ready or the typed label did not select the option: nothing is known to be wrong.
    NotReady,
    Unknown,
}
pub(crate) fn choose(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    choice: &Choice<'_>,
    fresh_page: FreshPage<'_>,
) -> Result<Chosen, BrowserError> {
    let fresh = fresh_page(connection, observer)?;
    let mut permit = gate.begin_preparation(guard, &fresh.observation, choice.action)?;
    match mutation::perform(connection, &mut permit, &fresh, (choice.payload)("verify"), choice.script)? {
        Receipt::Unchanged | Receipt::Changed => return Ok(Chosen::Done),
        Receipt::Unknown => return Ok(Chosen::Unknown),
        Receipt::Refused => {}
    }
    let Some(label) = mutation::read_label(connection, &mut permit, &fresh, (choice.payload)("label"), choice.script)? else {
        return Ok(Chosen::NotReady);
    };
    drop(permit);
    match trusted_input::focus(
        connection,
        observer,
        gate,
        guard,
        fresh,
        Target::icp(choice.action, (choice.payload)("target"), choice.script),
    )? {
        DispatchOutcome::Unknown => return Ok(Chosen::Unknown),
        DispatchOutcome::NotDispatched => return Ok(Chosen::NotReady),
        DispatchOutcome::Dispatched => {}
    }
    let fresh = fresh_page(connection, observer)?;
    let mut permit = gate.begin_preparation(guard, &fresh.observation, choice.action)?;
    for outcome in [
        trusted_input::escape(connection, &mut permit, &fresh)?,
        trusted_input::type_text(connection, &mut permit, &fresh, &label)?,
    ] {
        match outcome {
            DispatchOutcome::Unknown => return Ok(Chosen::Unknown),
            DispatchOutcome::NotDispatched => return Ok(Chosen::NotReady),
            DispatchOutcome::Dispatched => {}
        }
    }
    // Type-ahead selects the first option that starts with the typed text: always confirm by reading.
    match mutation::perform(connection, &mut permit, &fresh, (choice.payload)("verify"), choice.script)? {
        Receipt::Unchanged | Receipt::Changed => Ok(Chosen::Done),
        Receipt::Refused => Ok(Chosen::NotReady),
        Receipt::Unknown => Ok(Chosen::Unknown),
    }
}
