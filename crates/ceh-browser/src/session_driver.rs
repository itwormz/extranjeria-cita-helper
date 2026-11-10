//! Trusted worker-side integration of observed documents, core effects and native actions.
//! No UI scripts/selectors or raw page/profile data are returned.
use crate::{BrowserError, DispatchGate, bidi::BidiConnection, observer::PageObserver};
use ceh_core::{
    booking::PublicBookingDetails,
    control::{ControlMode, ControlSnapshot},
    flow::{AuthenticationMode, ClaveMethod, FlowEffect, FlowEngine, PageFacts},
    model::*,
    profiles::ProfileDraft,
    timings::{AssistTimings,TimingKind},
    preparation::{SessionPreparation,PreparationDecision},
};
use std::time::{Duration, Instant};
/// Independent unit samples are supplied by the runtime; the policy stays deterministic.
#[derive(Clone,Copy)]
pub struct SessionSamples{pub first_bootstrap:f64,pub second_bootstrap:f64,pub reload:f64,pub contact_typing:f64,pub typing:f64}
impl Default for SessionSamples{fn default()->Self{Self{first_bootstrap:0.5,second_bootstrap:0.5,reload:0.5,contact_typing:0.5,typing:0.5}}}
#[derive(Clone)]
pub struct ActionReceipt {
    pub document: DocumentId,
    pub action: ActionKind,
    pub outcome: DispatchOutcome,
}
pub struct DriverUpdate {
    pub preparing: bool,
    pub observation: Observation,
    pub effects: Vec<FlowEffect>,
    pub control: ControlSnapshot,
    pub offices: Vec<String>,
    pub details: PublicBookingDetails,
    pub receipts: Vec<ActionReceipt>,
    pub applicant_prepared: bool,
}
/// Linear native plan: only the driver can create it, and it is consumed once.
/// Public metadata may be published before any action or bounded enrichment.
pub struct NativeStep {
    page: crate::observer::ObservedPage,
    recovery: Option<crate::recovery_controls::RecoveryRead>,
    update: DriverUpdate,
    now: u64,
    preparation_reload: bool,
}
impl NativeStep {
    pub fn diagnostic_snapshot(&self, connection: &mut BidiConnection) -> Option<String> {
        if self.update.control.mode() != ControlMode::Automatic
            || !self.page.page.snapshot.official
            || !matches!(self.update.observation.kind, PageKind::Province | PageKind::Procedure | PageKind::Auth | PageKind::Applicant | PageKind::ApplicantNoSlots | PageKind::RequestMenu | PageKind::RequestNoSlots | PageKind::RequestNoSlotsRestart | PageKind::ServerError | PageKind::Blocked | PageKind::RateLimited | PageKind::Expired)
        {
            return None;
        }
        let script = include_str!("diagnostic_snapshot.js");
        let value = connection.call_before("script.callFunction", serde_json::json!({
            "target": {"realm": self.update.observation.document.as_str()},
            "functionDeclaration": script,
            "awaitPromise": false
        }), Instant::now() + Duration::from_millis(750)).ok()?;
        value["result"]["value"].as_str().filter(|html| html.len() <= 256 * 1024).map(str::to_owned)
    }

