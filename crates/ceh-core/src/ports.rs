//! Narrow boundaries; implementations must keep OS/network/runtime code outside core.
use crate::model::{ActionKind, DocumentId, Observation, ProfileId, SessionId};
use std::{fmt, future::Future};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortError {
    Unavailable,
    InvalidData,
    Unsupported,
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockReading {
    pub unix_millis: i64,
    pub monotonic_millis: u64,
}
pub trait Clock {
    fn now(&self) -> ClockReading;
}

pub struct ManualClock {
    reading: ClockReading,
}
impl ManualClock {
    pub fn new(reading: ClockReading) -> Self {
        Self { reading }
    }
    pub fn advance(&mut self, millis: u64) -> Result<(), PortError> {
        let monotonic_millis = self
            .reading
            .monotonic_millis
            .checked_add(millis)
            .ok_or(PortError::InvalidData)?;
        let delta = i64::try_from(millis).map_err(|_| PortError::InvalidData)?;
        let unix_millis = self
            .reading
            .unix_millis
            .checked_add(delta)
            .ok_or(PortError::InvalidData)?;
        self.reading = ClockReading {
            unix_millis,
            monotonic_millis,
        };
        Ok(())
    }
    pub fn set_wall(&mut self, unix_millis: i64) {
        self.reading.unix_millis = unix_millis;
    }
}
impl Clock for ManualClock {
    fn now(&self) -> ClockReading {
        self.reading
    }
}

/// Intentionally neither serializable nor printable. No zeroization claim is made.
pub struct SecretBytes(Vec<u8>);
impl SecretBytes {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}
impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretBytes([REDACTED])")
    }
}

pub trait SecretStore {
    fn load(
        &mut self,
        reference: &str,
    ) -> impl Future<Output = Result<SecretBytes, PortError>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainEvent {
    Statistics {
        event: Box<crate::statistics::PublicEvent>,
    },
    JournalUpdated {
        profile: ProfileId,
        session: SessionId,
        row: Box<crate::journal::JournalRow>,
    },
    PageObserved {
        profile: ProfileId,
        session: SessionId,
        observation: Observation,
    },
    ActionDispatched {
        profile: ProfileId,
        session: SessionId,
        document: DocumentId,
        action: ActionKind,
    },
    Handoff {
        profile: ProfileId,
        session: SessionId,
        document: DocumentId,
    },
}

pub trait StoragePort {
    fn record(&mut self, event: &DomainEvent)
    -> impl Future<Output = Result<(), PortError>> + Send;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationKind {
    Authentication,
    OfficeSelection,
    BookingSelection,
    StoppedWithError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub profile: ProfileId,
    pub session: SessionId,
    pub document: DocumentId,
    pub kind: NotificationKind,
}
pub trait NotificationSink {
    fn send(
        &mut self,
        notification: &Notification,
    ) -> impl Future<Output = Result<(), PortError>> + Send;
}
