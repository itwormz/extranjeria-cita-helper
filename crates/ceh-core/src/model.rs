//! Typed domain values only: no browser handles, GUI objects, credentials or I/O.
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidId;

impl fmt::Display for InvalidId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never echo potentially private external input in error reports.
        f.write_str("invalid domain identifier")
    }
}
impl std::error::Error for InvalidId {}

fn profile_id_valid(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-')
}
fn opaque_id_valid(value: &str) -> bool {
    !value.trim().is_empty()
}

macro_rules! identifier {
    ($name:ident, $validate:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, InvalidId> {
                let value = value.into();
                if $validate(&value) {
                    Ok(Self(value))
                } else {
                    Err(InvalidId)
                }
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}
identifier!(ProfileId, profile_id_valid);
identifier!(SessionId, opaque_id_valid);
identifier!(DocumentId, opaque_id_valid);
identifier!(OperationId, opaque_id_valid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Procedure {
    Huellas,
    Regreso,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageKind {
    Province,
    Procedure,
    Auth,
    AuthWait,
    Applicant,
    ApplicantNoSlots,
    RequestMenu,
    RequestNoSlots,
    RequestNoSlotsRestart,
    PostContactNoSlots,
    Office,
    Contact,
    Booking,
    Captcha,
    Expired,
    ServerError,
    Blocked,
    RateLimited,
    Loading,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicantNoSlotsMode {
    #[default]
    Wait,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoSlotsMode {
    #[default]
    Salir,
    Reload,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SearchOptions {
    pub applicant_no_slots_mode: ApplicantNoSlotsMode,
    pub no_slots_mode: NoSlotsMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestMethod {
    Get,
    Post,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub document: DocumentId,
    pub kind: PageKind,
    #[serde(default)]
    pub method: RequestMethod,
    #[serde(default)]
    pub operation_code: bool,
    #[serde(default)]
    pub variant: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ownership {
    Automatic,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Recovery,
    Cookie,
    Province,
    Procedure,
    Auth,
    Applicant,
    Request,
    Office,
    Contact,
    ApplicantReload,
    RequestReload,
    NoSlotsRestart,
    NoSlotsSalir,
    PostContactRestart,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingSubmission {
    pub document: DocumentId,
    pub action: ActionKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MachineState {
    pub profile: ProfileId,
    pub session: SessionId,
    pub ownership: Ownership,
    pub current: Option<Observation>,
    pub pending: Option<PendingSubmission>,
    #[serde(default)]
    pub options: SearchOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentGuard {
    pub session: SessionId,
    pub epoch: u64,
    pub document: DocumentId,
    pub expected_page: PageKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    BrowserAction {
        guard: DocumentGuard,
        action: ActionKind,
    },
    WaitUntil {
        monotonic_millis: u64,
    },
    Handoff {
        document: DocumentId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub next_state: MachineState,
    pub effects: Vec<Effect>,
}

/// A transport acknowledgement never confirms navigation or successful submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchOutcome {
    Dispatched,
    NotDispatched,
    Unknown,
}
