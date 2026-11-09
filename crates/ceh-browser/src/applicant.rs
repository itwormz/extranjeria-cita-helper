//! Service-specific applicant preparation. Final submission is a separate core effect.
use crate::{
    BrowserError, DispatchGate, DispatchOutcome,
    bidi::BidiConnection,
    mutation::{self, Receipt},
    observer::{ObservedPage, PageObserver},
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
        json!({"operation":operation,"fillMode":self.mode(),"regreso":self.regreso,"copy":self.copy,"documentType":self.document_type,"number":self.number,"name":self.name,"nationality":self.nationality})
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
pub fn prepare_applicant(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    input: &ApplicantInput,
    deadline: Instant,
) -> Result<PreparationOutcome, BrowserError> {
    if Instant::now() >= deadline {
        return Ok(PreparationOutcome::Failed);
    }
    let mut fresh = fresh_page(connection, observer, input)?;
    let mut permit = gate.begin_preparation(guard, &fresh.observation, ActionKind::Applicant)?;
    let mut operations = if input.copy {
        vec!["copy", "verify_copy"]
    } else {
        vec!["document_type", "number", "name"]
    };
    if !input.regreso {
        operations.push("nationality");
    }
    for operation in operations {
        loop {
            if Instant::now() >= deadline {
                return Ok(PreparationOutcome::Failed);
            }
            let result = mutation::perform(
                connection,
                &mut permit,
                &fresh,
                input.payload(operation),
                SCRIPT,
            )?;
            if result.completed() {
                break;
            }
            if result == Receipt::Unknown {
                return Ok(PreparationOutcome::Unknown);
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Ok(PreparationOutcome::Failed);
            };
            std::thread::sleep(remaining.min(Duration::from_millis(250)));
            fresh = fresh_page(connection, observer, input)?;
        }
        fresh = fresh_page(connection, observer, input)?;
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