    pub fn diagnostic_image(&self, connection: &mut BidiConnection) -> Option<Vec<u8>> {
        use base64::Engine;
        if self.update.control.mode() != ControlMode::Automatic || !self.page.page.snapshot.official
            || !matches!(self.update.observation.kind, PageKind::Province | PageKind::Procedure | PageKind::Auth)
            || self.page.page.sources.len() != 1
        {
            return None;
        }
        let source = self.page.page.sources.first()?;
        let result = connection.call("browsingContext.captureScreenshot", serde_json::json!({
            "context":source.context, "origin":"viewport", "format":{"type":"image/png"}
        })).ok()?;
        let encoded = result["data"].as_str().filter(|data| data.len() <= 4 * 1024 * 1024)?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).ok()?;
        if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return None;
        }
        let current = crate::dom::read_document(connection, &source.context, "manual").ok()?;
        (current.realm == source.realm && current.snapshot.official).then_some(bytes)
    }

    pub fn update(&self) -> &DriverUpdate {
        &self.update
    }
}
/// A requested fresh pass only runs where the app may navigate: the official site, never a captcha or
/// booking screen (the person owns those) nor a provider page such as Cl@ve. Elsewhere it is dropped
/// instead of failing the whole search.
fn fresh_pass_applicable(official: bool, kind: PageKind) -> bool {
    official && !matches!(kind, PageKind::Captcha | PageKind::Booking)
}
pub struct SessionDriver {
    flow: FlowEngine,
    observer: PageObserver,
    gate: DispatchGate,
    profile: ProfileDraft,
    timings: AssistTimings,
    last: Option<ActionKind>,
    pending: Option<(DocumentId, ActionKind)>,
    office: Option<String>,
    cancelled_preactions: Vec<(DocumentGuard, ActionKind)>,
    preparation:SessionPreparation,
    contact_typing_ms:f64,
    typing_ms:f64,
    appointment_ready_at: Option<u64>,
    confirm_appointment_round: bool,
    fresh_pass_requested: bool,
}
/// What the driver remembers of the previous action after a navigation recovery (Python `recover`): a
/// recovery GET clears `last_action`/`pending_action`; a reload keeps the action it repeats, so the next
/// server error still sees the availability search and reloads again instead of holding.
pub fn recovery_memory(
    action: ceh_core::recovery::RecoveryAction,
    last: Option<ActionKind>,
    pending: Option<(DocumentId, ActionKind)>,
) -> (Option<ActionKind>, Option<(DocumentId, ActionKind)>) {
    if action == ceh_core::recovery::RecoveryAction::Reload {
        (last.or(pending.map(|(_, kind)| kind)), None)
    } else {
        (None, None)
    }
}
impl SessionDriver {
    pub fn new(
        context: &str,
        session: SessionId,
        profile: ProfileDraft,
        timings: AssistTimings,
        gate: DispatchGate,
    ) -> Result<Self, BrowserError> {
        Self::with_samples(context,session,profile,timings,gate,SessionSamples::default())
    }
    pub fn with_samples(context:&str,session:SessionId,profile:ProfileDraft,timings:AssistTimings,gate:DispatchGate,samples:SessionSamples)->Result<Self,BrowserError>{
        let sampled=|kind,unit|timings.sample(kind,unit).map_err(|_|BrowserError::Unsupported);
        let preparation=SessionPreparation::new((sampled(TimingKind::Bootstrap,samples.first_bootstrap)?*1000.).ceil() as u64,(sampled(TimingKind::Bootstrap,samples.second_bootstrap)?*1000.).ceil() as u64,(timings.navigation_seconds()*1000.).ceil() as u64).map_err(|_|BrowserError::Unsupported)?;
        let reload_sample=sampled(TimingKind::Reload,samples.reload)?;
        let contact_typing_ms=sampled(TimingKind::ContactTyping,samples.contact_typing)?;
        let typing_ms=sampled(TimingKind::Typing,samples.typing)?;
        let options = SearchOptions {
            no_slots_mode: match profile.no_slots_mode.as_str() {
                "salir" => NoSlotsMode::Salir,
                "reload" => NoSlotsMode::Reload,
                _ => return Err(BrowserError::Unsupported),
            },
            applicant_no_slots_mode: match profile.applicant_no_slots_mode.as_str() {
                "wait" => ApplicantNoSlotsMode::Wait,
                "continue" => ApplicantNoSlotsMode::Continue,
                _ => return Err(BrowserError::Unsupported),
            },
        };
        let flow = FlowEngine::with_timings(
            session,
            options,
            timings.clone(),
            reload_sample,
        )
        .map_err(|_| BrowserError::Unsupported)?;
        let observer = PageObserver::new(context).map_err(|_| BrowserError::NoObservation)?;
        Ok(Self {
            flow,
            observer,
            gate,
            profile,
            timings,
            last: None,
            pending: None,
            office: None,
            cancelled_preactions: Vec::new(),
            preparation,
            contact_typing_ms,
            typing_ms,
            appointment_ready_at: None,
            confirm_appointment_round: false,
            fresh_pass_requested: false,
        })
    }
    pub fn ingest(&mut self, event: &serde_json::Value) -> Result<(), BrowserError> {
        self.observer
            .ingest(event)
            .map_err(|_| BrowserError::NoObservation)
    }
    pub fn set_passive_diagnostics(&mut self, enabled: bool) -> Result<(), BrowserError> {
        self.observer.set_passive_collection(enabled).map_err(|_| BrowserError::NoObservation)
    }

