//! Shared revocation boundary for a native browser worker and its controller.
use crate::{BrowserError, DispatchOutcome};
use ceh_core::{
    control::{Authority, ControlError, ControlMode, ControlSnapshot},
    model::{ActionKind, DocumentGuard, Observation, SessionId},
};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReservationStage {
    Preparation,
    Submission,
}
struct State {
    session: SessionId,
    authority: Authority,
    interrupted: bool,
    interrupt_generation: u64,
    submitted: Vec<(ceh_core::model::DocumentId, ActionKind, ReservationStage)>,
}

/// Clones share one authority. Browser responses must never be awaited under this gate.
#[derive(Clone)]
pub struct DispatchGate(Arc<Mutex<State>>);
impl DispatchGate {
    pub fn new(session: SessionId) -> Self {
        Self(Arc::new(Mutex::new(State {
            authority: Authority::new(session.clone()),
            session,
            submitted: Vec::new(),
            interrupted: false,
            interrupt_generation: 0,
        })))
    }

    pub fn apply_control(&self, snapshot: &ControlSnapshot) -> Result<(), BrowserError> {
        self.0
            .lock()
            .map_err(|_| BrowserError::InvalidControl)?
            .authority
            .apply(snapshot)
            .map_err(|error| match error {
                ControlError::WrongSession => BrowserError::WrongSession,
                ControlError::ManualOwnership => BrowserError::ManualOwnership,
                ControlError::StaleUpdate => BrowserError::StaleCommand,
                ControlError::EpochExhausted => BrowserError::InvalidControl,
            })
    }

    /// Revoke new writes immediately without inventing another control epoch.
    /// The worker remains the only owner of core pause/resume transitions.
    pub fn interrupt(&self) -> Result<u64, BrowserError> {
        let mut state = self.0.lock().map_err(|_| BrowserError::InvalidControl)?;
        // Revoke first: exhaustion must fail closed.
        state.interrupted = true;
        state.interrupt_generation = state
            .interrupt_generation
            .checked_add(1)
            .ok_or(BrowserError::InvalidControl)?;
        Ok(state.interrupt_generation)
    }

    /// Only a matching, resumed worker snapshot may reopen the write boundary.
    pub fn clear_interrupt(
        &self,
        snapshot: &ControlSnapshot,
        generation: u64,
    ) -> Result<(), BrowserError> {
        let mut state = self.0.lock().map_err(|_| BrowserError::InvalidControl)?;
        if generation != state.interrupt_generation
            || snapshot != &state.authority.snapshot()
            || snapshot.mode() != ControlMode::Automatic
        {
            return Err(BrowserError::InvalidControl);
        }
        state.interrupted = false;
        Ok(())
    }

    /// Reserve the final submission; preparation does not mark this as submitted.
    pub fn begin(
        &self,
        guard: &DocumentGuard,
        current: &Observation,
        action: ActionKind,
    ) -> Result<ActionPermit, BrowserError> {
        self.reserve(guard, current, action, ReservationStage::Submission)
    }
    /// Match core's Prepare phase. Acknowledged field changes can be prepared again
    /// after pause; ambiguous writes remain reserved. This never permits submission.
    pub fn begin_preparation(
        &self,
        guard: &DocumentGuard,
        current: &Observation,
        action: ActionKind,
    ) -> Result<ActionPermit, BrowserError> {
        self.reserve(guard, current, action, ReservationStage::Preparation)
    }
    fn reserve(
        &self,
        guard: &DocumentGuard,
        current: &Observation,
        action: ActionKind,
        stage: ReservationStage,
    ) -> Result<ActionPermit, BrowserError> {
        let mut state = self.0.lock().map_err(|_| BrowserError::InvalidControl)?;
        validate(&state, guard, current)?;
        let key = (guard.document.clone(), action, stage);
        if state.submitted.contains(&key) {
            return Err(BrowserError::DuplicateCommand);
        }
        state.submitted.push(key);
        Ok(ActionPermit {
            gate: self.clone(),
            guard: guard.clone(),
            action,
            stage,
            changed: false,
            ambiguous: false,
            pending: false,
        })
    }

