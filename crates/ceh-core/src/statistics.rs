//! Public statistical facts, independent of diagnostics and persistence.
use crate::{
    booking::{PublicBookingDetails, normalize_booking_details},
    control::ControlMode,
    model::*,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CounterKind {
    Check,
    Office,
    ServerError,
    RateLimited,
    Blocked,
    Booking,
    Expired,
    Pass,
    AutomationAction,
    ProcessedPage,
}
impl CounterKind {
    fn error(self) -> bool {
        matches!(
            self,
            Self::ServerError | Self::RateLimited | Self::Blocked | Self::Expired
        )
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct EventScope {
    pub profile: ProfileId,
    pub session: SessionId,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
pub enum EvidenceId {
    Document(DocumentId),
    Operation(OperationId),
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct EventKey {
    pub scope: EventScope,
    pub kind: CounterKind,
    pub identity: EvidenceId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PublicEvent {
    pub key: EventKey,
    pub at: i64,
    pub counted: bool,
    pub offices: Vec<String>,
    pub booking_details: PublicBookingDetails,
}
#[derive(Debug, Clone)]
pub struct CounterObservation {
    pub key: EventKey,
    pub at: i64,
    pub offices: Vec<String>,
    pub mode: ControlMode,
    pub automated: bool,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Counters {
    pub checks: u64,
    pub offices_seen: u64,
    pub server_errors: u64,
    pub rate_limited: u64,
    pub blocked: u64,
    pub booking_screens: u64,
    pub expired_sessions: u64,
    pub flow_passes: u64,
    pub actions_performed: u64,
    pub pages_processed: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatisticsError {
    InvalidIdentity,
    CounterOverflow,
}
impl Counters {
    fn increment(&mut self, kind: CounterKind) -> Result<(), StatisticsError> {
        let field = match kind {
            CounterKind::Check => &mut self.checks,
            CounterKind::Office => &mut self.offices_seen,
            CounterKind::ServerError => &mut self.server_errors,
            CounterKind::RateLimited => &mut self.rate_limited,
            CounterKind::Blocked => &mut self.blocked,
            CounterKind::Booking => &mut self.booking_screens,
            CounterKind::Expired => &mut self.expired_sessions,
            CounterKind::Pass => &mut self.flow_passes,
            CounterKind::AutomationAction => &mut self.actions_performed,
            CounterKind::ProcessedPage => &mut self.pages_processed,
        };
        *field = field
            .checked_add(1)
            .ok_or(StatisticsError::CounterOverflow)?;
        Ok(())
    }
}
#[derive(Default)]
pub struct StatisticsLedger {
    events: Vec<PublicEvent>,
    index: HashMap<EventKey, usize>,
    totals: HashMap<ProfileId, Counters>,
    error_streak: HashMap<EventScope, CounterKind>,
    pass_seen: HashSet<EventScope>,
}
impl StatisticsLedger {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn events(&self) -> &[PublicEvent] {
        &self.events
    }
    pub fn totals(&self, profile: &ProfileId) -> Counters {
        self.totals.get(profile).copied().unwrap_or_default()
    }
    pub fn known_offices(&self, profile: &ProfileId) -> Vec<String> {
        let mut labels: Vec<_> = self
            .events
            .iter()
            .filter(|e| &e.key.scope.profile == profile && e.key.kind == CounterKind::Office)
            .flat_map(|e| e.offices.clone())
            .collect();
        labels.sort();
        labels.dedup();
        labels
    }
    /// Called on recognized page events even if no metric is incremented.
    pub fn observe_page(&mut self, scope: &EventScope, stage: PageKind) {
        if !matches!(
            stage,
            PageKind::ServerError
                | PageKind::RateLimited
                | PageKind::Blocked
                | PageKind::Expired
                | PageKind::Loading
                | PageKind::Unknown
        ) {
            self.error_streak.remove(scope);
        }
    }
    pub fn record(&mut self, input: CounterObservation) -> Result<bool, StatisticsError> {
        let kind = input.key.kind;
        let operation = matches!(kind, CounterKind::Check | CounterKind::AutomationAction);
        if operation != matches!(input.key.identity, EvidenceId::Operation(_)) {
            return Err(StatisticsError::InvalidIdentity);
        }
        if matches!(
            kind,
            CounterKind::AutomationAction | CounterKind::ProcessedPage
        ) && (!input.automated || input.mode != ControlMode::Automatic)
        {
            return Ok(false);
        }
        // StatisticsSession updates its streak even when storage deduplicates a row.
        let counted = if kind.error() {
            let changed = self.error_streak.get(&input.key.scope) != Some(&kind);
            self.error_streak.insert(input.key.scope.clone(), kind);
            changed
        } else if kind == CounterKind::Pass {
            !self.pass_seen.insert(input.key.scope.clone())
        } else {
            true
        };
        if self.index.contains_key(&input.key) {
            return Ok(false);
        }
        let mut offices = vec![];
        if kind == CounterKind::Office {
            for label in input.offices.into_iter().filter(|s| !s.is_empty()) {
                let bounded = label.chars().take(300).collect::<String>();
                if !offices.contains(&bounded) {
                    offices.push(bounded);
                }
            }
        }
        let mut totals = self.totals(&input.key.scope.profile);
        if counted && (kind != CounterKind::Office || !offices.is_empty()) {
            totals.increment(kind)?;
        }
        self.totals.insert(input.key.scope.profile.clone(), totals);
        self.index.insert(input.key.clone(), self.events.len());
        self.events.push(PublicEvent {
            key: input.key,
            at: input.at,
            counted,
            offices,
            booking_details: PublicBookingDetails::default(),
        });
        Ok(true)
    }
    pub fn enrich_booking(
        &mut self,
        scope: &EventScope,
        document: &DocumentId,
        raw: &Value,
    ) -> bool {
        let details = normalize_booking_details(raw);
        if details.office.is_none() && details.offers.is_empty() {
            return false;
        }
        let key = EventKey {
            scope: scope.clone(),
            kind: CounterKind::Booking,
            identity: EvidenceId::Document(document.clone()),
        };
        if let Some(&index) = self.index.get(&key) {
            self.events[index].booking_details = details;
            true
        } else {
            false
        }
    }
}
