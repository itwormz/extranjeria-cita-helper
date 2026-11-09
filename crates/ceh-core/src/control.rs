//! Pure control authority. Only trusted runtime control messages may update it.
use crate::model::SessionId;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlMode {
    Automatic,
    Paused,
    Manual,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlSnapshot {
    session: SessionId,
    epoch: u64,
    mode: ControlMode,
}
impl ControlSnapshot {
    pub fn session(&self) -> &SessionId {
        &self.session
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn mode(&self) -> ControlMode {
        self.mode
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlError {
    WrongSession,
    StaleUpdate,
    ManualOwnership,
    EpochExhausted,
}
pub struct Authority {
    state: ControlSnapshot,
}
impl Authority {
    pub fn new(session: SessionId) -> Self {
        Self {
            state: ControlSnapshot {
                session,
                epoch: 0,
                mode: ControlMode::Automatic,
            },
        }
    }
    pub fn snapshot(&self) -> ControlSnapshot {
        self.state.clone()
    }
    pub fn epoch(&self) -> u64 {
        self.state.epoch
    }
    pub fn mode(&self) -> ControlMode {
        self.state.mode
    }
    fn transition(&mut self, mode: ControlMode) -> Result<ControlSnapshot, ControlError> {
        if self.state.mode == ControlMode::Manual && mode != ControlMode::Manual {
            return Err(ControlError::ManualOwnership);
        }
        if self.state.mode != mode {
            let epoch = self
                .state
                .epoch
                .checked_add(1)
                .ok_or(ControlError::EpochExhausted)?;
            self.state.epoch = epoch;
            self.state.mode = mode;
        }
        Ok(self.snapshot())
    }
    pub fn pause(&mut self) -> Result<ControlSnapshot, ControlError> {
        self.transition(ControlMode::Paused)
    }
    pub fn resume(&mut self) -> Result<ControlSnapshot, ControlError> {
        self.transition(ControlMode::Automatic)
    }
    pub fn handoff(&mut self) -> Result<ControlSnapshot, ControlError> {
        self.transition(ControlMode::Manual)
    }
    pub fn apply(&mut self, incoming: &ControlSnapshot) -> Result<(), ControlError> {
        if incoming.session != self.state.session {
            return Err(ControlError::WrongSession);
        }
        if self.state.mode == ControlMode::Manual && incoming.mode != ControlMode::Manual {
            return Err(ControlError::ManualOwnership);
        }
        if incoming.epoch < self.state.epoch
            || (incoming.epoch == self.state.epoch && incoming.mode != self.state.mode)
        {
            return Err(ControlError::StaleUpdate);
        }
        self.state = incoming.clone();
        Ok(())
    }
}