    pub fn set_network_journal(&mut self, wall: Option<Box<dyn Fn() -> String + Send>>) {
        self.observer.set_network_journal(wall);
    }
    pub fn take_network_rows(&mut self) -> Vec<serde_json::Value> {
        self.observer.take_network_rows()
    }
    pub fn take_diagnostic_responses(&mut self) -> crate::passive::ResponseBatch {
        self.observer.take_responses()
    }

    pub fn control(&self) -> ControlSnapshot {
        self.flow.control()
    }
    pub fn pause(&mut self, now: u64) -> Result<ControlSnapshot, BrowserError> {
        let mut preparation=self.preparation.clone();preparation.pause(now).map_err(|_|BrowserError::InvalidControl)?;
        let snapshot = self
            .flow
            .pause(now)
            .map_err(|_| BrowserError::ManualOwnership)?;
        self.gate.apply_control(&snapshot)?;
        self.preparation=preparation;
        for (guard, action) in self.cancelled_preactions.drain(..) {
            let _ = self.flow.cancel_unpublished_pre_action(&guard, action);
        }
        Ok(snapshot)
    }
    pub fn resume(&mut self, now: u64) -> Result<ControlSnapshot, BrowserError> {
        let mut preparation=self.preparation.clone();preparation.resume(now).map_err(|_|BrowserError::InvalidControl)?;
        let snapshot = self
            .flow
            .resume(now)
            .map_err(|_| BrowserError::ManualOwnership)?;
        self.gate.apply_control(&snapshot)?;
        self.preparation=preparation;
        Ok(snapshot)
    }
    /// Only native observations / trusted fixtures call this; never expose it to IPC.
    pub fn accept_observation(
        &mut self,
        observation: Observation,
        facts: PageFacts,
        now: u64,
    ) -> Result<Vec<FlowEffect>, BrowserError> {
        let effects = self
            .flow
            .observe_with_facts(observation, facts, now)
            .map_err(|_| BrowserError::InvalidControl)?;
        self.gate.apply_control(&self.flow.control())?;
        Ok(effects)
    }
    pub fn tick(
        &mut self,
        connection: &mut BidiConnection,
        now: u64,
    ) -> Result<DriverUpdate, BrowserError> {
        let step = self.plan(connection, now)?;
        self.execute(connection, step)
    }
    /// Apply saved profile timings to the running session. The one-time bootstrap pauses are not repeated.
    pub fn apply_timings(&mut self, timings: AssistTimings, samples: SessionSamples) -> Result<(), BrowserError> {
        let sampled = |kind, unit| timings.sample(kind, unit).map_err(|_| BrowserError::Unsupported);
        let reload_sample = sampled(TimingKind::Reload, samples.reload)?;
        let contact_typing_ms = sampled(TimingKind::ContactTyping, samples.contact_typing)?;
        let typing_ms = sampled(TimingKind::Typing, samples.typing)?;
        self.flow
            .apply_timings(timings.clone(), reload_sample)
            .map_err(|_| BrowserError::Unsupported)?;
        self.contact_typing_ms = contact_typing_ms;
        self.typing_ms = typing_ms;
        self.timings = timings;
        Ok(())
    }
    /// Ask for a new pass from the entry page (Try now). It replaces the next planned step when the
    /// automation owns an official, non-captcha page; on any other page the request is dropped.
    /// Recovery counters that outlive a browser session (blocked streak, 429 reload penalty).
    pub fn recovery_carry(&self) -> ceh_core::flow::RecoveryCarry {
        self.flow.recovery_carry()
    }
    pub fn apply_recovery_carry(&mut self, carry: ceh_core::flow::RecoveryCarry) {
        self.flow.apply_carry(carry);
    }
    pub fn request_fresh_pass(&mut self) {
        self.fresh_pass_requested = true;
    }
    pub fn set_appointment_schedule(&mut self, deadline: u64, confirm_round: bool) {
        self.appointment_ready_at = Some(deadline);
        self.confirm_appointment_round = confirm_round;
    }
    pub fn plan(
        &mut self,
        connection: &mut BidiConnection,
        now: u64,
    ) -> Result<NativeStep, BrowserError> {
        match self.flow.control().mode() {
            ControlMode::Paused => return Err(BrowserError::Paused),
            ControlMode::Manual => return Err(BrowserError::ManualOwnership),
            ControlMode::Automatic => {}
        }
        self.observer
            .set_actions(self.last, self.pending.as_ref().map(|p| p.1));
        let page = self
            .observer
            .observe(connection, &self.profile.fill_mode)
            .map_err(|_| BrowserError::NoObservation)?;
        if self
            .pending
            .as_ref()
            .is_some_and(|(doc, _)| doc != &page.observation.document)
        {
            self.pending = None;
        }
        let preparation=self.preparation.observe_quiet(&page.observation,page.page.snapshot.official,now,self.observer.network_quiet()).map_err(|_|BrowserError::InvalidControl)?;
        if preparation!=PreparationDecision::Continue{
            let reload=preparation==PreparationDecision::ReloadGet;
            let effects=match preparation{PreparationDecision::WaitUntil(deadline)=>vec![FlowEffect::WaitUntil(deadline)],PreparationDecision::Hold=>vec![FlowEffect::Hold],_=>vec![FlowEffect::ReobserveAt(now)]};
            let update=DriverUpdate{preparing:true,observation:page.observation.clone(),effects,control:self.flow.control(),offices:vec![],details:Default::default(),receipts:vec![],applicant_prepared:false};
            return Ok(NativeStep{page,recovery:None,update,now,preparation_reload:reload});
        }
        let mut facts = PageFacts {
            appointment_ready_at: self.appointment_ready_at,
            confirm_appointment_round: self.confirm_appointment_round,
            authentication: if self.profile.access_mode == "certificate" {
                AuthenticationMode::Certificate
            } else {
                AuthenticationMode::Clave
            },
            clave_method: if self.profile.clave_method == "permanente" {
                ClaveMethod::Permanente
            } else {
                ClaveMethod::Movil
            },
            ..Default::default()
        };
        facts.recovery.official_site = page.page.snapshot.official;
        facts.recovery.endpoint = match page.page.snapshot.endpoint.as_str() {
            "acEntrada" => ceh_core::recovery::RecoveryEndpoint::ApplicantEntry,
            "acCitar" => ceh_core::recovery::RecoveryEndpoint::Availability,
            "acInfo" => ceh_core::recovery::RecoveryEndpoint::AuthInfo,
            _ => Default::default(),
        };
        facts.recovery.last_action = self.last;
        facts.recovery.pending_action = self.pending.as_ref().map(|p| p.1);
        let recovery = if matches!(
            page.observation.kind,
            PageKind::Expired | PageKind::ServerError | PageKind::Blocked | PageKind::RateLimited
        ) {
            let read = crate::recovery_controls::read(
                connection,
                &page,
                self.last,
                self.pending.as_ref().map(|p| p.1),
                false,
            )?;
            facts.recovery = read.facts;
            Some(read)
        } else {
            None
        };
        let offices = if page.observation.kind == PageKind::Office {
            let read = crate::public_details::read_offices(connection, &page)?;
            facts.office_count = read.offices.len();
            update_office_cache(
                &mut self.office,
                page.observation.kind,
                read.selected,
                &read.offices,
            );
            read.offices
        } else {
            Vec::new()
        };
        if page.observation.kind != PageKind::Office {
            update_office_cache(&mut self.office, page.observation.kind, None, &[]);
        }
        if matches!(page.observation.kind, PageKind::Auth | PageKind::AuthWait) {
            let read = crate::auth::read_auth_facts(connection, &page)?;
            facts.certificate_method_available = read.certificate;
            facts.clave_method_available = if facts.clave_method == ClaveMethod::Permanente {
                read.permanente
            } else {
                read.movil
            };
            facts.clave_requires_attention = read.requires_attention;
            facts.clave_input_started = read.input_started;
        }
        facts.cookie_accept_available = crate::cookies::control_available(connection, &page)?;
        if matches!(page.observation.kind, PageKind::PostContactNoSlots | PageKind::RequestNoSlots | PageKind::RequestNoSlotsRestart) {
            facts.recovery.own_control = crate::exit::control_available(
                connection,
                &page,
                &crate::exit::ExitAction::from_action(match page.observation.kind {
                    PageKind::RequestNoSlots => ActionKind::NoSlotsSalir,
                    PageKind::RequestNoSlotsRestart => ActionKind::NoSlotsRestart,
                    _ => ActionKind::PostContactRestart,
                })?,
            )?;
        }
        let mut effects = self.accept_observation(page.observation.clone(), facts, now)?;
        if self.fresh_pass_requested {
            self.fresh_pass_requested = false;
            if self.flow.control().mode() == ControlMode::Automatic
                && fresh_pass_applicable(page.page.snapshot.official, page.observation.kind)
            {
                effects = self.flow.fresh_pass(now).map_err(|_| BrowserError::InvalidControl)?;
            }
        }
        let update = DriverUpdate {
            preparing:false,
            observation: page.observation.clone(),
            effects,
            control: self.flow.control(),
            offices,
            details: Default::default(),
            receipts: Vec::new(),
            applicant_prepared: false,
        };
        Ok(NativeStep {
            page,
            recovery,
            update,
            now,
            preparation_reload:false,
        })
    }
    pub fn execute(
        &mut self,
        connection: &mut BidiConnection,
        step: NativeStep,
    ) -> Result<DriverUpdate, BrowserError> {
        let candidates: Vec<_> = step
            .update
            .effects
            .iter()
            .filter_map(|e| match e {
                FlowEffect::AcceptCookies { guard } => Some((guard.clone(), ActionKind::Cookie)),
                FlowEffect::SelectCertificate { guard }
                | FlowEffect::SelectClaveMethod { guard, .. } => {
                    Some((guard.clone(), ActionKind::Auth))
                }
                _ => None,
            })
            .collect();
        let result = self.execute_inner(connection, step);
        if matches!(result, Err(BrowserError::Paused)) {
            self.cancelled_preactions.extend(candidates);
        }
        result
    }
    fn execute_inner(
        &mut self,
        connection: &mut BidiConnection,
        step: NativeStep,
    ) -> Result<DriverUpdate, BrowserError> {
        if self.flow.control() != step.update.control {
            return Err(BrowserError::StaleCommand);
        }
        let NativeStep {
            page,
            recovery,
            mut update,
            now,
            preparation_reload,
        } = step;
        if preparation_reload{
            let control=self.flow.control();
            let guard=DocumentGuard{session:control.session().clone(),epoch:control.epoch(),document:page.observation.document.clone(),expected_page:PageKind::Province};
            let outcome=crate::refresh::reload_entry(connection,&mut self.observer,&self.gate,&guard)?;
            self.preparation.reload_sent(outcome,now).map_err(|_|BrowserError::InvalidControl)?;
            update.receipts.push(ActionReceipt{document:guard.document,action:ActionKind::Recovery,outcome});
            return Ok(update);
        }
        let effects = &update.effects;
        // The lifecycle owner can preserve the process and publish handoff first.
        if effects.contains(&FlowEffect::Handoff) {
            update.details =
                crate::public_details::read_booking(connection, &page, self.office.as_deref())
                    .unwrap_or_default();
        }
        let mut receipts = Vec::new();
        for effect in effects {
            match effect {
                FlowEffect::Prepare { guard, action } => {
                    let deadline =
                        Instant::now() + Duration::from_secs_f64(self.timings.navigation_seconds());
                    let result = match action {
                        ActionKind::Province | ActionKind::Procedure | ActionKind::Office => {
                            let selection = self.selection(*action)?;
                            crate::selection::prepare_selection(
                                connection,
                                &mut self.observer,
                                &self.gate,
                                guard,
                                &selection,
                                deadline,
                            )
                        }
                        ActionKind::Applicant => crate::applicant::prepare_applicant(
                            connection,
                            &mut self.observer,
                            &self.gate,
                            guard,
                            &crate::applicant::ApplicantInput::from_profile(&self.profile)?,
                            Duration::from_secs_f64(self.typing_ms / 1000.0),
                            deadline,
                        ),
                        ActionKind::Contact => {
                            let delay=self.contact_typing_ms;
                        crate::contact::prepare_contact(
                                connection,
                                &mut self.observer,
                                &self.gate,
                                guard,
                                &crate::contact::ContactInput::from_profile(&self.profile)?,
                                Duration::from_secs_f64(delay / 1000.0),
                                deadline,
                            )
                        }
                        _ => Err(BrowserError::Unsupported),
                    }?;
                    update.applicant_prepared = *action == ActionKind::Applicant
                        && result == ceh_core::flow::PreparationOutcome::Completed;
                    self.flow
                        .complete_preparation(guard, *action, result)
                        .map_err(|_| BrowserError::InvalidControl)?;
                }
                FlowEffect::Activate { guard, action } => {
                    let outcome = match action {
                        ActionKind::Province | ActionKind::Procedure | ActionKind::Office => {
                            let selection = self.selection(*action)?;
                            crate::selection::activate_selection(
                                connection,
                                &mut self.observer,
                                &self.gate,
                                guard,
                                &selection,
                            )
                        }
                        ActionKind::Applicant => crate::applicant::activate_applicant(
                            connection,
                            &mut self.observer,
                            &self.gate,
                            guard,
                            &crate::applicant::ApplicantInput::from_profile(&self.profile)?,
                        ),
                        ActionKind::Contact => crate::contact::activate_contact(
                            connection,
                            &mut self.observer,
                            &self.gate,
                            guard,
                            &crate::contact::ContactInput::from_profile(&self.profile)?,
                        ),
                        ActionKind::Request => crate::request::activate_request(
                            connection,
                            &mut self.observer,
                            &self.gate,
                            guard,
                        ),
                        ActionKind::NoSlotsRestart
                        | ActionKind::NoSlotsSalir
                        | ActionKind::PostContactRestart => crate::exit::activate_exit(
                            connection,
                            &mut self.observer,
                            &self.gate,
                            guard,
                            &crate::exit::ExitAction::from_action(*action)?,
                        ),
                        _ => Err(BrowserError::Unsupported),
                    }?;
                    self.receipt(guard, *action, outcome, &mut receipts)?;
                }
                FlowEffect::AcceptCookies { guard } => {
                    let outcome =
                        crate::cookies::accept(connection, &mut self.observer, &self.gate, guard)?;
                    self.receipt(guard, ActionKind::Cookie, outcome, &mut receipts)?;
                }
                FlowEffect::EnterAuthentication { guard, .. } => {
                    let outcome = crate::auth::enter_portal(
                        connection,
                        &mut self.observer,
                        &self.gate,
                        guard,
                    )?;
                    self.receipt(guard, ActionKind::Auth, outcome, &mut receipts)?;
                }
                FlowEffect::SelectCertificate { guard }
                | FlowEffect::SelectClaveMethod { guard, .. } => {
                    let choice = match effect {
                        FlowEffect::SelectCertificate { .. } => {
                            crate::auth::AuthenticationChoice::Certificate
                        }
                        FlowEffect::SelectClaveMethod {
                            method: ClaveMethod::Permanente,
                            ..
                        } => crate::auth::AuthenticationChoice::Permanente,
                        _ => crate::auth::AuthenticationChoice::Movil,
                    };
                    let published = Instant::now();
                    let flow = &mut self.flow;
                    let outcome = crate::auth::select_method(
                        connection,
                        &mut self.observer,
                        &self.gate,
                        guard,
                        choice,
                        || {
                            let _ = flow.certificate_published(
                                guard,
                                now.saturating_add(published.elapsed().as_millis() as u64),
                            );
                        },
                    )?;
                    self.receipt(guard, ActionKind::Auth, outcome, &mut receipts)?;
                }
                FlowEffect::Reload { guard } | FlowEffect::FetchApplicant { guard } => {
                    let requested =
                        crate::refresh::plan_refresh(&page.observation, &page.page.snapshot)?;
                    let outcome = crate::refresh::execute_refresh(
                        connection,
                        &mut self.observer,
                        &self.gate,
                        guard,
                        requested,
                    )?;
                    let action = if guard.expected_page == PageKind::ApplicantNoSlots {
                        ActionKind::ApplicantReload
                    } else {
                        ActionKind::RequestReload
                    };
                    self.receipt(guard, action, outcome, &mut receipts)?;
                }
                FlowEffect::Recover {
                    guard,
                    action: ceh_core::recovery::RecoveryAction::ActivateOwnControl,
                } => {
                    let outcome = crate::recovery_controls::activate_own_control(
                        connection,
                        &mut self.observer,
                        &self.gate,
                        guard,
                        recovery.as_ref().ok_or(BrowserError::NoObservation)?,
                    )?;
                    self.receipt(guard, ActionKind::Recovery, outcome, &mut receipts)?;
                }
                FlowEffect::Recover { guard, action: ceh_core::recovery::RecoveryAction::GetEntry } if self.appointment_ready_at.is_some() => {
                    let outcome = crate::refresh::start_fresh_pass(connection, &mut self.observer, &self.gate, guard)?;
                    self.receipt(guard, ActionKind::Recovery, outcome, &mut receipts)?;
                }
                FlowEffect::Recover {
                    guard,
                    action: action @ (ceh_core::recovery::RecoveryAction::Reload
                    | ceh_core::recovery::RecoveryAction::GetCurrent
                    | ceh_core::recovery::RecoveryAction::GetEntry),
                } => {
                    let outcome = crate::recovery_navigation::execute_navigation(
                        connection, &mut self.observer, &self.gate, guard, *action,
                    )?;
                    let before = (self.last, self.pending.clone());
                    self.receipt(guard, ActionKind::Recovery, outcome, &mut receipts)?;
                    if outcome == DispatchOutcome::Dispatched {
                        (self.last, self.pending) = recovery_memory(*action, before.0, before.1);
                    }
                }
                // Recovery that replaces the browser is handled by the lifecycle owner.
                _ => {}
            }
        }
        update.receipts = receipts;
        update.control = self.flow.control();
        Ok(update)
    }
    /// Only called by the owning worker after successful copy preparation.
    /// The values are used for the private budget key and never exposed in the journal.
    pub fn copied_budget_identity(
        &mut self,
        connection: &mut BidiConnection,
        expected: &Observation,
    ) -> Result<(String, String), BrowserError> {
        if self.profile.fill_mode != "copy" {
            return Err(BrowserError::Unsupported);
        }
        let fresh = self.observer.observe(connection, "copy").map_err(|_| BrowserError::NoObservation)?;
        if !fresh.page.snapshot.official || fresh.observation.document != expected.document
            || !matches!(fresh.observation.kind, PageKind::Applicant | PageKind::ApplicantNoSlots)
        {
            return Err(BrowserError::StaleDocument);
        }
        let expression = r#"(() => {
          if (location.origin !== 'https://icp.administracionelectronica.gob.es') return null;
          const field = document.querySelector('input#txtIdCitado');
          const number = field?.value?.trim();
          if (!number || number.length > 128) return null;
          const radio = document.querySelector('input[name="rdbTipoDoc"]:checked');
          const kind = {rdbTipoDocNie:'NIE',rdbTipoDocDni:'DNI',rdbTipoDocPas:'PASSPORT'}[radio?.id]
            ?? (/^[XYZ]\d{7}[A-Z]$/i.test(number) ? 'NIE' : /^\d{8}[A-Z]$/i.test(number) ? 'DNI' : 'PASSPORT');
          return JSON.stringify([kind, number]);
        })()"#;
        let value = connection.call("script.evaluate", serde_json::json!({
            "target":{"realm":expected.document.as_str()},"expression":expression,"awaitPromise":false
        })).map_err(|_| BrowserError::NoObservation)?;
        let data = value["result"]["value"].as_str().filter(|v| v.len() <= 1024)
            .ok_or(BrowserError::NoObservation)?;
        serde_json::from_str(data).map_err(|_| BrowserError::NoObservation)
    }

    fn selection(&self, action: ActionKind) -> Result<crate::selection::Selection, BrowserError> {
        match action {
            ActionKind::Province => crate::selection::Selection::province(&self.profile.province),
            ActionKind::Procedure => {
                crate::selection::Selection::procedure(&self.profile.procedure)
            }
            ActionKind::Office => Ok(crate::selection::Selection::single_office()),
            _ => Err(BrowserError::Unsupported),
        }
    }
    fn receipt(
        &mut self,
        guard: &DocumentGuard,
        action: ActionKind,
        outcome: DispatchOutcome,
        out: &mut Vec<ActionReceipt>,
    ) -> Result<(), BrowserError> {
        self.flow
            .acknowledge_dispatch(guard, action, outcome)
            .map_err(|_| BrowserError::InvalidControl)?;
        if outcome != DispatchOutcome::NotDispatched {
            self.last = Some(action);
            self.pending = Some((guard.document.clone(), action));
        }
        out.push(ActionReceipt {
            document: guard.document.clone(),
            action,
            outcome,
        });
        Ok(())
    }
}