    /// Single-step convenience. Unknown delivery is reserved, never automatically retried.
    pub fn dispatch(
        &self,
        guard: &DocumentGuard,
        current: &Observation,
        action: ActionKind,
        send: impl FnOnce() -> DispatchOutcome,
    ) -> Result<DispatchOutcome, BrowserError> {
        let mut permit = self.begin(guard, current, action)?;
        permit.send(current, send)?;
        Ok(permit.finish())
    }
}
fn validate(
    state: &State,
    guard: &DocumentGuard,
    current: &Observation,
) -> Result<(), BrowserError> {
    if guard.session != state.session {
        return Err(BrowserError::WrongSession);
    }
    match state.authority.mode() {
        ControlMode::Paused => return Err(BrowserError::Paused),
        ControlMode::Manual => return Err(BrowserError::ManualOwnership),
        ControlMode::Automatic => {}
    }
    if state.interrupted {
        return Err(BrowserError::Paused);
    }
    if guard.epoch != state.authority.epoch() {
        return Err(BrowserError::StaleCommand);
    }
    if guard.document != current.document || guard.expected_page != current.kind {
        return Err(BrowserError::StaleDocument);
    }
    Ok(())
}

/// A linear transport reservation, not another flow engine. No clone or serialization.
/// Every write rechecks the original epoch/document; responses are awaited outside its lock.
pub struct ActionPermit {
    gate: DispatchGate,
    guard: DocumentGuard,
    action: ActionKind,
    stage: ReservationStage,
    changed: bool,
    ambiguous: bool,
    pending: bool,
}
impl ActionPermit {
    /// Only a bounded transport write belongs in this closure. Never await a reply here.
    pub fn send(
        &mut self,
        current: &Observation,
        send: impl FnOnce() -> DispatchOutcome,
    ) -> Result<DispatchOutcome, BrowserError> {
        let state = self
            .gate
            .0
            .lock()
            .map_err(|_| BrowserError::InvalidControl)?;
        validate(&state, &self.guard, current)?;
        if self.pending {
            return Err(BrowserError::InvalidControl);
        }
        if self.ambiguous {
            return Err(BrowserError::DuplicateCommand);
        }
        // Mark unknown before entering user code: unwinding cannot release an uncertain write.
        self.pending = true;
        let result = send();
        match result {
            DispatchOutcome::Unknown => {}
            DispatchOutcome::Dispatched => {
                self.pending = false;
                self.changed = true;
            }
            DispatchOutcome::NotDispatched => {
                self.pending = false;
            }
        }
        Ok(result)
    }
    /// Confirm exactly the outstanding step. Prior changed steps cannot be erased by
    /// a later refusal. Unknown means abort this phase, with its reservation retained.
    pub fn complete_step(&mut self, outcome: DispatchOutcome) -> Result<(), BrowserError> {
        if !self.pending {
            return Err(BrowserError::InvalidControl);
        }
        self.pending = false;
        match outcome {
            DispatchOutcome::Unknown => self.ambiguous = true,
            DispatchOutcome::Dispatched => self.changed = true,
            DispatchOutcome::NotDispatched => {}
        }
        Ok(())
    }
    pub fn finish(self) -> DispatchOutcome {
        if self.pending || self.ambiguous {
            DispatchOutcome::Unknown
        } else if self.changed {
            DispatchOutcome::Dispatched
        } else {
            DispatchOutcome::NotDispatched
        }
    }
}
impl Drop for ActionPermit {
    fn drop(&mut self) {
        if (!self.changed || self.stage == ReservationStage::Preparation)
            && !self.pending
            && !self.ambiguous
            && let Ok(mut state) = self.gate.0.lock()
        {
            state
                .submitted
                .retain(|key| key != &(self.guard.document.clone(), self.action, self.stage));
        }
    }
}
