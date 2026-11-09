//! Browser boundary and deterministic replay; no browser engine dependency.
#[cfg(feature = "bidi")]
pub mod applicant;
#[cfg(feature = "bidi")]
pub mod auth;
#[cfg(feature = "bidi")]
pub mod bidi;
#[cfg(feature = "bidi")]
pub mod contact;
mod dispatch;
#[cfg(feature = "bidi")]
pub mod dom;
#[cfg(feature = "bidi")]
pub mod exit;
#[cfg(feature = "bidi")]
mod mutation;
pub mod navigation;
#[cfg(feature = "bidi")]
pub mod observer;
#[cfg(feature = "bidi")]
pub mod passive;
#[cfg(feature = "bidi")]
pub mod refresh;
#[cfg(feature = "bidi")]
pub mod request;
#[cfg(feature = "bidi")]
pub mod selection;
#[cfg(feature = "bidi")]
mod trusted_input;
use ceh_core::control::{Authority, ControlError, ControlMode, ControlSnapshot};
use ceh_core::model::{ActionKind, DocumentGuard, Observation, Ownership, SessionId};
pub use dispatch::{ActionPermit, DispatchGate};
use std::{collections::VecDeque, future::Future};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserError {
    NoObservation,
    Paused,
    StaleCommand,
    DuplicateCommand,
    InvalidControl,
    EndOfReplay,
    WrongSession,
    StaleDocument,
    ManualOwnership,
    Unsupported,
}

pub use ceh_core::model::DispatchOutcome;

pub trait BrowserPort {
    /// Apply trusted control before dequeuing further browser commands.
    fn apply_control(&mut self, snapshot: &ControlSnapshot) -> Result<(), BrowserError>;
    fn observe(&mut self) -> impl Future<Output = Result<Observation, BrowserError>> + Send;
    /// Recheck session, document, recognized stage and ownership immediately before dispatch.
    fn execute(
        &mut self,
        guard: &DocumentGuard,
        action: ActionKind,
    ) -> impl Future<Output = Result<DispatchOutcome, BrowserError>> + Send;
    fn handoff(
        &mut self,
        guard: &DocumentGuard,
    ) -> impl Future<Output = Result<(), BrowserError>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchedAction {
    pub guard: DocumentGuard,
    pub action: ActionKind,
}

/// Advances only through supplied observations; never creates a browser or I/O.
pub struct ReplayBrowser {
    session: SessionId,
    pending: VecDeque<Observation>,
    current: Option<Observation>,
    authority: Authority,
    actions: Vec<DispatchedAction>,
}

impl ReplayBrowser {
    pub fn new(session: SessionId, observations: Vec<Observation>) -> Self {
        Self {
            authority: Authority::new(session.clone()),
            session,
            pending: observations.into(),
            current: None,
            actions: Vec::new(),
        }
    }
    pub fn actions(&self) -> &[DispatchedAction] {
        &self.actions
    }
    pub fn ownership(&self) -> Ownership {
        if self.authority.mode() == ControlMode::Automatic {
            Ownership::Automatic
        } else {
            Ownership::Manual
        }
    }

    fn validate(&self, guard: &DocumentGuard) -> Result<(), BrowserError> {
        if self.session != guard.session {
            return Err(BrowserError::WrongSession);
        }
        if guard.epoch != self.authority.epoch() {
            return Err(BrowserError::StaleCommand);
        }
        let current = self.current.as_ref().ok_or(BrowserError::NoObservation)?;
        if current.document != guard.document || current.kind != guard.expected_page {
            return Err(BrowserError::StaleDocument);
        }
        Ok(())
    }
}

impl BrowserPort for ReplayBrowser {
    fn apply_control(&mut self, snapshot: &ControlSnapshot) -> Result<(), BrowserError> {
        self.authority.apply(snapshot).map_err(|error| match error {
            ControlError::WrongSession => BrowserError::WrongSession,
            ControlError::ManualOwnership => BrowserError::ManualOwnership,
            ControlError::StaleUpdate => BrowserError::StaleCommand,
            ControlError::EpochExhausted => BrowserError::InvalidControl,
        })
    }
    async fn observe(&mut self) -> Result<Observation, BrowserError> {
        if self.authority.mode() == ControlMode::Paused {
            return Err(BrowserError::Paused);
        }
        let next = self.pending.pop_front().ok_or(BrowserError::EndOfReplay)?;
        self.current = Some(next.clone());
        Ok(next)
    }
    async fn execute(
        &mut self,
        guard: &DocumentGuard,
        action: ActionKind,
    ) -> Result<DispatchOutcome, BrowserError> {
        if self.authority.mode() == ControlMode::Paused {
            return Err(BrowserError::Paused);
        }
        if self.authority.mode() == ControlMode::Manual {
            return Err(BrowserError::ManualOwnership);
        }
        self.validate(guard)?;
        if self
            .actions
            .iter()
            .any(|previous| previous.guard.document == guard.document && previous.action == action)
        {
            return Err(BrowserError::DuplicateCommand);
        }
        self.actions.push(DispatchedAction {
            guard: guard.clone(),
            action,
        });
        Ok(DispatchOutcome::Dispatched)
    }
    async fn handoff(&mut self, guard: &DocumentGuard) -> Result<(), BrowserError> {
        self.validate(guard)?;
        self.authority
            .handoff()
            .map_err(|_| BrowserError::InvalidControl)?;
        Ok(())
    }
}

#[cfg(feature = "bidi")]
pub mod cookies;

#[cfg(feature = "bidi")]
pub mod public_details;

#[cfg(feature = "bidi")]
pub mod recovery_controls;

#[cfg(feature = "bidi")]
pub mod launch;

#[cfg(feature = "bidi")]
pub mod firefox;

#[cfg(feature = "bidi")]
pub mod session_driver;

#[cfg(feature = "bidi")]
mod firefox_certificates;
