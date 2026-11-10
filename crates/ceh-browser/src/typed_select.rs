//! A `<select>` chosen exactly like Python (`site._select_without_focus`): the step script sets the
//! option and fires ONE `input` and ONE `change` event, without focus and without keyboard or pointer
//! events on the list. Choosing it with the keyboard's type-ahead (a click on the select, then a key per
//! character) fired a burst of selection changes and requests that the site's WAF answered with a block
//! (live timeline 2026-11-10 21:48); the script choice is what Python does and what every earlier Rust
//! run used. Clicks on buttons and typing into text fields stay trusted browser input.
use crate::{
    BrowserError, DispatchGate,
    bidi::BidiConnection,
    mutation::{self, Receipt},
    observer::PageObserver,
    typed_field::FreshPage,
};
use ceh_core::model::{ActionKind, DocumentGuard};
use serde_json::Value;

/// The step script understands `select`.
pub(crate) struct Choice<'a> {
    pub action: ActionKind,
    pub script: &'static str,
    pub payload: &'a dyn Fn(&str) -> Value,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Chosen {
    Done,
    /// The page is not ready (no single matching, enabled option yet): nothing was changed, waiting is safe.
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
    match mutation::perform(connection, &mut permit, &fresh, (choice.payload)("select"), choice.script)? {
        Receipt::Unchanged | Receipt::Changed => Ok(Chosen::Done),
        Receipt::Refused => Ok(Chosen::NotReady),
        Receipt::Unknown => Ok(Chosen::Unknown),
    }
}
