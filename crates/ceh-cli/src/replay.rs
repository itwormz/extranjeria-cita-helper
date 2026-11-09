//! Offline-only integration harness. The concrete ReplayBrowser cannot open a website.
use ceh_browser::{BrowserPort, ReplayBrowser};
use ceh_core::{
    control::ControlMode,
    flow::{FlowEffect, FlowEngine, PageFacts, PreparationOutcome},
    journal::{JournalEvent, JournalMode, PageJournal},
    model::*,
    ports::{Clock, ClockReading, ManualClock},
    statistics::*,
};
use ceh_services::notifications::NotificationPlanner;
use ceh_store::{
    migrate::StoreError,
    periods::Period,
    statistics::StatisticsReader,
    writer::{ActivityEvidence, ActivityKind, StatisticsWriter},
};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayError {
    InvalidInput,
    InvalidSequence,
    Flow,
    Port,
}
impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "offline replay: {self:?}")
    }
}
impl std::error::Error for ReplayError {}
fn empty() -> Value {
    json!({})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaySpec {
    pub schema_version: u32,
    pub profile: ProfileId,
    pub session: SessionId,
    pub wall_start_millis: i64,
    #[serde(default)]
    pub options: SearchOptions,
    #[serde(default = "empty")]
    pub timings: Value,
    #[serde(default)]
    pub reload_sample: Option<f64>,
    #[serde(default)]
    pub notifications: bool,
    pub steps: Vec<ReplayStep>,
}
#[derive(Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReplayStep {
    Observe {
        at_ms: u64,
        #[serde(default)]
        wall_millis: Option<i64>,
        observation: Observation,
        #[serde(default)]
        facts: PageFacts,
        #[serde(default)]
        offices: Vec<String>,
        #[serde(default)]
        details_pending: bool,
        #[serde(default)]
        lost_ack: bool,
    },
    Prepared {
        at_ms: u64,
        #[serde(default)]
        wall_millis: Option<i64>,
        guard: DocumentGuard,
        action: ActionKind,
        outcome: PreparationOutcome,
    },
    Pause {
        at_ms: u64,
        #[serde(default)]
        wall_millis: Option<i64>,
    },
    Resume {
        at_ms: u64,
        #[serde(default)]
        wall_millis: Option<i64>,
    },
    Heartbeat {
        at_ms: u64,
        #[serde(default)]
        wall_millis: Option<i64>,
    },
    AvailabilityRequest {
        at_ms: u64,
        #[serde(default)]
        wall_millis: Option<i64>,
        operation: OperationId,
    },
    AutomationAction {
        at_ms: u64,
        #[serde(default)]
        wall_millis: Option<i64>,
        operation: OperationId,
    },
    BookingDetails {
        at_ms: u64,
        #[serde(default)]
        wall_millis: Option<i64>,
        document: DocumentId,
        details: Value,
    },
    Activity {
        at_ms: u64,
        #[serde(default)]
        wall_millis: Option<i64>,
        kind: ActivityKind,
        #[serde(default)]
        evidence: ActivityEvidence,
    },
}
impl ReplayStep {
    fn time(&self) -> (u64, Option<i64>) {
        match self {
            Self::Observe {
                at_ms, wall_millis, ..
            }
            | Self::Prepared {
                at_ms, wall_millis, ..
            }
            | Self::Pause { at_ms, wall_millis }
            | Self::Resume { at_ms, wall_millis }
            | Self::Heartbeat { at_ms, wall_millis }
            | Self::AvailabilityRequest {
                at_ms, wall_millis, ..
            }
            | Self::AutomationAction {
                at_ms, wall_millis, ..
            }
            | Self::BookingDetails {
                at_ms, wall_millis, ..
            }
            | Self::Activity {
                at_ms, wall_millis, ..
            } => (*at_ms, *wall_millis),
        }
    }
}
fn track<T>(errors: &mut u64, result: Result<T, StoreError>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(_) => {
            *errors = errors.saturating_add(1);
            None
        }
    }
}
struct Run<'a> {
    db: &'a mut Connection,
    scope: EventScope,
    flow: FlowEngine,
    browser: ReplayBrowser,
    ledger: StatisticsLedger,
    journal: PageJournal,
    planner: NotificationPlanner,
    notify: bool,
    notifications: Vec<Value>,
    current: Option<Observation>,
    phase: &'static str,
    errors: u64,
    booking_doc: Option<DocumentId>,
    details_pending: bool,
    pass_seen: bool,
}
impl Run<'_> {
    fn mode(&self) -> ControlMode {
        if self.flow.control().mode() == ControlMode::Automatic && self.phase == "manual" {
            ControlMode::Manual
        } else {
            self.flow.control().mode()
        }
    }
    fn activity(&mut self, kind: ActivityKind, at: i64, evidence: &ActivityEvidence) {
        let stage = self.current.as_ref().map(|v| v.kind);
        track(
            &mut self.errors,
            StatisticsWriter::new(self.db).activity(
                &self.scope,
                kind,
                at,
                stage,
                self.phase,
                evidence,
            ),
        );
    }
    fn counter(
        &mut self,
        kind: CounterKind,
        identity: EvidenceId,
        at: i64,
        offices: Vec<String>,
    ) -> Result<bool, ReplayError> {
        self.counter_in_mode(kind, identity, at, offices, self.mode())
    }
    fn counter_in_mode(
        &mut self,
        kind: CounterKind,
        identity: EvidenceId,
        at: i64,
        offices: Vec<String>,
        mode: ControlMode,
    ) -> Result<bool, ReplayError> {
        let inserted = self
            .ledger
            .record(CounterObservation {
                key: EventKey {
                    scope: self.scope.clone(),
                    kind,
                    identity,
                },
                at,
                offices,
                mode,
                automated: true,
            })
            .map_err(|_| ReplayError::InvalidInput)?;
        if inserted {
            let event = self.ledger.events().last().expect("inserted event").clone();
            track(
                &mut self.errors,
                StatisticsWriter::new(self.db).counter(&event),
            );
        }
        Ok(inserted)
    }
    fn notify(&mut self, at: u64) -> Result<(), ReplayError> {
        let Some(observed) = &self.current else {
            return Ok(());
        };
        let details = self
            .booking_doc
            .as_ref()
            .and_then(|doc| {
                self.ledger.events().iter().rev().find(|event| {
                    event.key.kind == CounterKind::Booking
                        && event.key.identity == EvidenceId::Document(doc.clone())
                })
            })
            .map(|event| serde_json::to_value(&event.booking_details).expect("public details"))
            .unwrap_or(json!({}));
        let manual = self.flow.control().mode() == ControlMode::Manual;
        let state = ceh_services::notifications::notification_state(&ceh_services::notifications::NotificationStateInput {
            run_id: self.scope.session.as_str().to_owned(),
            document: Some(observed.document.as_str().to_owned()),
            page: observed.kind,
            attention: manual,
            booking_handoff: manual,
            paused: self.flow.control().mode() == ControlMode::Paused,
            booking_details: Some(details),
            booking_details_pending: self.details_pending,
            ..Default::default()
        });
        let config = json!({"telegram":{"enabled":self.notify},"webhook":{"enabled":self.notify}});
        let intents = self
            .planner
            .observe(
                self.scope.profile.as_str(),
                "Replay",
                "Barcelona",
                &state,
                "en",
                &config,
            )
            .map_err(|_| ReplayError::InvalidInput)?;
        for intent in intents {
            self.notifications.push(json!({"at_ms":at,"channel":intent.channel(),"event":intent.event()["event"],"message":intent.message()}));
        }
        Ok(())
    }
    async fn effects(
        &mut self,
        effects: &[FlowEffect],
        wall: i64,
        mono: u64,
        lost_ack: bool,
    ) -> Result<(), ReplayError> {
        // Revoke queued work before touching storage, enrichment or notifications.
        self.browser
            .apply_control(&self.flow.control())
            .map_err(|_| ReplayError::Port)?;
        for effect in effects {
            match effect {
                FlowEffect::Activate { guard, action } => {
                    self.phase = "working";
                    let outcome = self
                        .browser
                        .execute(guard, *action)
                        .await
                        .map_err(|_| ReplayError::Port)?;
                    self.flow
                        .acknowledge_dispatch(
                            guard,
                            *action,
                            if lost_ack {
                                DispatchOutcome::Unknown
                            } else {
                                outcome
                            },
                        )
                        .map_err(|_| ReplayError::Flow)?;
                    self.journal
                        .event(
                            JournalEvent::ActionCompleted,
                            Some(guard.document.clone()),
                            Some(guard.expected_page),
                            false,
                            wall,
                        )
                        .map_err(|_| ReplayError::InvalidSequence)?;
                }
                FlowEffect::Handoff => {
                    self.phase = "booking_handoff";
                    self.activity(ActivityKind::Handoff, wall, &ActivityEvidence::default());
                    self.journal
                        .event(JournalEvent::Handoff, None, None, false, wall)
                        .map_err(|_| ReplayError::InvalidSequence)?;
                }
                FlowEffect::Hold => {
                    self.phase = "manual";
                    self.journal
                        .phase(JournalMode::Manual, None, wall)
                        .map_err(|_| ReplayError::InvalidSequence)?;
                }
                FlowEffect::Alert => {
                    if self.phase != "booking_handoff" {
                        self.phase = "manual";
                    }
                    self.journal
                        .phase(JournalMode::Manual, None, wall)
                        .map_err(|_| ReplayError::InvalidSequence)?;
                }
                FlowEffect::WaitUntil(deadline) => {
                    self.phase = "waiting";
                    self.journal
                        .phase(
                            JournalMode::Waiting,
                            Some(deadline.saturating_sub(mono) as f64 / 1000.0),
                            wall,
                        )
                        .map_err(|_| ReplayError::InvalidSequence)?;
                }
                FlowEffect::Recover { .. } => {
                    self.phase = "recovery";
                    self.journal
                        .event(JournalEvent::Recovery, None, None, false, wall)
                        .map_err(|_| ReplayError::InvalidSequence)?;
                }
                FlowEffect::Prepare { .. }
                | FlowEffect::EnterAuthentication { .. }
                | FlowEffect::SelectClaveMethod { .. }
                | FlowEffect::AcceptCookies { .. }
                | FlowEffect::SelectCertificate { .. }
                | FlowEffect::FetchApplicant { .. }
                | FlowEffect::Reload { .. } => {
                    self.phase = "working";
                }
                _ => {}
            }
        }
        Ok(())
    }
}
pub async fn run(spec: ReplaySpec, db: &mut Connection) -> Result<Value, ReplayError> {
    if spec.schema_version != 1
        || spec.steps.len() > 10_000
        || chrono::DateTime::from_timestamp_millis(spec.wall_start_millis).is_none()
    {
        return Err(ReplayError::InvalidInput);
    }
    let timings =
        ceh_core::timings::parse_timings(&spec.timings).map_err(|_| ReplayError::InvalidInput)?;
    let sample = spec.reload_sample.unwrap_or(timings.reload_seconds()[0]);
    let flow =
        FlowEngine::with_timings(spec.session.clone(), spec.options.clone(), timings, sample)
            .map_err(|_| ReplayError::InvalidInput)?;
    let observations = spec
        .steps
        .iter()
        .filter_map(|step| {
            if let ReplayStep::Observe { observation, .. } = step {
                Some(observation.clone())
            } else {
                None
            }
        })
        .collect();
    let mut state = Run {
        db,
        scope: EventScope {
            profile: spec.profile,
            session: spec.session.clone(),
        },
        flow,
        browser: ReplayBrowser::new(spec.session, observations),
        ledger: StatisticsLedger::new(),
        journal: PageJournal::new(),
        planner: NotificationPlanner::default(),
        notify: spec.notifications,
        notifications: Vec::new(),
        current: None,
        phase: "working",
        errors: 0,
        booking_doc: None,
        details_pending: false,
        pass_seen: false,
    };
    track(&mut state.errors,StatisticsWriter::new(state.db).start_session(&state.scope,spec.wall_start_millis,&json!({"app_version":env!("CARGO_PKG_VERSION"),"province":"Barcelona","procedure":"huellas","no_slots_mode":spec.options.no_slots_mode,"applicant_no_slots_mode":spec.options.applicant_no_slots_mode})));
    state.activity(
        ActivityKind::Start,
        spec.wall_start_millis,
        &ActivityEvidence::default(),
    );
    state.activity(
        ActivityKind::EffortStarted,
        spec.wall_start_millis,
        &ActivityEvidence::default(),
    );
    state
        .journal
        .event(
            JournalEvent::Start,
            None,
            None,
            false,
            spec.wall_start_millis,
        )
        .map_err(|_| ReplayError::InvalidSequence)?;
    track(
        &mut state.errors,
        StatisticsWriter::new(state.db).phase(&state.scope, state.phase, spec.wall_start_millis),
    );
    let mut clock = ManualClock::new(ClockReading {
        unix_millis: spec.wall_start_millis,
        monotonic_millis: 0,
    });
    let mut trace = Vec::new();
    for step in spec.steps {
        let (mono, wall_override) = step.time();
        let previous = clock.now();
        let elapsed = mono
            .checked_sub(previous.monotonic_millis)
            .ok_or(ReplayError::InvalidSequence)?;
        clock
            .advance(elapsed)
            .map_err(|_| ReplayError::InvalidInput)?;
        if let Some(wall) = wall_override {
            clock.set_wall(wall);
        }
        let wall = clock.now().unix_millis;
        if state.mode() == ControlMode::Automatic && elapsed > 0 {
            track(
                &mut state.errors,
                StatisticsWriter::new(state.db).add_active(
                    &state.scope,
                    elapsed as f64 / 1000.0,
                    wall,
                ),
            );
        }
        let mut effects = Vec::new();
        let mut lost_ack = false;
        match step {
            ReplayStep::Observe {
                observation,
                facts,
                offices,
                details_pending,
                lost_ack: lost,
                ..
            } => {
                if state.flow.control().mode() == ControlMode::Paused {
                    return Err(ReplayError::InvalidSequence);
                }
                let received = state
                    .browser
                    .observe()
                    .await
                    .map_err(|_| ReplayError::Port)?;
                if received != observation {
                    return Err(ReplayError::InvalidSequence);
                }
                if state.flow.control().mode() == ControlMode::Automatic
                    && !matches!(observation.kind, PageKind::Unknown | PageKind::Loading)
                    && state
                        .current
                        .as_ref()
                        .is_some_and(|old| old.kind != observation.kind)
                {
                    state.phase = "working";
                }
                let automated = state.mode() == ControlMode::Automatic;
                effects = state
                    .flow
                    .observe_with_facts(observation.clone(), facts, mono)
                    .map_err(|_| ReplayError::Flow)?;
                state
                    .browser
                    .apply_control(&state.flow.control())
                    .map_err(|_| ReplayError::Port)?;
                state.ledger.observe_page(&state.scope, observation.kind);
                if automated && !matches!(observation.kind, PageKind::Unknown | PageKind::Loading) {
                    state.counter_in_mode(
                        CounterKind::ProcessedPage,
                        EvidenceId::Document(observation.document.clone()),
                        wall,
                        vec![],
                        ControlMode::Automatic,
                    )?;
                }
                if automated
                    && observation.kind == PageKind::Province
                    && state.counter(
                        CounterKind::Pass,
                        EvidenceId::Document(observation.document.clone()),
                        wall,
                        vec![],
                    )?
                {
                    let event = if state.pass_seen {
                        JournalEvent::FlowRestart
                    } else {
                        JournalEvent::FlowStart
                    };
                    state.pass_seen = true;
                    state
                        .journal
                        .event(
                            event,
                            Some(observation.document.clone()),
                            Some(PageKind::Province),
                            false,
                            wall,
                        )
                        .map_err(|_| ReplayError::InvalidSequence)?;
                }
                state
                    .journal
                    .observe(
                        observation.document.clone(),
                        observation.kind,
                        spec.options.applicant_no_slots_mode == ApplicantNoSlotsMode::Continue,
                        wall,
                    )
                    .map_err(|_| ReplayError::InvalidSequence)?;
                track(
                    &mut state.errors,
                    StatisticsWriter::new(state.db).page(&state.scope, &observation, wall),
                );
                let kind = match observation.kind {
                    PageKind::ServerError => Some(CounterKind::ServerError),
                    PageKind::RateLimited => Some(CounterKind::RateLimited),
                    PageKind::Blocked => Some(CounterKind::Blocked),
                    PageKind::Expired => Some(CounterKind::Expired),
                    PageKind::Booking => Some(CounterKind::Booking),
                    PageKind::Office if !offices.is_empty() => Some(CounterKind::Office),
                    _ => None,
                };
                if let Some(kind) = kind {
                    state.counter(
                        kind,
                        EvidenceId::Document(observation.document.clone()),
                        wall,
                        offices.clone(),
                    )?;
                }
                if observation.kind == PageKind::Office {
                    state.journal.offices(offices, None);
                }
                if observation.kind == PageKind::Booking {
                    state.booking_doc = Some(observation.document.clone());
                    state.details_pending = details_pending;
                }
                state.current = Some(observation);
                lost_ack = lost;
            }
            ReplayStep::Prepared {
                guard,
                action,
                outcome,
                ..
            } => state
                .flow
                .complete_preparation(&guard, action, outcome)
                .map_err(|_| ReplayError::Flow)?,
            ReplayStep::Pause { .. } => {
                state.flow.pause(mono).map_err(|_| ReplayError::Flow)?;
                state
                    .browser
                    .apply_control(&state.flow.control())
                    .map_err(|_| ReplayError::Port)?;
                state.phase = "paused";
                state.activity(ActivityKind::Pause, wall, &ActivityEvidence::default());
                state
                    .journal
                    .event(JournalEvent::Pause, None, None, true, wall)
                    .map_err(|_| ReplayError::InvalidSequence)?;
            }
            ReplayStep::Resume { .. } => {
                state.flow.resume(mono).map_err(|_| ReplayError::Flow)?;
                state
                    .browser
                    .apply_control(&state.flow.control())
                    .map_err(|_| ReplayError::Port)?;
                state.phase = "working";
                state.activity(ActivityKind::Resume, wall, &ActivityEvidence::default());
                state
                    .journal
                    .event(JournalEvent::Resume, None, None, true, wall)
                    .map_err(|_| ReplayError::InvalidSequence)?;
            }
            ReplayStep::Heartbeat { .. } => {
                track(
                    &mut state.errors,
                    StatisticsWriter::new(state.db).heartbeat(&state.scope, wall),
                );
            }
            ReplayStep::AvailabilityRequest { operation, .. } => {
                state.counter(
                    CounterKind::Check,
                    EvidenceId::Operation(operation),
                    wall,
                    vec![],
                )?;
                state
                    .journal
                    .event(JournalEvent::Check, None, None, false, wall)
                    .map_err(|_| ReplayError::InvalidSequence)?;
            }
            ReplayStep::AutomationAction { operation, .. } => {
                state.counter(
                    CounterKind::AutomationAction,
                    EvidenceId::Operation(operation),
                    wall,
                    vec![],
                )?;
            }
            ReplayStep::BookingDetails {
                document, details, ..
            } => {
                if state.booking_doc.as_ref() == Some(&document) {
                    if state
                        .ledger
                        .enrich_booking(&state.scope, &document, &details)
                    {
                        track(
                            &mut state.errors,
                            StatisticsWriter::new(state.db).enrich_booking(
                                &state.scope,
                                &document,
                                &details,
                            ),
                        );
                        state.journal.booking(&document, &details);
                    }
                    state.details_pending = false;
                }
            }
            ReplayStep::Activity { kind, evidence, .. } => state.activity(kind, wall, &evidence),
        }
        state.effects(&effects, wall, mono, lost_ack).await?;
        track(
            &mut state.errors,
            StatisticsWriter::new(state.db).phase(&state.scope, state.phase, wall),
        );
        state.notify(mono)?;
        trace.push(json!({"at_ms":mono,"effects":effects,"control":state.flow.control(),"phase":state.phase}));
    }
    let last = clock.now();
    state.activity(
        ActivityKind::Finish,
        last.unix_millis,
        &ActivityEvidence::default(),
    );
    track(
        &mut state.errors,
        StatisticsWriter::new(state.db).end_session(&state.scope, last.unix_millis, "completed"),
    );
    state
        .journal
        .event(JournalEvent::Finish, None, None, false, last.unix_millis)
        .map_err(|_| ReplayError::InvalidSequence)?;
    state.planner.stop();
    let now = chrono::DateTime::from_timestamp_millis(last.unix_millis).unwrap_or_else(|| {
        chrono::DateTime::from_timestamp_millis(spec.wall_start_millis).expect("validated start")
    });
    let statistics = track(
        &mut state.errors,
        StatisticsReader::new(state.db).summary(state.scope.profile.as_str(), Period::All, now),
    );
    let timeline = track(
        &mut state.errors,
        StatisticsReader::new(state.db).timeline_history(
            state.scope.profile.as_str(),
            Period::All,
            now,
            None,
            None,
        ),
    );
    let card = statistics
        .as_ref()
        .filter(|_| state.errors == 0)
        .map(|statistics| {
            ceh_services::exports::public_card(
                &json!({"statistics":statistics,"recorded_at":now.to_rfc3339()}),
                "en",
            )
        });
    Ok(
        json!({"schema_version":1,"offline":true,"native_browser_started":false,"network_delivery_performed":false,"native_vault_accessed":false,"control":state.flow.control(),"trace":trace,"browser_actions":state.browser.actions().iter().map(|action|json!({"guard":action.guard,"action":action.action})).collect::<Vec<_>>(),"session_counters":state.ledger.totals(&state.scope.profile),"statistics":statistics,"statistics_errors":state.errors,"journal":state.journal.rows(),"timeline":timeline,"notifications":state.notifications,"public_card":card,"native_adapter_gaps":["DOM preparation and authentication","reload and recovery command execution","native cancellation and window behavior"]}),
    )
}
