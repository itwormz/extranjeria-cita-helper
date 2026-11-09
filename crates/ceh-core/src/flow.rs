//! Deterministic action phases. Effects are intentions, never transition evidence.
use crate::control::{Authority, ControlMode, ControlSnapshot};
use crate::model::*;
use crate::recovery::{
    RecoveryAction, RecoveryEndpoint, RecoveryFacts, RecoveryPlan, plan_recovery,
};
use crate::timings::{AssistTimings, RetryKind, deadline_millis};
use serde::{Deserialize, Serialize};

/// Each distinct HTTP 429 on the applicant form lengthens its refresh interval by this much.
const RATE_LIMIT_RELOAD_PENALTY_SECONDS: f64 = 30.0;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum FlowEffect {
    Silence,
    AcceptCookies {
        guard: DocumentGuard,
    },
    SelectCertificate {
        guard: DocumentGuard,
    },
    SelectClaveMethod {
        guard: DocumentGuard,
        method: ClaveMethod,
    },
    FetchApplicant {
        guard: DocumentGuard,
    },
    Recover {
        guard: DocumentGuard,
        action: RecoveryAction,
    },
    EnterAuthentication {
        guard: DocumentGuard,
        mode: AuthenticationMode,
    },
    Prepare {
        guard: DocumentGuard,
        action: ActionKind,
    },
    Activate {
        guard: DocumentGuard,
        action: ActionKind,
    },
    Reload {
        guard: DocumentGuard,
    },
    WaitUntil(u64),
    ReobserveAt(u64),
    Hold,
    Handoff,
    Alert,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowError {
    ClockOverflow,
    ControlDenied,
    ClockReversed,
    InvalidDelay,
    StaleCompletion,
    UnexpectedCompletion,
    /// No page has been observed yet.
    NoCurrent,
}
#[derive(Clone)]
enum Phase {
    Preparing(ActionKind),
    Prepared(ActionKind),
    Waiting(u64),
    UnknownReread {
        deadline: u64,
        variant: String,
    },
    Loading {
        deadline: u64,
    },
    LoadingCooldown {
        deadline: u64,
    },
    Recovering {
        deadline: u64,
        plan: RecoveryPlan,
        attempt: usize,
    },
    Dispatched,
    Held,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreparationOutcome {
    Completed,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthenticationMode {
    #[default]
    Certificate,
    Clave,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaveMethod {
    #[default]
    Movil,
    Permanente,
}

/// Visible facts supplied by the browser adapter, without personal form values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PageFacts {
    pub office_count: usize,
    pub appointment_ready_at: Option<u64>,
    pub confirm_appointment_round: bool,
    pub cookie_accept_available: bool,
    pub authentication: AuthenticationMode,
    pub recovery: RecoveryFacts,
    pub certificate_method_available: bool,
    pub clave_method: ClaveMethod,
    pub clave_method_available: bool,
    pub clave_requires_attention: bool,
    pub clave_input_started: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DispatchRecord {
    pub guard: DocumentGuard,
    pub action: ActionKind,
    pub outcome: Option<DispatchOutcome>,
}

pub struct FlowEngine {
    session: SessionId,
    authority: Authority,
    paused_at: Option<u64>,
    options: SearchOptions,
    current: Option<Observation>,
    phase: Option<Phase>,
    manual: bool,
    auth_attention: bool,
    stage_attention: Option<(DocumentId, PageKind)>,
    auth_acknowledged: Vec<DocumentId>,
    certificate_pending: Option<DocumentGuard>,
    certificate_deadline: Option<u64>,
    certificate_alerted: bool,
    round_saw_banner: bool,
    last_tick: Option<u64>,
    attempted: Vec<(DocumentId, ActionKind)>,
    dispatches: Vec<DispatchRecord>,
    reload_delay: f64,
    applicant_reload_penalty: f64,
    last_rate_limit_document: Option<DocumentId>,
    fresh_pass_document: Option<DocumentId>,
    recovery_jitter: f64,
    timings: AssistTimings,
    recovery_attempts: Vec<((PageKind, bool), usize)>,
}
impl FlowEngine {
    pub fn new(session: SessionId, options: SearchOptions) -> Self {
        Self {
            authority: Authority::new(session.clone()),
            paused_at: None,
            session,
            options,
            current: None,
            phase: None,
            manual: false,
            auth_attention: false,
            stage_attention: None,
            auth_acknowledged: vec![],
            certificate_pending: None,
            certificate_deadline: None,
            certificate_alerted: false,
            round_saw_banner: false,
            last_tick: None,
            attempted: vec![],
            dispatches: vec![],
            reload_delay: AssistTimings::default().reload_seconds()[0],
            applicant_reload_penalty: 0.0,
            last_rate_limit_document: None,
            fresh_pass_document: None,
            timings: AssistTimings::default(),
            recovery_jitter: AssistTimings::default().recovery_jitter_seconds()[0],
            recovery_attempts: vec![],
        }
    }
    pub fn with_timings(
        session: SessionId,
        options: SearchOptions,
        timings: AssistTimings,
        reload_sample: f64,
    ) -> Result<Self, FlowError> {
        let [min, max] = timings.reload_seconds();
        if !reload_sample.is_finite() || reload_sample < min || reload_sample > max {
            return Err(FlowError::InvalidDelay);
        }
        let mut engine = Self::new(session, options);
        engine.reload_delay = reload_sample;
        engine.recovery_jitter = timings.recovery_jitter_seconds()[0];
        engine.timings = timings;
        Ok(engine)
    }
    /// Replace the profile timings of a running search (Python applies saved timings during the run).
    /// The sample must lie in the new reload range; on error nothing changes.
    pub fn apply_timings(&mut self, timings: AssistTimings, reload_sample: f64) -> Result<(), FlowError> {
        let [min, max] = timings.reload_seconds();
        if !reload_sample.is_finite() || reload_sample < min || reload_sample > max {
            return Err(FlowError::InvalidDelay);
        }
        self.reload_delay = reload_sample;
        self.recovery_jitter = timings.recovery_jitter_seconds()[0];
        self.timings = timings;
        Ok(())
    }
    /// Supply a sampled delay explicitly; the deterministic core never samples randomness.
    pub fn with_recovery_jitter(mut self, sample: f64) -> Result<Self, FlowError> {
        self.timings
            .recovery_delay(RetryKind::ServerRetry, 0, sample)
            .map_err(|_| FlowError::InvalidDelay)?;
        self.recovery_jitter = sample;
        Ok(self)
    }
    pub fn dispatch_record(
        &self,
        guard: &DocumentGuard,
        action: ActionKind,
    ) -> Option<&DispatchRecord> {
        self.dispatches
            .iter()
            .find(|r| r.guard == *guard && r.action == action)
    }
    pub fn acknowledge_dispatch(
        &mut self,
        guard: &DocumentGuard,
        action: ActionKind,
        outcome: DispatchOutcome,
    ) -> Result<(), FlowError> {
        if self.authority.mode() != ControlMode::Automatic
            || guard.session != self.session
            || guard.epoch != self.authority.epoch()
        {
            return Err(FlowError::StaleCompletion);
        }
        let record = self
            .dispatches
            .iter_mut()
            .find(|r| r.guard == *guard && r.action == action)
            .ok_or(FlowError::UnexpectedCompletion)?;
        if record.outcome.is_some_and(|previous| previous != outcome) {
            return Err(FlowError::UnexpectedCompletion);
        }
        record.outcome = Some(outcome);
        if outcome == DispatchOutcome::NotDispatched
            && self.certificate_pending.as_ref() == Some(guard)
        {
            self.clear_certificate_wait();
        }
        Ok(())
    }
    /// Trusted worker proof that a pre-action was never published after its
    /// epoch was revoked. This records cancellation, never authorizes a write.
    /// Unknown/accepted publication must use the normal receipt path instead.
    pub fn cancel_unpublished_pre_action(
        &mut self,
        guard: &DocumentGuard,
        action: ActionKind,
    ) -> Result<(), FlowError> {
        let supported = action == ActionKind::Cookie
            || (action == ActionKind::Auth && guard.expected_page == PageKind::AuthWait);
        let current = self.current.as_ref().ok_or(FlowError::StaleCompletion)?;
        if !supported
            || guard.session != self.session
            || guard.epoch >= self.authority.epoch()
            || self.authority.mode() == ControlMode::Manual
            || current.document != guard.document
            || current.kind != guard.expected_page
        {
            return Err(FlowError::StaleCompletion);
        }
        let record = self
            .dispatches
            .iter_mut()
            .find(|r| r.guard == *guard && r.action == action)
            .ok_or(FlowError::UnexpectedCompletion)?;
        if record.outcome == Some(DispatchOutcome::NotDispatched) {
            return Ok(()); // A late duplicate must not erase a newer intent.
        }
        if record.outcome.is_some()
            || (self.certificate_pending.as_ref() == Some(guard)
                && self.certificate_deadline.is_some())
        {
            return Err(FlowError::UnexpectedCompletion);
        }
        record.outcome = Some(DispatchOutcome::NotDispatched);
        self.attempted
            .retain(|(document, kind)| document != &guard.document || *kind != action);
        if self.certificate_pending.as_ref() == Some(guard) {
            self.clear_certificate_wait();
        }
        Ok(())
    }
    fn clear_certificate_wait(&mut self) {
        self.certificate_pending = None;
        self.certificate_deadline = None;
        self.certificate_alerted = false;
    }
    /// Trusted controller fact after the worker publishes the certificate click.
    /// `now` is controller processing time, not an untrusted browser timestamp.
    /// Recording an already published command after pause is allowed; this never
    /// grants authority for another write or emits a notification while paused.
    pub fn certificate_published(
        &mut self,
        guard: &DocumentGuard,
        now: u64,
    ) -> Result<(), FlowError> {
        let current = self.current.as_ref().ok_or(FlowError::StaleCompletion)?;
        if self.authority.mode() == ControlMode::Manual
            || self.certificate_pending.as_ref() != Some(guard)
            || current.document != guard.document
            || current.kind != PageKind::AuthWait
        {
            return Err(FlowError::StaleCompletion);
        }
        if self.last_tick.is_some_and(|last| now < last) {
            return Err(FlowError::ClockReversed);
        }
        if self.certificate_deadline.is_none() {
            let start = if self.authority.mode() == ControlMode::Paused {
                self.paused_at.unwrap_or(now)
            } else {
                now
            };
            self.certificate_deadline =
                Some(start.checked_add(5000).ok_or(FlowError::ClockOverflow)?);
        }
        self.last_tick = Some(now);
        Ok(())
    }
    /// Start a new pass from the entry page in the same browser session (Python "Try now", 0.3.5).
    /// Only while the engine owns the browser; submitted actions are never forgotten, so the old
    /// document cannot be filled or submitted again. Until a different page is observed a second call
    /// returns nothing (exactly one new pass).
    pub fn fresh_pass(&mut self, _now: u64) -> Result<Vec<FlowEffect>, FlowError> {
        if self.authority.mode() != ControlMode::Automatic || self.manual {
            return Err(FlowError::ControlDenied);
        }
        let current = self.current.clone().ok_or(FlowError::NoCurrent)?;
        if self.fresh_pass_document.as_ref() == Some(&current.document) {
            return Ok(Vec::new());
        }
        self.fresh_pass_document = Some(current.document.clone());
        self.round_saw_banner = false;
        self.phase = Some(Phase::Dispatched);
        let guard = self.guard(&current);
        Ok(vec![FlowEffect::Recover { guard, action: RecoveryAction::GetEntry }])
    }
    /// Seconds added to the applicant-form refresh interval after HTTP 429 responses (Python 0.3.1+).
    pub fn applicant_reload_penalty(&self) -> f64 {
        self.applicant_reload_penalty
    }
    pub fn control(&self) -> ControlSnapshot {
        self.authority.snapshot()
    }
    pub fn pause(&mut self, now: u64) -> Result<ControlSnapshot, FlowError> {
        if self.last_tick.is_some_and(|last| now < last) {
            return Err(FlowError::ClockReversed);
        }
        let snapshot = self
            .authority
            .pause()
            .map_err(|_| FlowError::ControlDenied)?;
        if self.paused_at.is_none() {
            self.paused_at = Some(now);
        }
        self.last_tick = Some(now);
        Ok(snapshot)
    }
    pub fn resume(&mut self, now: u64) -> Result<ControlSnapshot, FlowError> {
        if self.last_tick.is_some_and(|last| now < last) {
            return Err(FlowError::ClockReversed);
        }
        if self.authority.mode() == ControlMode::Manual {
            return Err(FlowError::ControlDenied);
        }
        if self.authority.mode() == ControlMode::Automatic {
            self.last_tick = Some(now);
            return Ok(self.control());
        }
        let elapsed = self.paused_at.map_or(0, |start| now - start);
        let shifted = match self.phase.clone() {
            Some(Phase::Preparing(_) | Phase::Prepared(_)) => None,
            Some(Phase::Waiting(deadline)) => Some(Phase::Waiting(
                deadline
                    .checked_add(elapsed)
                    .ok_or(FlowError::ClockOverflow)?,
            )),
            Some(Phase::Recovering {
                deadline,
                plan,
                attempt,
            }) => Some(Phase::Recovering {
                deadline: deadline
                    .checked_add(elapsed)
                    .ok_or(FlowError::ClockOverflow)?,
                plan,
                attempt,
            }),
            Some(Phase::UnknownReread { deadline, variant }) => Some(Phase::UnknownReread {
                deadline: deadline
                    .checked_add(elapsed)
                    .ok_or(FlowError::ClockOverflow)?,
                variant,
            }),
            Some(Phase::Loading { deadline }) => Some(Phase::Loading {
                deadline: deadline
                    .checked_add(elapsed)
                    .ok_or(FlowError::ClockOverflow)?,
            }),
            Some(Phase::LoadingCooldown { deadline }) => Some(Phase::LoadingCooldown {
                deadline: deadline
                    .checked_add(elapsed)
                    .ok_or(FlowError::ClockOverflow)?,
            }),
            other => other,
        };
        let shifted_certificate = self
            .certificate_deadline
            .map(|deadline| {
                deadline
                    .checked_add(elapsed)
                    .ok_or(FlowError::ClockOverflow)
            })
            .transpose()?;
        let snapshot = self
            .authority
            .resume()
            .map_err(|_| FlowError::ControlDenied)?;
        self.paused_at = None;
        self.phase = shifted;
        self.certificate_deadline = shifted_certificate;
        self.last_tick = Some(now);
        Ok(snapshot)
    }
    pub fn complete_preparation(
        &mut self,
        guard: &DocumentGuard,
        action: ActionKind,
        outcome: PreparationOutcome,
    ) -> Result<(), FlowError> {
        let current = self
            .current
            .as_ref()
            .ok_or(FlowError::UnexpectedCompletion)?;
        if self.authority.mode() != ControlMode::Automatic || self.guard(current) != *guard {
            return Err(FlowError::StaleCompletion);
        }
        if !matches!(self.phase,Some(Phase::Preparing(expected)) if expected==action) {
            return Err(FlowError::UnexpectedCompletion);
        }
        self.phase = Some(if outcome == PreparationOutcome::Completed {
            Phase::Prepared(action)
        } else {
            Phase::Dispatched
        });
        Ok(())
    }
    fn guard(&self, observed: &Observation) -> DocumentGuard {
        DocumentGuard {
            session: self.session.clone(),
            epoch: self.authority.epoch(),
            document: observed.document.clone(),
            expected_page: observed.kind,
        }
    }
    fn applicant_refresh_safe(&self, observed: &Observation, facts: &PageFacts) -> bool {
        observed.method == RequestMethod::Get
            || (observed.method == RequestMethod::Post
                && facts.recovery.official_site
                && facts.recovery.endpoint == RecoveryEndpoint::ApplicantEntry
                && !matches!(
                    facts.recovery.pending_action.or(facts.recovery.last_action),
                    Some(
                        ActionKind::Applicant
                            | ActionKind::Contact
                            | ActionKind::Request
                            | ActionKind::Office
                    )
                ))
    }
    fn wait_action(&self, observed: &Observation, facts: &PageFacts) -> (ActionKind, FlowEffect) {
        let guard = self.guard(observed);
        match observed.kind {
            PageKind::ApplicantNoSlots => (
                ActionKind::ApplicantReload,
                if self.applicant_refresh_safe(observed, facts) {
                    if observed.method == RequestMethod::Post {
                        FlowEffect::FetchApplicant { guard }
                    } else {
                        FlowEffect::Reload { guard }
                    }
                } else {
                    FlowEffect::Hold
                },
            ),
            PageKind::RequestNoSlotsRestart => (
                ActionKind::NoSlotsRestart,
                if facts.recovery.own_control {
                    FlowEffect::Activate {
                        guard,
                        action: ActionKind::NoSlotsRestart,
                    }
                } else {
                    FlowEffect::Hold
                },
            ),
            PageKind::RequestNoSlots => {
                let safe_reload = observed.method == RequestMethod::Get
                    || (observed.method == RequestMethod::Post
                        && facts.recovery.official_site
                        && facts.recovery.endpoint == RecoveryEndpoint::Availability
                        && facts.recovery.pending_action.or(facts.recovery.last_action)
                            == Some(ActionKind::Request));
                (
                    ActionKind::RequestReload,
                    if self.options.no_slots_mode == NoSlotsMode::Salir {
                        if facts.recovery.own_control {
                            FlowEffect::Activate {
                                guard,
                                action: ActionKind::NoSlotsSalir,
                            }
                        } else if facts.appointment_ready_at.is_some() && facts.recovery.official_site {
                            FlowEffect::Recover { guard, action: RecoveryAction::GetEntry }
                        } else {
                            FlowEffect::Hold
                        }
                    } else if facts.appointment_ready_at.is_some() && facts.recovery.official_site {
                        FlowEffect::Recover { guard, action: RecoveryAction::GetEntry }
                    } else if safe_reload {
                        FlowEffect::Reload { guard }
                    } else {
                        FlowEffect::Hold
                    },
                )
            }
            _ => (ActionKind::RequestReload, FlowEffect::Hold),
        }
    }
    pub fn observe(
        &mut self,
        observed: Observation,
        now: u64,
    ) -> Result<Vec<FlowEffect>, FlowError> {
        self.observe_with_facts(observed, PageFacts::default(), now)
    }
    pub fn observe_with_facts(
        &mut self,
        observed: Observation,
        facts: PageFacts,
        now: u64,
    ) -> Result<Vec<FlowEffect>, FlowError> {
        let kind = observed.kind;
        let stage_left = self.authority.mode() == ControlMode::Automatic
            && !self.manual
            && self
                .stage_attention
                .as_ref()
                .is_some_and(|(document, stage)| {
                    document != &observed.document
                        || *stage != kind
                        || (kind == PageKind::Office && facts.office_count == 1)
                });
        let certificate_loading = kind == PageKind::Loading && self.certificate_deadline.is_some();
        let silence = (self.auth_attention && kind != PageKind::AuthWait && !certificate_loading) || stage_left;
        let stage = (observed.document.clone(), kind);
        let mut effects = self.advance(observed, facts, now)?;
        if stage_left {
            self.stage_attention = None;
        }
        if matches!(kind, PageKind::Office | PageKind::Captcha)
            && effects.contains(&FlowEffect::Alert)
        {
            self.stage_attention = Some(stage);
        }
        if !matches!(kind, PageKind::AuthWait | PageKind::Loading) {
            self.clear_certificate_wait();
        }
        if silence {
            self.auth_attention = false;
            effects.insert(0, FlowEffect::Silence);
        }
        for effect in &effects {
            let issued = match effect {
                FlowEffect::Activate { guard, action } => Some((guard, *action)),
                FlowEffect::AcceptCookies { guard } => Some((guard, ActionKind::Cookie)),
                FlowEffect::Recover { guard, .. } => Some((guard, ActionKind::Recovery)),
                FlowEffect::EnterAuthentication { guard, .. }
                | FlowEffect::SelectClaveMethod { guard, .. }
                | FlowEffect::SelectCertificate { guard } => Some((guard, ActionKind::Auth)),
                FlowEffect::Reload { guard } | FlowEffect::FetchApplicant { guard } => Some((
                    guard,
                    if guard.expected_page == PageKind::ApplicantNoSlots {
                        ActionKind::ApplicantReload
                    } else {
                        ActionKind::RequestReload
                    },
                )),
                _ => None,
            };
            if let Some((guard, action)) = issued
                && !self
                    .dispatches
                    .iter()
                    .any(|r| r.guard == *guard && r.action == action)
            {
                self.dispatches.push(DispatchRecord {
                    guard: guard.clone(),
                    action,
                    outcome: None,
                });
            }
        }
        Ok(effects)
    }
    fn advance(
        &mut self,
        observed: Observation,
        facts: PageFacts,
        now: u64,
    ) -> Result<Vec<FlowEffect>, FlowError> {
        if self.last_tick.is_some_and(|previous| now < previous) {
            return Err(FlowError::ClockReversed);
        }
        self.last_tick = Some(now);
        if self.authority.mode() == ControlMode::Paused {
            return Ok(vec![]);
        }
        if self.manual {
            return Ok(vec![]);
        }
        if observed.kind == PageKind::Booking {
            self.authority
                .handoff()
                .map_err(|_| FlowError::ControlDenied)?;
            self.manual = true;
            self.current = Some(observed);
            self.phase = None;
            return Ok(vec![FlowEffect::Handoff, FlowEffect::Alert]);
        }
        if matches!(
            observed.kind,
            PageKind::Applicant
                | PageKind::ApplicantNoSlots
                | PageKind::Office
                | PageKind::Contact
                | PageKind::RequestNoSlots
                | PageKind::RequestNoSlotsRestart
                | PageKind::PostContactNoSlots
        ) {
            self.recovery_attempts.clear();
        }
        let same = self
            .current
            .as_ref()
            .is_some_and(|old| old.document == observed.document && old.kind == observed.kind);
        let canceled_continuation = !same
            && matches!(
                self.phase,
                Some(
                    Phase::Preparing(_)
                        | Phase::Prepared(_)
                        | Phase::Waiting(_)
                        | Phase::Recovering { .. }
                        | Phase::UnknownReread { .. }
                        | Phase::Loading { .. }
                        | Phase::LoadingCooldown { .. }
                )
            );
        if !same {
            self.phase = None;
        }
        if self.fresh_pass_document.as_ref().is_some_and(|document| document != &observed.document) {
            self.fresh_pass_document = None;
        }
        self.current = Some(observed.clone());
        if observed.kind == PageKind::RateLimited
            && self.timings.adaptive_reload_on_429()
            && facts.recovery.endpoint == RecoveryEndpoint::ApplicantEntry
            && observed.method == RequestMethod::Get
            && self.last_rate_limit_document.as_ref() != Some(&observed.document)
        {
            self.last_rate_limit_document = Some(observed.document.clone());
            self.applicant_reload_penalty += RATE_LIMIT_RELOAD_PENALTY_SECONDS;
        }
        if observed.kind == PageKind::Loading
            && let Some(deadline) = self.certificate_deadline
        {
            let poll = FlowEffect::ReobserveAt(now.checked_add(500).ok_or(FlowError::ClockOverflow)?);
            if now >= deadline && !self.certificate_alerted {
                self.certificate_alerted = true;
                self.auth_attention = true;
                return Ok(vec![FlowEffect::Alert, poll]);
            }
            return Ok(vec![poll]);
        }
        if canceled_continuation {
            return Ok(vec![]);
        }
        let guard = self.guard(&observed);
        if observed.kind == PageKind::Province {
            self.round_saw_banner = false;
        } else if observed.kind == PageKind::ApplicantNoSlots && facts.confirm_appointment_round {
            self.round_saw_banner = true;
        }
        // A banner is a bounded pre-action, never permission to touch a manual
        // page or bypass recovery. Always reobserve after its one attempted click.
        let cookie_page = matches!(
            observed.kind,
            PageKind::Province
                | PageKind::Procedure
                | PageKind::Auth
                | PageKind::Applicant
                | PageKind::ApplicantNoSlots
                | PageKind::Contact
                | PageKind::RequestMenu
                | PageKind::RequestNoSlots
                | PageKind::RequestNoSlotsRestart
                | PageKind::PostContactNoSlots
        );
        if facts.cookie_accept_available && cookie_page {
            let key = (observed.document.clone(), ActionKind::Cookie);
            if !self.attempted.contains(&key) {
                self.attempted.push(key);
                return Ok(vec![FlowEffect::AcceptCookies { guard }]);
            }
            return Ok(vec![FlowEffect::ReobserveAt(
                now.checked_add(500).ok_or(FlowError::ClockOverflow)?,
            )]);
        }
        if matches!(
            observed.kind,
            PageKind::Expired | PageKind::ServerError | PageKind::Blocked | PageKind::RateLimited
        ) {
            let key = (observed.kind, facts.recovery.own_control);
            let attempt = self
                .recovery_attempts
                .iter()
                .find(|(k, _)| *k == key)
                .map_or(0, |(_, n)| *n);
            if matches!(self.phase, Some(Phase::Held)) {
                return Ok(vec![]);
            }
            let fresh = plan_recovery(
                &observed,
                &facts.recovery,
                &self.timings,
                attempt,
                self.recovery_jitter,
            )
            .map_err(|_| FlowError::InvalidDelay)?;
            if let Some(Phase::Recovering {
                deadline,
                plan,
                attempt: scheduled_attempt,
            }) = &self.phase
            {
                if fresh != *plan || attempt != *scheduled_attempt {
                    self.phase = Some(Phase::Held);
                    return Ok(vec![FlowEffect::Hold]);
                }
                if now < *deadline {
                    return Ok(vec![]);
                }
                if let Some((_, n)) = self.recovery_attempts.iter_mut().find(|(k, _)| *k == key) {
                    *n = n.saturating_add(1);
                } else {
                    self.recovery_attempts.push((key, 1));
                }
                let action = plan.after;
                self.phase = Some(
                    if matches!(
                        action,
                        RecoveryAction::Restart { .. } | RecoveryAction::Hold
                    ) {
                        Phase::Held
                    } else {
                        Phase::Dispatched
                    },
                );
                return Ok(if action == RecoveryAction::Hold {
                    vec![FlowEffect::Hold]
                } else {
                    vec![FlowEffect::Recover { guard, action }]
                });
            }
            let deadline =
                deadline_millis(now, fresh.delay_seconds).map_err(|_| FlowError::ClockOverflow)?;
            self.phase = Some(Phase::Recovering {
                deadline,
                plan: fresh,
                attempt,
            });
            return Ok(vec![FlowEffect::WaitUntil(deadline)]);
        }

        if observed.kind == PageKind::AuthWait {
            let poll =
                FlowEffect::ReobserveAt(now.checked_add(500).ok_or(FlowError::ClockOverflow)?);
            if facts.authentication == AuthenticationMode::Certificate {
                let key = (observed.document.clone(), ActionKind::Auth);
                if facts.certificate_method_available
                    && self.certificate_pending.is_none()
                    && !self.attempted.contains(&key)
                {
                    self.attempted.push(key);
                    self.certificate_pending = Some(guard.clone());
                    return Ok(vec![FlowEffect::SelectCertificate { guard }]);
                }
                if self
                    .certificate_deadline
                    .is_some_and(|deadline| now >= deadline)
                    && !self.certificate_alerted
                {
                    self.certificate_alerted = true;
                    self.auth_attention = true;
                    return Ok(vec![FlowEffect::Alert, poll]);
                }
            }
            if facts.authentication == AuthenticationMode::Clave {
                self.clear_certificate_wait();
                let key = (observed.document.clone(), ActionKind::Auth);
                if facts.clave_method_available && !self.attempted.contains(&key) {
                    self.attempted.push(key);
                    return Ok(vec![FlowEffect::SelectClaveMethod {
                        guard,
                        method: facts.clave_method,
                    }]);
                }
                if facts.clave_input_started && !self.auth_acknowledged.contains(&observed.document)
                {
                    self.auth_acknowledged.push(observed.document.clone());
                }
                let call = facts.clave_requires_attention
                    && !self.auth_acknowledged.contains(&observed.document);
                if call && !self.auth_attention {
                    self.auth_attention = true;
                    return Ok(vec![FlowEffect::Alert, poll]);
                }
                if !call && self.auth_attention {
                    self.auth_attention = false;
                    return Ok(vec![FlowEffect::Silence, poll]);
                }
            }
            return Ok(vec![poll]);
        }
        if observed.kind == PageKind::Office
            && facts.office_count == 1
            && self
                .stage_attention
                .as_ref()
                .is_some_and(|(document, kind)| {
                    document == &observed.document && *kind == PageKind::Office
                })
        {
            self.phase = None;
        }
        if observed.kind == PageKind::Office && facts.office_count != 1 {
            if matches!(self.phase, Some(Phase::Held)) {
                return Ok(vec![]);
            }
            self.phase = Some(Phase::Held);
            return Ok(vec![if facts.office_count > 1 {
                FlowEffect::Alert
            } else {
                FlowEffect::Hold
            }]);
        }
        if matches!(observed.kind, PageKind::Applicant | PageKind::RequestMenu)
            && matches!(self.phase, None | Some(Phase::Prepared(ActionKind::Applicant)))
            && let Some(deadline) = facts.appointment_ready_at.filter(|deadline| now < *deadline)
        {
            return Ok(vec![FlowEffect::WaitUntil(deadline)]);
        }
        match self.phase.clone() {
            Some(Phase::UnknownReread { deadline, variant }) => {
                if variant != observed.variant {
                    let deadline = now.checked_add(500).ok_or(FlowError::ClockOverflow)?;
                    self.phase = Some(Phase::UnknownReread {
                        deadline,
                        variant: observed.variant,
                    });
                    return Ok(vec![FlowEffect::ReobserveAt(deadline)]);
                }
                if now < deadline {
                    return Ok(vec![]);
                }
                self.phase = Some(Phase::Held);
                return Ok(vec![FlowEffect::Hold]);
            }
            Some(Phase::Loading { deadline }) => {
                if now < deadline {
                    return Ok(vec![FlowEffect::ReobserveAt(
                        now.saturating_add(500).min(deadline),
                    )]);
                }
                let delay = self
                    .timings
                    .recovery_delay(RetryKind::ServerRetry, 0, self.recovery_jitter)
                    .map_err(|_| FlowError::InvalidDelay)?;
                let deadline = deadline_millis(now, delay).map_err(|_| FlowError::ClockOverflow)?;
                self.phase = Some(Phase::LoadingCooldown { deadline });
                return Ok(vec![FlowEffect::WaitUntil(deadline)]);
            }
            Some(Phase::LoadingCooldown { deadline }) => {
                if now < deadline {
                    return Ok(vec![]);
                }
                self.phase = Some(Phase::Held);
                return Ok(vec![FlowEffect::Recover {
                    guard,
                    action: RecoveryAction::Restart {
                        clear_profile: false,
                        reopen: false,
                    },
                }]);
            }

            Some(Phase::Recovering { .. }) => {
                unreachable!("recovery phase belongs to the same recognized error page")
            }
            Some(Phase::Preparing(_)) => return Ok(vec![]),
            Some(Phase::Prepared(action)) => {
                self.attempted.push((observed.document.clone(), action));
                self.phase = Some(Phase::Dispatched);
                return Ok(vec![FlowEffect::Activate { guard, action }]);
            }
            Some(Phase::Waiting(deadline)) => {
                if observed.kind == PageKind::ApplicantNoSlots
                    && !self.applicant_refresh_safe(&observed, &facts)
                {
                    self.phase = Some(Phase::Held);
                    return Ok(vec![FlowEffect::Hold]);
                }
                if now < deadline {
                    return Ok(vec![]);
                }
                let (key, effect) = self.wait_action(&observed, &facts);
                self.attempted.push((observed.document.clone(), key));
                self.phase = Some(if effect == FlowEffect::Hold {
                    Phase::Held
                } else {
                    Phase::Dispatched
                });
                return Ok(vec![effect]);
            }
            Some(Phase::Dispatched) => {
                self.phase = Some(Phase::Held);
                return Ok(vec![FlowEffect::Hold]);
            }
            Some(Phase::Held) => return Ok(vec![]),
            None => {}
        }
        if observed.kind == PageKind::Applicant && self.round_saw_banner
            && facts.confirm_appointment_round && facts.recovery.official_site
        {
            self.phase = Some(Phase::Dispatched);
            return Ok(vec![FlowEffect::Recover { guard, action: RecoveryAction::GetEntry }]);
        }
        let action = match observed.kind {
            PageKind::Province => Some(ActionKind::Province),
            PageKind::Procedure => Some(ActionKind::Procedure),
            PageKind::Applicant => Some(ActionKind::Applicant),
            PageKind::ApplicantNoSlots
                if self.options.applicant_no_slots_mode == ApplicantNoSlotsMode::Continue && !facts.confirm_appointment_round =>
            {
                Some(ActionKind::Applicant)
            }
            PageKind::Contact => Some(ActionKind::Contact),
            PageKind::Office if facts.office_count == 1 => Some(ActionKind::Office),
            _ => None,
        };
        if let Some(action) = action {
            if self
                .attempted
                .contains(&(observed.document.clone(), action))
            {
                self.phase = Some(Phase::Held);
                return Ok(vec![FlowEffect::Hold]);
            }
            self.phase = Some(Phase::Preparing(action));
            return Ok(vec![FlowEffect::Prepare { guard, action }]);
        }
        match observed.kind {
            PageKind::Auth => {
                self.phase = Some(Phase::Dispatched);
                Ok(vec![FlowEffect::EnterAuthentication {
                    guard,
                    mode: facts.authentication,
                }])
            }
            PageKind::RequestMenu => {
                if self
                    .attempted
                    .contains(&(observed.document.clone(), ActionKind::Request))
                {
                    self.phase = Some(Phase::Held);
                    return Ok(vec![FlowEffect::Hold]);
                }
                self.attempted
                    .push((observed.document.clone(), ActionKind::Request));
                self.phase = Some(Phase::Dispatched);
                Ok(vec![FlowEffect::Activate {
                    guard,
                    action: ActionKind::Request,
                }])
            }
            PageKind::ApplicantNoSlots
            | PageKind::RequestNoSlots
            | PageKind::RequestNoSlotsRestart => {
                if observed.kind == PageKind::ApplicantNoSlots
                    && !self.applicant_refresh_safe(&observed, &facts)
                {
                    self.phase = Some(Phase::Held);
                    return Ok(vec![FlowEffect::Hold]);
                }
                let (key, _) = self.wait_action(&observed, &facts);
                if self.attempted.contains(&(observed.document.clone(), key)) {
                    self.phase = Some(Phase::Held);
                    return Ok(vec![FlowEffect::Hold]);
                }
                let delay = match observed.kind {
                    PageKind::RequestNoSlotsRestart => self.timings.no_slots_restart_seconds(),
                    PageKind::ApplicantNoSlots if self.timings.adaptive_reload_on_429() => {
                        self.reload_delay + self.applicant_reload_penalty
                    }
                    _ => self.reload_delay,
                };
                let deadline = deadline_millis(now, delay).map_err(|_| FlowError::ClockOverflow)?;
                self.phase = Some(Phase::Waiting(deadline));
                Ok(vec![FlowEffect::WaitUntil(deadline)])
            }
            PageKind::PostContactNoSlots => {
                let key = (observed.document.clone(), ActionKind::PostContactRestart);
                if !facts.recovery.own_control || self.attempted.contains(&key) {
                    self.phase = Some(Phase::Held);
                    return Ok(vec![FlowEffect::Hold]);
                }
                self.attempted.push(key);
                self.phase = Some(Phase::Dispatched);
                Ok(vec![FlowEffect::Activate {
                    guard,
                    action: ActionKind::PostContactRestart,
                }])
            }
            PageKind::Captcha => {
                self.phase = Some(Phase::Held);
                Ok(vec![FlowEffect::Alert])
            }
            PageKind::Loading => {
                let deadline = deadline_millis(now, self.timings.navigation_seconds())
                    .map_err(|_| FlowError::ClockOverflow)?;
                self.phase = Some(Phase::Loading { deadline });
                Ok(vec![FlowEffect::ReobserveAt(
                    now.saturating_add(500).min(deadline),
                )])
            }
            PageKind::Unknown => {
                let deadline = now.checked_add(500).ok_or(FlowError::ClockOverflow)?;
                self.phase = Some(Phase::UnknownReread {
                    deadline,
                    variant: observed.variant,
                });
                Ok(vec![FlowEffect::ReobserveAt(deadline)])
            }
            PageKind::AuthWait => Ok(vec![FlowEffect::ReobserveAt(
                now.checked_add(500).ok_or(FlowError::ClockOverflow)?,
            )]),
            _ => {
                self.phase = Some(Phase::Held);
                Ok(vec![FlowEffect::Hold])
            }
        }
    }
}
