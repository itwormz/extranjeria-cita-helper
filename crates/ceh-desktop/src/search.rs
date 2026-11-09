//! Native owned search worker. UI supplies profile IDs and controls, never page facts.
use ceh_browser::{
    DispatchGate,
    firefox::{FirefoxInstallation, OwnedFirefox, ProfileLease},
    session_driver::SessionDriver,
};
use ceh_core::{
    budget_commands::{BudgetCommand, apply_budget_command},
    control::ControlMode,
    flow::FlowEffect,
    journal::{JournalEvent, JournalMode, JournalRow, PageJournal},
    model::*,
    statistics::{CounterKind, Counters, EventScope},
};
use ceh_store::live::LiveStatistics;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
#[derive(Clone, Default, Serialize)]
pub struct SearchSnapshot {
    pub profile: Option<String>,
    pub active: bool,
    pub paused: bool,
    pub manual: bool,
    pub browser_pid: Option<u32>,
    pub page: Option<PageKind>,
    pub deadline: Option<i64>,
    pub attention: bool,
    pub error: Option<&'static str>,
    pub rows: Vec<JournalRow>,
    pub counters: Counters,
    pub revision: u64,
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Control {
    Pause,
    Resume,
    Stop,
}
/// Changes applied to a running search at the next tick.
enum LiveCommand {
    Budget(BudgetCommand),
    /// Start a new pass from the entry page in the same session (Try now).
    FreshPass,
    /// The person asked to continue searching after the booking handoff.
    Resume,
    Timings(ceh_core::timings::AssistTimings),
}
struct Message {
    command: Control,
    generation: u64,
}
struct Handle {
    sender: mpsc::SyncSender<Message>,
    live: mpsc::Sender<LiveCommand>,
    gate: Arc<Mutex<Option<DispatchGate>>>,
    cancelled: Arc<AtomicBool>,
}
#[derive(Default)]
pub struct SearchController {
    state: Arc<Mutex<SearchSnapshot>>,
    handle: Mutex<Option<Handle>>,
}
impl SearchController {
    pub fn snapshot(&self) -> Result<SearchSnapshot, &'static str> {
        self.state
            .lock()
            .map(|s| s.clone())
            .map_err(|_| "search_unavailable")
    }
    pub fn control(&self, command: Control) -> Result<(), &'static str> {
        let handle = self.handle.lock().map_err(|_| "search_unavailable")?;
        let h = handle.as_ref().ok_or("search_not_running")?;
        if !self.snapshot()?.active {
            return Err("search_not_running");
        }
        let generation = h
            .gate
            .lock()
            .map_err(|_| "search_unavailable")?
            .as_ref()
            .map(|g| g.interrupt())
            .transpose()
            .map_err(|_| "search_unavailable")?
            .unwrap_or(0);
        if matches!(command, Control::Stop) {
            h.cancelled.store(true, Ordering::Release);
        }
        h.sender
            .try_send(Message {
                command,
                generation,
            })
            .map_err(|_| "search_busy")
    }
    /// Budget commands act on the running search only; an idle controller refuses them.
    pub fn budget_command(&self, command: BudgetCommand) -> Result<(), &'static str> {
        // Refuse here, where the caller can be told, instead of silently dropping it in the worker.
        match &command {
            BudgetCommand::SetDailyTarget(Some(target)) if *target == 0 || *target > i32::MAX as u32 => {
                return Err("invalid_target");
            }
            BudgetCommand::TryNow if self.snapshot()?.manual => return Err("search_manual"),
            _ => {}
        }
        let try_now = command == BudgetCommand::TryNow;
        self.send_live(LiveCommand::Budget(command))?;
        if try_now {
            self.send_live(LiveCommand::FreshPass)?;
        }
        Ok(())
    }
    /// Continue searching after the date-selection handoff: only while the person owns the browser.
    pub fn resume_search(&self) -> Result<(), &'static str> {
        if !self.snapshot()?.manual {
            return Err("search_not_manual");
        }
        self.send_live(LiveCommand::Resume)
    }
    /// Re-read the active profile's saved timings and appointment policy and apply them to the run.
    pub fn refresh_settings(&self, root: &Path) -> Result<(), &'static str> {
        let id = self.snapshot()?.profile.ok_or("search_not_running")?;
        if !self.snapshot()?.active {
            return Err("search_not_running");
        }
        let bundle = ceh_store::profiles::read_profiles(&root.join("tasks.yml"), crate::catalog(), "ru")
            .map_err(|_| "profiles_unavailable")?;
        let loaded = bundle
            .profiles
            .into_iter()
            .find(|p| p.validation.normalized.id == id)
            .ok_or("profile_missing")?;
        if !loaded.validation.errors.is_empty() {
            return Err("profile_invalid");
        }
        let timings = loaded.timings.ok_or("profile_invalid")?;
        let policy = loaded.appointment_policy.ok_or("profile_invalid")?;
        self.send_live(LiveCommand::Timings(timings))?;
        self.send_live(LiveCommand::Budget(BudgetCommand::ApplyPolicy(policy)))
    }
    fn send_live(&self, command: LiveCommand) -> Result<(), &'static str> {
        let handle = self.handle.lock().map_err(|_| "search_unavailable")?;
        let h = handle.as_ref().ok_or("search_not_running")?;
        if !self.snapshot()?.active {
            return Err("search_not_running");
        }
        h.live.send(command).map_err(|_| "search_not_running")
    }
    pub fn start(
        &self,
        root: &Path,
        id: &str,
        window: tauri::WebviewWindow,
        sound: Arc<Mutex<crate::sound::SoundPlayback>>,
    ) -> Result<(), &'static str> {
        let mut handle = self.handle.lock().map_err(|_| "search_unavailable")?;
        if self.snapshot()?.active {
            return Err("search_already_running");
        }
        let bundle =
            ceh_store::profiles::read_profiles(&root.join("tasks.yml"), crate::catalog(), "ru")
                .map_err(|_| "profiles_unavailable")?;
        let language = bundle.preferences.language.clone();
        let loaded = bundle
            .profiles
            .into_iter()
            .find(|p| p.validation.normalized.id == id)
            .ok_or("profile_missing")?;
        if !loaded.validation.errors.is_empty() {
            return Err("profile_invalid");
        }
        if loaded.validation.normalized.browser != "firefox" {
            return Err("browser_unsupported");
        }
        FirefoxInstallation::discover().map_err(|_| "firefox_not_installed")?;
        let profile = loaded.validation.normalized;
        let timings = loaded.timings.ok_or("profile_invalid")?;
        let appointment_policy = loaded.appointment_policy.ok_or("profile_invalid")?;
        // Secrets of enabled channels are read now, before any browser opens; unreadable ones stop the start.
        let vault = ceh_services::secrets::Vault::new(ceh_services::secrets::native::NativeBackend::new());
        let (notification_config, prepared) = crate::notification_worker::prepare_for_start(root, id, &vault)?;
        let http = ceh_services::notifications::HttpDelivery::new().map_err(|_| "notification_transport_unavailable")?;
        let mut notifications = crate::notification_worker::LiveNotifications::start(
            crate::notification_worker::LiveSettings {
                profile_id: id.to_owned(),
                label: profile.label.clone(),
                province: profile.province.clone(),
                language,
                config: notification_config,
                browser: "Firefox".into(),
                run_id: uuid::Uuid::new_v4().to_string(),
            },
            prepared,
            move |intent, prepared| ceh_services::notifications::deliver_intent_prepared(intent, prepared, &http),
            |_, _| {},
        );
        let (sender, receiver) = mpsc::sync_channel(32);
        let (live, live_receiver) = mpsc::channel();
        let gate = Arc::new(Mutex::new(None));
        let cancelled = Arc::new(AtomicBool::new(false));
        let revision = self.snapshot()?.revision.wrapping_add(1);
        *self.state.lock().map_err(|_| "search_unavailable")? = SearchSnapshot {
            profile: Some(id.into()),
            active: true,
            revision,
            ..Default::default()
        };
        let state = self.state.clone();
        let root = root.to_owned();
        let worker_gate = gate.clone();
        let stop = cancelled.clone();
        std::thread::Builder::new()
            .name("ceh-firefox".into())
            .spawn(move || {
                let result = run(
                    &root,
                    profile,
                    timings,
                    appointment_policy,
                    &receiver,
                    &live_receiver,
                    &mut notifications,
                    &worker_gate,
                    &stop,
                    &state,
                    &window,
                    &sound,
                );
                let requested_stop = stop.load(Ordering::Acquire);
                if result.is_err() && !requested_stop {
                    notifications.observe(ceh_services::notifications::NotificationStateInput {
                        failed: true,
                        ..Default::default()
                    });
                }
                let enabled = ceh_store::profiles::read_profiles(
                    &root.join("tasks.yml"),
                    crate::catalog(),
                    "ru",
                )
                .is_ok_and(|b| b.preferences.sound);
                if let Ok(mut player) = sound.lock() {
                    let _ = crate::session_presentation::apply_native(
                        &window,
                        &root,
                        &mut player,
                        terminal_change(requested_stop, result.is_err(), enabled),
                    );
                }
                if let Ok(mut s) = state.lock() {
                    s.active = false;
                    s.paused = false;
                    s.deadline = None;
                    if let Err(code) = result
                        && !requested_stop
                    {
                        s.error = Some(code);
                        s.attention = true;
                    }
                    s.revision = s.revision.wrapping_add(1);
                }
            })
            .map_err(|_| "search_unavailable")?;
        *handle = Some(Handle {
            sender,
            live,
            gate,
            cancelled,
        });
        Ok(())
    }
}
impl Drop for SearchController {
    fn drop(&mut self) {
        if let Ok(handle) = self.handle.get_mut()
            && let Some(h) = handle.take()
        {
            h.cancelled.store(true, Ordering::Release);
            if let Ok(gate) = h.gate.lock()
                && let Some(gate) = gate.as_ref()
            {
                let _ = gate.interrupt();
            }
        }
    }
}
fn observation_retry_ms(attempt: u32) -> u64 {
    if attempt < 5 { 500 } else { 30_000 }
}
fn terminal_change(
    requested_stop: bool,
    failed: bool,
    sound: bool,
) -> crate::session_presentation::PresentationChange {
    use crate::session_presentation::{PresentationChange, SoundCommand};
    let error = failed && !requested_stop;
    PresentationChange {
        icon: Some(if error {
            crate::presentation::IconStatus::Error
        } else {
            crate::presentation::IconStatus::Idle
        }),
        sound: if error && sound {
            SoundCommand::Play
        } else {
            SoundCommand::Stop
        },
    }
}
fn apply_worker_control(
    driver: &mut SessionDriver,
    gate: &DispatchGate,
    message: &Message,
    now: u64,
) -> Result<(), &'static str> {
    match message.command {
        Control::Pause => {
            driver.pause(now).map_err(|_| "search_control_failed")?;
        }
        Control::Resume => {
            let snapshot = driver.resume(now).map_err(|_| "search_control_failed")?;
            let _ = gate.clear_interrupt(&snapshot, message.generation);
        }
        Control::Stop => {}
    }
    Ok(())
}
fn attention_state(
    previous: bool,
    page_changed: bool,
    manual: bool,
    effects: &[FlowEffect],
) -> bool {
    manual
        || effects.contains(&FlowEffect::Alert)
        || effects.contains(&FlowEffect::Hold)
        || (previous && !page_changed && !effects.contains(&FlowEffect::Silence))
}
fn unit_sample()->f64{
 let bytes=uuid::Uuid::new_v4();let mut first=[0u8;8];first.copy_from_slice(&bytes.as_bytes()[..8]);u64::from_le_bytes(first) as f64/u64::MAX as f64
}
fn wall() -> Result<i64, &'static str> {
    crate::current_time().map(|v| v.timestamp_millis())
}
fn update(
    state: &Mutex<SearchSnapshot>,
    f: impl FnOnce(&mut SearchSnapshot),
) -> Result<(), &'static str> {
    let mut s = state.lock().map_err(|_| "search_unavailable")?;
    f(&mut s);
    s.revision = s.revision.wrapping_add(1);
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn run(
    root: &Path,
    profile: ceh_core::profiles::ProfileDraft,
    timings: ceh_core::timings::AssistTimings,
    appointment_policy: ceh_core::appointment_budget::AppointmentPolicy,
    receiver: &mpsc::Receiver<Message>,
    live: &mpsc::Receiver<LiveCommand>,
    notifications: &mut crate::notification_worker::LiveNotifications,
    shared_gate: &Mutex<Option<DispatchGate>>,
    cancelled: &AtomicBool,
    state: &Mutex<SearchSnapshot>,
    window: &tauri::WebviewWindow,
    sound: &Mutex<crate::sound::SoundPlayback>,
) -> Result<(), &'static str> {
    let installation = FirefoxInstallation::discover().map_err(|_| "firefox_not_installed")?;
    let lease = ProfileLease::temporary(root).map_err(|_| "browser_profile_unavailable")?;
    let mut browser =
        OwnedFirefox::start(&installation, lease, false, cancelled).map_err(|error| {
            if error == ceh_browser::firefox::FirefoxError::CertificateCache {
                "browser_certificate_cache_failed"
            } else {
                "browser_start_failed"
            }
        })?;
    update(state, |s| s.browser_pid = browser.process_id())?;
    // Any unexpected return preserves the current browser, including a possible booking.
    let result = run_owned(
        root,
        &profile,
        &timings,
        &appointment_policy,
        receiver,
        live,
        notifications,
        shared_gate,
        cancelled,
        state,
        window,
        sound,
        &mut browser,
    );
    browser.handoff();
    if let Ok(mut player) = sound.lock() {
        let _ = player.stop();
    }
    result
}
#[allow(clippy::too_many_arguments)]
fn run_owned(
    root: &Path,
    profile: &ceh_core::profiles::ProfileDraft,
    timings: &ceh_core::timings::AssistTimings,
    appointment_policy: &ceh_core::appointment_budget::AppointmentPolicy,
    receiver: &mpsc::Receiver<Message>,
    live: &mpsc::Receiver<LiveCommand>,
    notifications: &mut crate::notification_worker::LiveNotifications,
    shared_gate: &Mutex<Option<DispatchGate>>,
    cancelled: &AtomicBool,
    state: &Mutex<SearchSnapshot>,
    window: &tauri::WebviewWindow,
    sound: &Mutex<crate::sound::SoundPlayback>,
    browser: &mut OwnedFirefox,
) -> Result<(), &'static str> {
    // Saved profile changes and budget commands replace these during the run.
    let mut timings = timings.clone();
    let mut appointment_policy = appointment_policy.clone();
    let session =
        SessionId::new(uuid::Uuid::new_v4().to_string()).map_err(|_| "search_unavailable")?;
    let mut gate = DispatchGate::new(session.clone());
    *shared_gate.lock().map_err(|_| "search_unavailable")? = Some(gate.clone());
    let mut driver = SessionDriver::with_samples(
        browser.context(),
        session.clone(),
        profile.clone(),
        timings.clone(),
        gate.clone(),
        ceh_browser::session_driver::SessionSamples{first_bootstrap:unit_sample(),second_bootstrap:unit_sample(),reload:unit_sample(),contact_typing:unit_sample()},
    )
    .map_err(|_| "profile_invalid")?;
    let database = root.join("data/profile-statistics.sqlite3");
    std::fs::create_dir_all(root.join("data")).map_err(|_| "statistics_unavailable")?;
    ceh_store::migrate::migrate_paths(
        &ceh_store::migrate::MigrationOptions {
            database: database.clone(),
            legacy_json: None,
            backup_directory: root.join("data/backups"),
        },
        wall()?,
    )
    .map_err(|_| "statistics_unavailable")?;
    let mut db = rusqlite::Connection::open(database).map_err(|_| "statistics_unavailable")?;
    db.busy_timeout(Duration::from_secs(2))
        .map_err(|_| "statistics_unavailable")?;
    let mut budget_store = ceh_store::appointment_budget::BudgetStore::open(&root.join("data"), profile)
        .map_err(|_| "appointment_budget_unavailable")?;
    let mut budget = budget_store.load().map_err(|_| "appointment_budget_unavailable")?;
    let mut copied_budget_identity = None;
    let mut initial_budget_store = None;
    // Round state of the budget: banners, offices, awaited request result and the one-time hot retry.
    let mut observer = ceh_core::budget_observer::BudgetObserver::default();
    let clock = Instant::now();
    let mono = || clock.elapsed().as_millis().min(u64::MAX as u128) as u64;
    let mut stats = LiveStatistics::start(
        &mut db,
        EventScope {
            profile: ProfileId::new(&profile.id).map_err(|_| "profile_invalid")?,
            session: session.clone(),
        },
        &json!({"procedure":profile.procedure,"province":profile.province,"browser":"firefox"}),
        wall()?,
        mono(),
    )
    .map_err(|_| "statistics_unavailable")?;
    let mut journal = PageJournal::new();
    journal
        .event(JournalEvent::Start, None, None, false, wall()?)
        .map_err(|_| "search_unavailable")?;
    let mut diagnostics = crate::search_diagnostics::SearchDiagnostics::new(root, session.as_str().to_owned(), profile);
    let mut presenter = crate::session_presentation::SessionPresentation::new(session, 3000);
    let mut sequence = 0u64;
    let context = browser.context().to_owned();
    if cancelled.load(Ordering::Acquire) {
        stats
            .finish(&mut db, wall()?, mono(), "stopped")
            .map_err(|_| "statistics_unavailable")?;
        return Ok(());
    }
    let initial_preferences = ceh_store::profiles::read_profiles(&root.join("tasks.yml"), crate::catalog(), "ru").ok();
    diagnostics.set_enabled(initial_preferences.as_ref().is_some_and(|bundle| bundle.preferences.diagnostics),
        browser.connection(), &mut driver, &context);
    // Only the new owned about:blank context receives this initial GET.
    browser.connection().call("browsingContext.navigate",json!({"context":context,"url":"https://icp.administracionelectronica.gob.es/icpplus/index.html","wait":"none"})).map_err(|_|"browser_navigation_failed")?;
    let result = (|| {
        let mut wait = false;
        let mut manual = false;
        let mut operation = 0u64;
        let mut read_failures = 0u32;
        let mut retry_at = Instant::now();
        let mut preferences_at = Instant::now() - Duration::from_secs(3);
        let mut sound_enabled = false;
        loop {
            while let Ok(message) = receiver.try_recv() {
                match message.command {
                    Control::Stop => return Ok(()),
                    Control::Pause => {
                        if driver.control().mode() != ControlMode::Manual {
                            apply_worker_control(&mut driver, &gate, &message, mono())?;
                            stats
                                .set_phase(&mut db, "paused", wall()?, mono())
                                .map_err(|_| "statistics_unavailable")?;
                            journal
                                .event(JournalEvent::Pause, None, None, true, wall()?)
                                .map_err(|_| "search_unavailable")?;
                        }
                    }
                    Control::Resume => {
                        if driver.control().mode() != ControlMode::Manual {
                            apply_worker_control(&mut driver, &gate, &message, mono())?;
                            stats
                                .set_phase(&mut db, "working", wall()?, mono())
                                .map_err(|_| "statistics_unavailable")?;
                            journal
                                .event(JournalEvent::Resume, None, None, true, wall()?)
                                .map_err(|_| "search_unavailable")?;
                        }
                    }
                }
            }
            while let Ok(command) = live.try_recv() {
                match command {
                    LiveCommand::Budget(command) => {
                        // Try now is refused while the person owns the browser (booking handoff).
                        if manual && command == BudgetCommand::TryNow {
                            continue;
                        }
                        let now = crate::current_time()?;
                        if apply_budget_command(&mut budget, &mut appointment_policy, command, now).is_ok() {
                            budget_store.save(&budget).map_err(|_| "appointment_budget_unavailable")?;
                        }
                    }
                    LiveCommand::Resume => {
                        // Take the same browser back with a new control session; cookies and the site session stay.
                        // A second request finds `manual` already false and is dropped.
                        if manual && browser.resume_automation().is_ok() {
                            let next = SessionId::new(uuid::Uuid::new_v4().to_string()).map_err(|_| "search_unavailable")?;
                            gate = DispatchGate::new(next.clone());
                            *shared_gate.lock().map_err(|_| "search_unavailable")? = Some(gate.clone());
                            driver = SessionDriver::with_samples(
                                &context,
                                next.clone(),
                                profile.clone(),
                                timings.clone(),
                                gate.clone(),
                                ceh_browser::session_driver::SessionSamples {
                                    first_bootstrap: unit_sample(),
                                    second_bootstrap: unit_sample(),
                                    reload: unit_sample(),
                                    contact_typing: unit_sample(),
                                },
                            )
                            .map_err(|_| "profile_invalid")?;
                            presenter = crate::session_presentation::SessionPresentation::new(next, 3000);
                            browser
                                .connection()
                                .call(
                                    "browsingContext.navigate",
                                    json!({"context": context, "url": "https://icp.administracionelectronica.gob.es/icpplus/index.html", "wait": "none"}),
                                )
                                .map_err(|_| "browser_resume_failed")?;
                            manual = false;
                            wait = false;
                            read_failures = 0;
                            retry_at = Instant::now();
                            observer = ceh_core::budget_observer::BudgetObserver::default();
                            update(state, |s| {
                                s.manual = false;
                                s.attention = false;
                                s.page = None;
                                s.deadline = None;
                            })?;
                            stats
                                .set_phase(&mut db, "working", wall()?, mono())
                                .map_err(|_| "statistics_unavailable")?;
                            journal
                                .event(JournalEvent::Resume, None, None, true, wall()?)
                                .map_err(|_| "search_unavailable")?;
                        }
                    }
                    LiveCommand::FreshPass => {
                        if !manual {
                            driver.request_fresh_pass();
                        }
                    }
                    LiveCommand::Timings(next) => {
                        let samples = ceh_browser::session_driver::SessionSamples {
                            first_bootstrap: unit_sample(),
                            second_bootstrap: unit_sample(),
                            reload: unit_sample(),
                            contact_typing: unit_sample(),
                        };
                        if driver.apply_timings(next.clone(), samples).is_ok() {
                            timings = next;
                        }
                    }
                }
            }
            if cancelled.load(Ordering::Acquire) {
                return Ok(());
            }
            let paused = driver.control().mode() == ControlMode::Paused;
            let mut effects = Vec::new();
            if !paused && !manual && Instant::now() < retry_at {
                stats
                    .heartbeat(&mut db, wall()?, mono())
                    .map_err(|_| "statistics_unavailable")?;
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            if !paused && !manual {
                let budget_now = crate::current_time()?;
                let ready = budget.next_submission(budget_now, &appointment_policy, observer.hot_retry());
                let delay = (ready - budget_now).num_milliseconds().max(0) as u64;
                driver.set_appointment_schedule(mono().saturating_add(delay), appointment_policy.confirm_new_round);
                let step = match driver.plan(browser.connection(), mono()) {
                    Ok(step) => {
                        read_failures = 0;
                        step
                    }
                    Err(ceh_browser::BrowserError::NoObservation)
                        if browser.connection().available() =>
                    {
                        // No action has been planned: a destroyed navigation realm can be read again.
                        stats
                            .set_phase(&mut db, "unobserved", wall()?, mono())
                            .map_err(|_| "statistics_unavailable")?;
                        read_failures = read_failures.saturating_add(1);
                        let delay = observation_retry_ms(read_failures);
                        retry_at = Instant::now() + Duration::from_millis(delay);
                        let deadline = wall()?.saturating_add(delay as i64);
                        update(state, |s| s.deadline = Some(deadline))?;
                        continue;
                    }
                    Err(_) => return Err("browser_observation_failed"),
                };
                let before = step.update();
                diagnostics.observe(&step, browser.connection(), &mut driver);
                let at = wall()?;
                let now = mono();
                if before.control.mode() == ControlMode::Manual {
                    browser.handoff();
                    manual = true;
                }
                if before.preparing{stats.preparation_page(&mut db,&before.observation,ControlMode::Automatic,at,now)}else{stats.page(&mut db,&before.observation,&before.offices,ControlMode::Automatic,at,now)}.map_err(|_|"statistics_unavailable")?;
                journal
                    .observe(
                        before.observation.document.clone(),
                        before.observation.kind,
                        profile.applicant_no_slots_mode == "continue",
                        at,
                    )
                    .map_err(|_| "search_unavailable")?;
                {
                    let page = ceh_core::budget_observer::ObservedPage {
                        kind: before.observation.kind,
                        operation_code: before.observation.operation_code,
                    };
                    let mut changed = observer.observe(&page, &mut budget, budget_now, &appointment_policy);
                    if before.observation.kind == PageKind::Office {
                        changed |= observer.offices_seen(&mut budget, budget_now, before.offices.clone());
                    }
                    if before.observation.kind == PageKind::Province {
                        observer.on_province();
                    }
                    if changed {
                        budget_store.save(&budget).map_err(|_| "appointment_budget_unavailable")?;
                    }
                }
                if !before.offices.is_empty() {
                    journal.offices(before.offices.clone(), None);
                }
                if matches!(before.observation.kind, PageKind::Blocked | PageKind::RateLimited) {
                    browser.handoff();
                    update(state, |s| s.page = Some(before.observation.kind))?;
                    return Err("browser_access_limited");
                }
                if std::env::var("CEH_PREVIEW_FIREFOX").as_deref() == Ok("applicant")
                    && matches!(before.observation.kind, PageKind::Applicant | PageKind::ApplicantNoSlots)
                {
                    browser.handoff();
                    update(state, |s| s.page = Some(before.observation.kind))?;
                    return Ok(());
                }
                effects = before.effects.clone();
                let deadline = effects
                    .iter()
                    .filter_map(|e| match e {
                        FlowEffect::WaitUntil(v) => {
                            Some(at + v.saturating_sub(now).min(i64::MAX as u64) as i64)
                        }
                        _ => None,
                    })
                    .min();
                if deadline.is_some() {
                    wait = true;
                } else if !effects.is_empty() {
                    wait = false;
                }
                let previous = state.lock().map_err(|_| "search_unavailable")?.clone();
                let attention = attention_state(
                    previous.attention,
                    previous.page != Some(before.observation.kind),
                    manual,
                    &effects,
                );
                if effects.contains(&FlowEffect::Silence) {
                    update(state, |s| s.attention = false)?;
                }
                update(state, |s| {
                    s.page = Some(before.observation.kind);
                    s.manual = manual;
                    s.attention = attention;
                    if deadline.is_some() {
                        s.deadline = deadline;
                    } else if !wait {
                        s.deadline = None;
                    }
                })?;
                // Telegram's booking text waits for the public details (read right after the handoff below).
                notifications.observe(ceh_services::notifications::NotificationStateInput {
                    document: Some(before.observation.document.as_str().to_owned()),
                    page: before.observation.kind,
                    attention,
                    booking_handoff: manual,
                    paused,
                    offices: before.offices.clone(),
                    booking_details_pending: manual,
                    ..Default::default()
                });
                // UI/store/handoff already published before actions or optional public enrichment.
                if effects.iter().any(|e| {
                    matches!(
                        e,
                        FlowEffect::Prepare { .. }
                            | FlowEffect::Activate { .. }
                            | FlowEffect::EnterAuthentication { .. }
                            | FlowEffect::SelectCertificate { .. }
                            | FlowEffect::SelectClaveMethod { .. }
                            | FlowEffect::AcceptCookies { .. }
                    )
                }) {
                    let bytes = uuid::Uuid::new_v4();
                    let sample = u64::from_le_bytes(
                        bytes.as_bytes()[..8]
                            .try_into()
                            .map_err(|_| "search_unavailable")?,
                    ) as f64
                        / u64::MAX as f64;
                    let until =
                        Instant::now() + Duration::from_secs_f64(timings.sample(ceh_core::timings::TimingKind::Action,sample).map_err(|_|"profile_invalid")?);
                    while Instant::now() < until && !cancelled.load(Ordering::Acquire) {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    if cancelled.load(Ordering::Acquire) {
                        return Ok(());
                    }
                }
                let entering_auth = effects.iter().any(|effect| matches!(effect, FlowEffect::EnterAuthentication { .. }));
                let submits_appointment = effects.iter().any(|effect| matches!(effect, FlowEffect::Activate { action: ActionKind::Request, .. }));
                let prior_budget = if submits_appointment {
                    let prior = (budget.clone(), observer.clone());
                    observer.request_sent(&mut budget, crate::current_time()?, &appointment_policy);
                    budget_store.save(&budget).map_err(|_| "appointment_budget_unavailable")?;
                    Some(prior)
                } else {
                    None
                };
                let after = match driver.execute(browser.connection(), step) {
                    Ok(after) => after,
                    Err(ceh_browser::BrowserError::Paused) => {
                        continue;
                    }
                    Err(
                        ceh_browser::BrowserError::StaleDocument
                        | ceh_browser::BrowserError::NoObservation,
                    ) if browser.connection().available() => {
                        if entering_auth {
                            return Err("browser_auth_entry_changed");
                        }
                        // Do not replay this linear plan. Only a fresh observation may advance core.
                        update(state, |s| s.attention = true)?;
                        std::thread::sleep(Duration::from_millis(500));
                        continue;
                    }
                    Err(_) => return Err("browser_action_failed"),
                };
                if profile.fill_mode == "copy" && after.applicant_prepared {
                    let identity = driver.copied_budget_identity(browser.connection(), &after.observation)
                        .map_err(|_| "appointment_budget_unavailable")?;
                    if copied_budget_identity.as_ref() != Some(&identity) {
                        let next = ceh_store::appointment_budget::BudgetStore::open_for_document(
                            &root.join("data"), profile, Some((&identity.0, &identity.1)),
                        ).map_err(|_| "appointment_budget_unavailable")?;
                        let before = std::mem::replace(
                            &mut budget,
                            next.load().map_err(|_| "appointment_budget_unavailable")?,
                        );
                        let now = crate::current_time()?;
                        budget.carry_today_choices_from(&before, now, &appointment_policy);
                        next.save(&budget).map_err(|_| "appointment_budget_unavailable")?;
                        let previous = std::mem::replace(&mut budget_store, next);
                        if initial_budget_store.is_none() {
                            initial_budget_store = Some(previous);
                        }
                        copied_budget_identity = Some(identity);
                    }
                }
                if let Some((prior_budget, prior_observer)) = prior_budget {
                    // The request was counted before it was sent; an undelivered click gives the attempt back.
                    if !after.receipts.iter().any(|receipt| receipt.action == ActionKind::Request && receipt.outcome != DispatchOutcome::NotDispatched) {
                        budget = prior_budget;
                        observer = prior_observer;
                        budget_store.save(&budget).map_err(|_| "appointment_budget_unavailable")?;
                    }
                }
                if entering_auth {
                    match after.receipts.iter().find(|receipt| receipt.action == ActionKind::Auth).map(|receipt| receipt.outcome) {
                        Some(DispatchOutcome::Dispatched) => {}
                        Some(DispatchOutcome::Unknown) => return Err("browser_auth_entry_unconfirmed"),
                        _ => return Err("browser_auth_entry_unavailable"),
                    }
                }
                if manual {
                    let details =
                        serde_json::to_value(&after.details).map_err(|_| "search_unavailable")?;
                    stats
                        .enrich_booking(&mut db, &after.observation.document, &details)
                        .map_err(|_| "statistics_unavailable")?;
                    journal.booking(&after.observation.document, &details);
                    notifications.observe(ceh_services::notifications::NotificationStateInput {
                        document: Some(after.observation.document.as_str().to_owned()),
                        page: after.observation.kind,
                        attention: true,
                        booking_handoff: true,
                        booking_details: Some(details.clone()),
                        booking_details_pending: false,
                        ..Default::default()
                    });
                    stats
                        .set_phase(&mut db, "booking_handoff", wall()?, mono())
                        .map_err(|_| "statistics_unavailable")?;
                }
                for receipt in &after.receipts {
                    if receipt.outcome != DispatchOutcome::Dispatched {
                        continue;
                    }
                    operation = operation.checked_add(1).ok_or("search_unavailable")?;
                    let id = OperationId::new(format!("action-{operation}"))
                        .map_err(|_| "search_unavailable")?;
                    stats
                        .operation(
                            &mut db,
                            CounterKind::AutomationAction,
                            id.clone(),
                            ControlMode::Automatic,
                            wall()?,
                        )
                        .map_err(|_| "statistics_unavailable")?;
                    let check = matches!(
                        receipt.action,
                        ActionKind::Request
                            | ActionKind::RequestReload
                            | ActionKind::ApplicantReload
                    );
                    if check {
                        stats
                            .operation(
                                &mut db,
                                CounterKind::Check,
                                id,
                                ControlMode::Automatic,
                                wall()?,
                            )
                            .map_err(|_| "statistics_unavailable")?;
                    }
                    journal
                        .event(
                            if check {
                                JournalEvent::Check
                            } else {
                                JournalEvent::ActionCompleted
                            },
                            Some(receipt.document.clone()),
                            Some(after.observation.kind),
                            false,
                            wall()?,
                        )
                        .map_err(|_| "search_unavailable")?;
                }
                if effects.iter().any(|e| {
                    matches!(
                        e,
                        FlowEffect::Recover {
                            action: ceh_core::recovery::RecoveryAction::Restart { .. }
                                | ceh_core::recovery::RecoveryAction::GetCurrent
                                | ceh_core::recovery::RecoveryAction::Reload,
                            ..
                        }
                    )
                }) {
                    return Err("browser_recovery_required");
                }
                let phase = if manual {
                    "booking_handoff"
                } else if attention {
                    "manual"
                } else if wait {
                    "waiting"
                } else {
                    "working"
                };
                stats
                    .set_phase(&mut db, phase, wall()?, mono())
                    .map_err(|_| "statistics_unavailable")?;
                journal
                    .phase(
                        if attention {
                            JournalMode::Manual
                        } else if wait {
                            JournalMode::Waiting
                        } else {
                            JournalMode::Working
                        },
                        deadline.map(|v| (v - at).max(0) as f64 / 1000.0),
                        wall()?,
                    )
                    .map_err(|_| "search_unavailable")?;
            } else {
                stats
                    .heartbeat(&mut db, wall()?, mono())
                    .map_err(|_| "statistics_unavailable")?;
            }
            update(state, |s| {
                s.paused = paused;
                s.rows = journal.rows().to_vec();
                s.counters = stats.counters();
            })?;
            sequence = sequence.checked_add(1).ok_or("search_unavailable")?;
            if preferences_at.elapsed() >= Duration::from_secs(3) {
                if let Ok(bundle) = ceh_store::profiles::read_profiles(&root.join("tasks.yml"), crate::catalog(), "ru") {
                    sound_enabled = bundle.preferences.sound;
                    diagnostics.set_enabled(bundle.preferences.diagnostics, browser.connection(), &mut driver, &context);
                }
                preferences_at = Instant::now();
            }
            let change = presenter
                .update(
                    &driver.control(),
                    sequence,
                    mono(),
                    crate::session_presentation::PresentationFrame {
                        active: true,
                        browser_retained: manual,
                        sound_enabled,
                        repeat_alert: !matches!(
                            state.lock().map_err(|_| "search_unavailable")?.page,
                            Some(PageKind::AuthWait | PageKind::Loading)
                        ),
                        ..Default::default()
                    },
                    &effects,
                )
                .map_err(|_| "search_control_failed")?;
            if let Ok(mut player) = sound.lock() {
                let _ =
                    crate::session_presentation::apply_native(window, root, &mut player, change);
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    })();
    diagnostics.stop_collector(browser.connection(), &mut driver);
    let at = wall()?;
    let reason = if result.is_err() { "error" } else { "stopped" };
    stats
        .finish(&mut db, at, mono(), reason)
        .map_err(|_| "statistics_unavailable")?;
    journal
        .event(JournalEvent::Finish, None, None, true, at)
        .map_err(|_| "search_unavailable")?;
    update(state, |s| {
        s.rows = journal.rows().to_vec();
        s.counters = stats.counters();
    })?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn held_attention_does_not_restart_active_time_on_empty_polls() {
        assert!(attention_state(false, false, false, &[FlowEffect::Alert]));
        assert!(attention_state(true, false, false, &[]));
        assert!(!attention_state(true, true, false, &[]));
        assert!(!attention_state(true, false, false, &[FlowEffect::Silence]));
        assert!(attention_state(false, true, true, &[]));
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    #[test]
    fn superseded_resume_does_not_abort_newer_pause() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/contracts/fixtures/profile-validation.json"
        ))
        .unwrap();
        let profile =
            serde_json::from_value(fixture["cases"][0]["expected"]["normalized"].clone()).unwrap();
        let session = SessionId::new("control-race").unwrap();
        let gate = DispatchGate::new(session.clone());
        let mut driver = SessionDriver::new(
            "context",
            session,
            profile,
            Default::default(),
            gate.clone(),
        )
        .unwrap();
        let first = gate.interrupt().unwrap();
        let second = gate.interrupt().unwrap();
        let third = gate.interrupt().unwrap();
        apply_worker_control(
            &mut driver,
            &gate,
            &Message {
                command: Control::Pause,
                generation: first,
            },
            10,
        )
        .unwrap();
        apply_worker_control(
            &mut driver,
            &gate,
            &Message {
                command: Control::Resume,
                generation: second,
            },
            20,
        )
        .unwrap();
        apply_worker_control(
            &mut driver,
            &gate,
            &Message {
                command: Control::Pause,
                generation: third,
            },
            30,
        )
        .unwrap();
        assert_eq!(driver.control().mode(), ControlMode::Paused);
        let fourth = gate.interrupt().unwrap();
        apply_worker_control(
            &mut driver,
            &gate,
            &Message {
                command: Control::Resume,
                generation: fourth,
            },
            40,
        )
        .unwrap();
        assert_eq!(driver.control().mode(), ControlMode::Automatic);
    }
    #[test]
    fn observation_retries_slow_down_and_terminal_failure_is_visible() {
        assert_eq!(observation_retry_ms(1), 500);
        assert_eq!(observation_retry_ms(5), 30_000);
        assert_eq!(observation_retry_ms(u32::MAX), 30_000);
        use crate::session_presentation::SoundCommand;
        let failure = terminal_change(false, true, true);
        assert_eq!(failure.icon, Some(crate::presentation::IconStatus::Error));
        assert_eq!(failure.sound, SoundCommand::Play);
        assert_eq!(terminal_change(true, true, true).sound, SoundCommand::Stop);
        assert_eq!(
            terminal_change(false, true, false).sound,
            SoundCommand::Stop
        );
    }
}

#[cfg(test)]
mod live_command_tests {
    use super::*;

    fn running(manual: bool) -> (SearchController, mpsc::Receiver<LiveCommand>, mpsc::Receiver<Message>) {
        let controller = SearchController::default();
        controller.state.lock().unwrap().active = true;
        controller.state.lock().unwrap().manual = manual;
        let (sender, messages) = mpsc::sync_channel(1);
        let (live, commands) = mpsc::channel();
        *controller.handle.lock().unwrap() = Some(Handle {
            sender,
            live,
            gate: Arc::new(Mutex::new(None)),
            cancelled: Arc::new(AtomicBool::new(false)),
        });
        (controller, commands, messages)
    }

    #[test]
    fn invalid_daily_targets_are_refused_before_they_reach_the_worker() {
        let (controller, commands, _messages) = running(false);
        for bad in [0, i32::MAX as u32 + 1, u32::MAX] {
            assert_eq!(controller.budget_command(BudgetCommand::SetDailyTarget(Some(bad))), Err("invalid_target"));
        }
        assert!(commands.try_recv().is_err());
        assert_eq!(controller.budget_command(BudgetCommand::SetDailyTarget(Some(i32::MAX as u32))), Ok(()));
        assert!(commands.try_recv().is_ok());
    }

    #[test]
    fn try_now_also_requests_a_fresh_pass_after_the_budget_command() {
        let (controller, commands, _messages) = running(false);
        assert_eq!(controller.budget_command(BudgetCommand::TryNow), Ok(()));
        assert!(matches!(commands.try_recv(), Ok(LiveCommand::Budget(BudgetCommand::TryNow))));
        assert!(matches!(commands.try_recv(), Ok(LiveCommand::FreshPass)));
        assert!(commands.try_recv().is_err());
    }

    #[test]
    fn resume_is_refused_unless_the_person_owns_the_browser() {
        let (controller, commands, _messages) = running(false);
        assert_eq!(controller.resume_search(), Err("search_not_manual"));
        assert!(commands.try_recv().is_err());
        let (controller, commands, _messages) = running(true);
        assert_eq!(controller.resume_search(), Ok(()));
        assert!(matches!(commands.try_recv(), Ok(LiveCommand::Resume)));
        let idle = SearchController::default();
        assert_eq!(idle.resume_search(), Err("search_not_manual"));
    }

    #[test]
    fn try_now_is_refused_while_the_person_owns_the_browser() {
        let (controller, commands, _messages) = running(true);
        assert_eq!(controller.budget_command(BudgetCommand::TryNow), Err("search_manual"));
        assert!(commands.try_recv().is_err());
        let (controller, commands, _messages) = running(false);
        assert_eq!(controller.budget_command(BudgetCommand::TryNow), Ok(()));
        assert!(commands.try_recv().is_ok());
    }
}
