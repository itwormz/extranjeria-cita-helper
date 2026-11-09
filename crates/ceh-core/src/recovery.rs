//! Recovery policy without I/O. The caller must still reobserve before dispatch.
use crate::{
    model::*,
    timings::{AssistTimings, RetryKind, TimingError},
};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryEndpoint {
    #[default]
    Other,
    Availability,
    AuthInfo,
    AuthReturn,
    ApplicantEntry,
    Index,
    ClavePortal,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RecoveryFacts {
    pub official_site: bool,
    pub endpoint: RecoveryEndpoint,
    pub last_action: Option<ActionKind>,
    pub pending_action: Option<ActionKind>,
    pub own_control: bool,
    pub certificate_validation_error: bool,
    /// True only for a recognized recovery-first message not yet attempted.
    pub immediate_expiry_control: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum RecoveryAction {
    Hold,
    Reload,
    ActivateOwnControl,
    GetCurrent,
    GetEntry,
    Restart { clear_profile: bool, reopen: bool },
}
#[derive(Debug, Clone, PartialEq)]
pub struct RecoveryPlan {
    pub delay_seconds: f64,
    pub after: RecoveryAction,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryError {
    UnsupportedPage,
    InvalidTiming,
}
impl From<TimingError> for RecoveryError {
    fn from(_: TimingError) -> Self {
        Self::InvalidTiming
    }
}
pub fn plan_recovery(
    observed: &Observation,
    facts: &RecoveryFacts,
    timings: &AssistTimings,
    attempt: usize,
    jitter: f64,
) -> Result<RecoveryPlan, RecoveryError> {
    let effective = facts.pending_action.or(facts.last_action);
    let delay = |kind| timings.recovery_delay(kind, attempt, jitter);
    let (mut seconds, after) = match observed.kind {
        PageKind::RateLimited => (
            delay(RetryKind::RateLimit)?,
            if observed.method == RequestMethod::Get {
                RecoveryAction::GetCurrent
            } else {
                RecoveryAction::GetEntry
            },
        ),
        PageKind::Blocked => (
            delay(RetryKind::Blocked)?,
            RecoveryAction::Restart {
                clear_profile: true,
                reopen: false,
            },
        ),
        PageKind::ServerError if facts.endpoint == RecoveryEndpoint::ClavePortal => (
            delay(RetryKind::ClaveError)?,
            RecoveryAction::Restart {
                clear_profile: false,
                reopen: true,
            },
        ),
        PageKind::Expired => {
            let seconds = delay(RetryKind::Expired)?;
            (
                if facts.immediate_expiry_control {
                    0.0
                } else {
                    seconds
                },
                if facts.own_control {
                    RecoveryAction::ActivateOwnControl
                } else {
                    RecoveryAction::Hold
                },
            )
        }
        PageKind::ServerError if facts.official_site && facts.own_control => (
            delay(RetryKind::ServerButton)?,
            RecoveryAction::ActivateOwnControl,
        ),
        PageKind::ServerError => {
            let repeatable_search = observed.method == RequestMethod::Post
                && facts.official_site
                && facts.endpoint == RecoveryEndpoint::Availability
                && effective == Some(ActionKind::Request);
            let action = if observed.method == RequestMethod::Get || repeatable_search {
                RecoveryAction::Reload
            } else if observed.method == RequestMethod::Post
                && effective != Some(ActionKind::Contact)
                && (!facts.official_site
                    || matches!(
                        facts.endpoint,
                        RecoveryEndpoint::AuthInfo
                            | RecoveryEndpoint::AuthReturn
                            | RecoveryEndpoint::ApplicantEntry
                            | RecoveryEndpoint::Index
                    ))
            {
                RecoveryAction::GetEntry
            } else {
                RecoveryAction::Hold
            };
            (delay(RetryKind::ServerRetry)?, action)
        }
        _ => return Err(RecoveryError::UnsupportedPage),
    };
    if facts.certificate_validation_error && observed.kind == PageKind::ServerError {
        seconds = seconds.max(delay(RetryKind::ClaveError)?);
    }
    Ok(RecoveryPlan {
        delay_seconds: seconds,
        after,
    })
}
