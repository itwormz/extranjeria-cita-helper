//! Service-specific applicant preparation. Final submission is a separate core effect.
use crate::{
    BrowserError, DispatchGate, DispatchOutcome,
    bidi::BidiConnection,
    mutation::{self, Receipt},
    observer::{ObservedPage, PageObserver},
    typed_field, typed_select,
};
use ceh_core::{
    flow::PreparationOutcome,
    model::{ActionKind, DocumentGuard, PageKind},
    profiles::ProfileDraft,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
const SCRIPT: &str = include_str!("applicant-step.js");
/// Never Debug/serialize this object: private profile data goes only to its own browser.
pub struct ApplicantInput {
    regreso: bool,
    copy: bool,
    continue_empty: bool,
    document_type: String,
    number: String,
    name: String,
    nationality: String,
}
fn bounded(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
impl ApplicantInput {
    pub fn from_profile(profile: &ProfileDraft) -> Result<Self, BrowserError> {
        if !matches!(profile.procedure.as_str(), "huellas" | "regreso")
            || !matches!(profile.fill_mode.as_str(), "manual" | "copy")
            || !matches!(
                profile.applicant_no_slots_mode.as_str(),
                "wait" | "continue"
            )
        {
            return Err(BrowserError::Unsupported);
        }
        let regreso = profile.procedure == "regreso";
        let copy = profile.fill_mode == "copy";
        if copy && !matches!(profile.access_mode.as_str(), "certificate" | "clave") {
            return Err(BrowserError::Unsupported);
        }
        let kind = if regreso {
            profile.document_type.as_str()
        } else {
            "NIE"
        };
        if !copy
            && (!matches!(kind, "NIE" | "DNI" | "PASSPORT")
                || !bounded(&profile.document_number, 128)
                || !bounded(&profile.full_name, 1024))
        {
            return Err(BrowserError::Unsupported);
        }
        if !regreso && !bounded(&profile.nationality, 256) {
            return Err(BrowserError::Unsupported);
        }
        Ok(Self {
            regreso,
            copy,
            continue_empty: profile.applicant_no_slots_mode == "continue",
            document_type: if copy { String::new() } else { kind.to_owned() },
            number: if copy {
                String::new()
            } else {
                profile.document_number.clone()
            },
            name: if copy {
                String::new()
            } else {
                profile.full_name.clone()
            },
            nationality: if regreso {
                String::new()
            } else {
                profile.nationality.clone()
            },
        })
    }
    fn mode(&self) -> &'static str {
        if self.copy { "copy" } else { "manual" }
    }
    fn payload(&self, operation: &str) -> Value {
        self.field_payload(operation, "", "")
    }
    fn field_payload(&self, operation: &str, field: &str, prefix: &str) -> Value {
        json!({"operation":operation,"field":field,"prefix":prefix,"fillMode":self.mode(),"regreso":self.regreso,"copy":self.copy,"documentType":self.document_type,"number":self.number,"name":self.name,"nationality":self.nationality})
    }
}
fn fresh_page(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    input: &ApplicantInput,
) -> Result<ObservedPage, BrowserError> {
    let fresh = observer
        .observe(connection, input.mode())
        .map_err(|_| BrowserError::NoObservation)?;
    if !fresh.page.snapshot.official {
        return Err(BrowserError::Unsupported);
    }
    if !matches!(
        fresh.observation.kind,
        PageKind::Applicant | PageKind::ApplicantNoSlots
    ) {
        return Err(BrowserError::StaleDocument);
    }
    if fresh.observation.kind == PageKind::ApplicantNoSlots && !input.continue_empty {
        return Err(BrowserError::Unsupported);
    }
    Ok(fresh)
}
/// Press an element with the browser's own pointer, then wait for the page to show the expected state.
/// `satisfied` and `verify` are read-only script operations, `target` resolves the element to press.
fn press_until(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    input: &ApplicantInput,
    deadline: Instant,
    (satisfied, target, verify): (&str, &str, &str),
) -> Result<PreparationOutcome, BrowserError> {
    let fresh = fresh_page(connection, observer, input)?;
    let mut permit = gate.begin_preparation(guard, &fresh.observation, ActionKind::Applicant)?;
    match mutation::perform(connection, &mut permit, &fresh, input.payload(satisfied), SCRIPT)? {
        Receipt::Unchanged | Receipt::Changed => return Ok(PreparationOutcome::Completed),
        Receipt::Unknown => return Ok(PreparationOutcome::Unknown),
        Receipt::Refused => {}
    }
    drop(permit);
    match crate::trusted_input::focus(
        connection,
        observer,
        gate,
        guard,
        fresh,
        crate::trusted_input::Target::icp(ActionKind::Applicant, input.payload(target), SCRIPT),
    )? {
        DispatchOutcome::Unknown => return Ok(PreparationOutcome::Unknown),
        DispatchOutcome::NotDispatched => return Ok(PreparationOutcome::Failed),
        DispatchOutcome::Dispatched => {}
    }
    loop {
        if Instant::now() >= deadline {
            return Ok(PreparationOutcome::Failed);
        }
        let fresh = fresh_page(connection, observer, input)?;
        let mut permit = gate.begin_preparation(guard, &fresh.observation, ActionKind::Applicant)?;
        match mutation::perform(connection, &mut permit, &fresh, input.payload(verify), SCRIPT)? {
            Receipt::Unchanged | Receipt::Changed => return Ok(PreparationOutcome::Completed),
            Receipt::Unknown => return Ok(PreparationOutcome::Unknown),
            Receipt::Refused => {}
        }
        drop(permit);
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return Ok(PreparationOutcome::Failed);
        };
        std::thread::sleep(remaining.min(Duration::from_millis(250)));
    }
}
/// Fill the applicant form with the browser's own input only: pointer for the document type and the
/// copy button, keyboard for text and for the nationality list. Scripts only read and resolve elements.
pub fn prepare_applicant(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    input: &ApplicantInput,
    typing_delay: Duration,
    deadline: Instant,
) -> Result<PreparationOutcome, BrowserError> {
    let typing_delay = typed_field::typing_delay(typing_delay);
    if Instant::now() >= deadline {
        return Ok(PreparationOutcome::Failed);
    }
    let page = |connection: &mut BidiConnection, observer: &mut PageObserver| fresh_page(connection, observer, input);
    let mut operations = if input.copy {
        vec!["copy", "verify_copy"]
    } else {
        vec!["document_type", "number", "name"]
    };
    if !input.regreso {
        operations.push("nationality");
    }
    for operation in operations {
        let outcome = match operation {
            "document_type" => press_until(connection, observer, gate, guard, input, deadline, ("verify_document_type", "target_document_type", "verify_document_type"))?,
            "copy" => press_until(connection, observer, gate, guard, input, deadline, ("verify_copy_ready", "target_copy", "verify_copy"))?,
            "verify_copy" => {
                let fresh = fresh_page(connection, observer, input)?;
                let mut permit = gate.begin_preparation(guard, &fresh.observation, ActionKind::Applicant)?;
                match mutation::perform(connection, &mut permit, &fresh, input.payload("verify_copy"), SCRIPT)? {
                    Receipt::Unchanged | Receipt::Changed => PreparationOutcome::Completed,
                    Receipt::Unknown => PreparationOutcome::Unknown,
                    Receipt::Refused => PreparationOutcome::Failed,
                }
            }
            "number" | "name" => {
                let payload = |op: &str, prefix: &str| input.field_payload(op, operation, prefix);
                let spec = typed_field::Field {
                    action: ActionKind::Applicant,
                    script: SCRIPT,
                    value: if operation == "number" { &input.number } else { &input.name },
                    payload: &payload,
                };
                typed_field::enter(connection, observer, gate, guard, &spec, &page, typing_delay, deadline)?
            }
            _ => {
                let payload = |op: &str| input.field_payload(op, "nationality", "");
                let choice = typed_select::Choice { action: ActionKind::Applicant, script: SCRIPT, payload: &payload, typing_delay };
                loop {
                    if Instant::now() >= deadline {
                        break PreparationOutcome::Failed;
                    }
                    match typed_select::choose(connection, observer, gate, guard, &choice, &page)? {
                        typed_select::Chosen::Done => break PreparationOutcome::Completed,
                        typed_select::Chosen::Unknown => break PreparationOutcome::Unknown,
                        typed_select::Chosen::Mismatch => break PreparationOutcome::Failed,
                        typed_select::Chosen::NotReady => std::thread::sleep(
                            Duration::from_millis(250).min(deadline.saturating_duration_since(Instant::now())),
                        ),
                    }
                }
            }
        };
        if outcome != PreparationOutcome::Completed {
            return Ok(outcome);
        }
    }
    Ok(PreparationOutcome::Completed)
}
pub fn activate_applicant(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    input: &ApplicantInput,
) -> Result<DispatchOutcome, BrowserError> {
    let fresh = fresh_page(connection, observer, input)?;
    crate::trusted_input::click(
        connection,
        observer,
        gate,
        guard,
        fresh,
        crate::trusted_input::Target::icp(ActionKind::Applicant, input.payload("submit"), SCRIPT),
    )
}