fn update_office_cache(
    cache: &mut Option<String>,
    kind: PageKind,
    selected: Option<String>,
    offices: &[String],
) {
    if kind == PageKind::Office {
        *cache = selected.or_else(|| {
            if offices.len() == 1 {
                offices.first().cloned()
            } else {
                None
            }
        });
    } else if matches!(
        kind,
        PageKind::Province
            | PageKind::Procedure
            | PageKind::Auth
            | PageKind::Applicant
            | PageKind::ApplicantNoSlots
            | PageKind::RequestMenu
            | PageKind::RequestNoSlots
            | PageKind::RequestNoSlotsRestart
            | PageKind::PostContactNoSlots
    ) {
        *cache = None;
    }
}
#[cfg(test)]
mod office_memory_tests {
    use super::*;
    #[test]
    fn current_office_never_inherits_a_previous_pass_or_unresolved_choice() {
        let mut cached = Some("OLD OFFICE".to_owned());
        update_office_cache(&mut cached, PageKind::Province, None, &[]);
        assert!(cached.is_none());
        update_office_cache(&mut cached, PageKind::Office, None, &["ONE".into()]);
        assert_eq!(cached.as_deref(), Some("ONE"));
        update_office_cache(&mut cached, PageKind::Contact, None, &[]);
        assert_eq!(cached.as_deref(), Some("ONE"));
        update_office_cache(
            &mut cached,
            PageKind::Office,
            None,
            &["ONE".into(), "TWO".into()],
        );
        assert!(cached.is_none());
        update_office_cache(
            &mut cached,
            PageKind::Office,
            Some("TWO".into()),
            &["ONE".into(), "TWO".into()],
        );
        assert_eq!(cached.as_deref(), Some("TWO"));
        update_office_cache(&mut cached, PageKind::Applicant, None, &[]);
        assert!(cached.is_none());
    }
}

#[cfg(test)]
mod fresh_pass_tests {
    use super::fresh_pass_applicable;
    use ceh_core::model::PageKind;

    #[test]
    fn fresh_pass_only_runs_on_official_pages_that_are_not_captcha_or_booking() {
        assert!(fresh_pass_applicable(true, PageKind::ApplicantNoSlots));
        assert!(fresh_pass_applicable(true, PageKind::RequestNoSlots));
        assert!(!fresh_pass_applicable(true, PageKind::Captcha), "a captcha belongs to the person");
        assert!(!fresh_pass_applicable(true, PageKind::Booking), "a booking screen belongs to the person");
        assert!(!fresh_pass_applicable(false, PageKind::AuthWait), "the Cl@ve provider page is not the site");
        assert!(!fresh_pass_applicable(false, PageKind::Applicant));
    }
}
