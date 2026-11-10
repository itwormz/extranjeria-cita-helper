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
use std::time::Duration;

/// The step script understands `verify`, `target` and `label`.
pub(crate) struct Choice<'a> {
    pub action: ActionKind,
    pub script: &'static str,
    pub payload: &'a dyn Fn(&str) -> Value,
    /// The profile's pause between typed characters (clamped like the text fields').
    pub typing_delay: Duration,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Chosen {
    Done,
    /// The page is not ready (nothing was typed): waiting and trying again is safe.
    NotReady,
    /// The label was typed but the page shows another option (type-ahead prefix collision, or the page
    /// resets the value): typing again would repeat the same wrong change, so the step fails at once.
    Mismatch,
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
    match trusted_input::escape(connection, &mut permit, &fresh)? {
        DispatchOutcome::Unknown => return Ok(Chosen::Unknown),
        DispatchOutcome::NotDispatched => return Ok(Chosen::NotReady),
        DispatchOutcome::Dispatched => {}
    }
    for (character, pause) in paced_keys(&label, choice.typing_delay) {
        match trusted_input::key(connection, &mut permit, &fresh, character)? {
            DispatchOutcome::Unknown => return Ok(Chosen::Unknown),
            DispatchOutcome::NotDispatched => return Ok(Chosen::NotReady),
            DispatchOutcome::Dispatched => {}
        }
        if !pause.is_zero() {
            std::thread::sleep(pause);
        }
    }
    // Type-ahead selects the first option that starts with the typed text: always confirm by reading.
    match mutation::perform(connection, &mut permit, &fresh, (choice.payload)("verify"), choice.script)? {
        Receipt::Unchanged | Receipt::Changed => Ok(Chosen::Done),
        Receipt::Refused => Ok(Chosen::Mismatch),
        Receipt::Unknown => Ok(Chosen::Unknown),
    }
}

/// One key per character with the profile's typing pause after each, never a burst: a person's keystrokes
/// are spaced, and the site's page script reacts (and reports to the WAF) to every selection change.
pub(crate) fn paced_keys(label: &str, configured: Duration) -> Vec<(char, Duration)> {
    let pause = crate::typed_field::typing_delay(configured);
    label.chars().map(|character| (character, pause)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_label_is_typed_one_key_at_a_time_with_the_clamped_typing_pause() {
        let keys = paced_keys("POLICÍA-T", Duration::from_millis(97));
        assert_eq!(keys.len(), 9);
        assert_eq!(keys.iter().map(|(c, _)| *c).collect::<String>(), "POLICÍA-T");
        assert!(keys.iter().all(|(_, pause)| *pause == Duration::from_millis(97)));
        let slow = paced_keys("ab", Duration::from_millis(900));
        assert!(slow.iter().all(|(_, pause)| *pause == crate::typed_field::MAX_TYPING_DELAY));
        assert!(paced_keys("ab", Duration::ZERO).iter().all(|(_, pause)| pause.is_zero()));
    }
}
