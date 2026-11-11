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
    /// A non-fatal notice for the window (the search keeps running).
    pub warning: Option<&'static str>,
    /// Id of the startup report saved for a failed start (`CEH-<id>`); the window offers to copy it.
    pub report_id: Option<String>,
    pub rows: Vec<JournalRow>,
    pub counters: Counters,
    /// Today's budget (Python `summary()` keys) while a search runs; the window shows it and its wait reason.
    pub appointment_budget: Option<ceh_core::appointment_budget::BudgetSummary>,
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
/// What closing the window requires (Python `closeEvent`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClosePlan {
    /// No search runs: the window closes at once.
    CloseNow,
    /// The app's own automation stops and its browser is closed, then the window closes.
    StopAndCloseBrowser,
    /// A browser handed to the person (a booking in progress) is never closed by the app.
    StopKeepBrowser,
}
pub fn close_plan(active: bool, manual: bool) -> ClosePlan {
    match (active, manual) {
        (false, _) => ClosePlan::CloseNow,
        (true, false) => ClosePlan::StopAndCloseBrowser,
        (true, true) => ClosePlan::StopKeepBrowser,
    }
}
#[derive(Default)]
pub struct SearchController {
    state: Arc<Mutex<SearchSnapshot>>,
    handle: Mutex<Option<Handle>>,
    /// Set when the window closes during an automatic search: its browser is closed with the search.
    close_browser: Arc<AtomicBool>,
}
impl SearchController {
    pub fn snapshot(&self) -> Result<SearchSnapshot, &'static str> {
        self.state
            .lock()
            .map(|s| s.clone())
            .map_err(|_| "search_unavailable")
    }
    /// Prepare to close the window: stops a running search (closing its browser unless it was handed to
    /// the person) and tells the caller whether to wait for the search thread to end.
    pub fn shutdown(&self) -> ClosePlan {
        let Ok(snapshot) = self.snapshot() else { return ClosePlan::CloseNow };
        let plan = close_plan(snapshot.active, snapshot.manual);
        if plan == ClosePlan::StopAndCloseBrowser {
            self.close_browser.store(true, Ordering::Release);
        }
        if plan != ClosePlan::CloseNow {
            let _ = self.control(Control::Stop);
        }
        plan
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
    /// Start a new pass from the entry page in the running browser session (Python `restart`).
    pub fn restart(&self) -> Result<(), &'static str> {
        self.send_live(LiveCommand::FreshPass)
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
        match Engine::of(&loaded.validation.normalized.browser) {
            Some(Engine::Firefox) => {
                FirefoxInstallation::discover().map_err(|_| "firefox_not_installed")?;
            }
            Some(Engine::Chromium(kind)) => {
                ceh_browser::chromium::ChromiumInstallation::discover(kind).map_err(|_| "browser_not_installed")?;
            }
            None => return Err("browser_unsupported"),
        }
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
        self.close_browser.store(false, Ordering::Release);
        let close_browser = self.close_browser.clone();
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
                    &close_browser,
                );
                let requested_stop = stop.load(Ordering::Acquire);
                let busy = result == Err("browser_profile_busy");
                if let Err(code) = &result
                    && !requested_stop
                    && !busy
                {
                    let home = std::env::var("HOME").unwrap_or_default();
                    let found = FirefoxInstallation::discover().is_ok();
                    let id = crate::current_time()
                        .ok()
                        .and_then(|now| crate::startup_report::save_failure(&root, &home, code, found, now));
                    let _ = update(&state, |s| s.report_id = id);
                }
                if result.is_err() && !requested_stop && !busy {
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
                let _ = clear_budget(&state);
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
/// Whole days since the app created its persistent browser profile of `key`, when known.
fn profile_age_days(root: &Path, key: &str, browser: &str) -> Option<u64> {
    let prefix = match Engine::of(browser) {
        Some(Engine::Chromium(ceh_browser::firefox::ProfileKind::Brave)) => "brave",
        Some(Engine::Chromium(_)) => "chrome",
        _ => "firefox",
    };
    let marker = root.join("browser-profiles").join(format!("{prefix}-{key}")).join("ceh-owned-profile");
    let created = std::fs::metadata(marker).ok()?.created().ok()?;
    Some(std::time::SystemTime::now().duration_since(created).ok()?.as_secs() / 86_400)
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
/// Publish today's budget summary to the window (called every tick of the search loop).
fn publish_budget(
    state: &Mutex<SearchSnapshot>,
    budget: &mut ceh_core::appointment_budget::AppointmentBudget,
    now: chrono::DateTime<chrono::Utc>,
    policy: &ceh_core::appointment_budget::AppointmentPolicy,
    hot: bool,
) -> Result<(), &'static str> {
    let summary = budget.summary(now, policy, hot);
    update(state, |s| s.appointment_budget = Some(summary))
}
fn clear_budget(state: &Mutex<SearchSnapshot>) -> Result<(), &'static str> {
    update(state, |s| s.appointment_budget = None)
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
/// The browser a search profile drives: Firefox over WebDriver BiDi, Chrome or Brave over CDP.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Firefox,
    Chromium(ceh_browser::firefox::ProfileKind),
}
impl Engine {
    pub fn of(browser: &str) -> Option<Self> {
        use ceh_browser::firefox::ProfileKind;
        match browser {
            "firefox" => Some(Self::Firefox),
            "chrome" => Some(Self::Chromium(ProfileKind::Chrome)),
            "brave" => Some(Self::Chromium(ProfileKind::Brave)),
            _ => None,
        }
    }
    pub fn kind(self) -> ceh_browser::firefox::ProfileKind {
        match self {
            Self::Firefox => ceh_browser::firefox::ProfileKind::Firefox,
            Self::Chromium(kind) => kind,
        }
    }
}
/// How one browser session ended: for good, or with a restart the core asked for (`Recover{Restart}`).
enum SessionEnd {
    Finished,
    NewSession(crate::restart::RestartRequest),
}
/// The search of one start: browser sessions one after another until the person stops it. A block (profile
/// cleared) or a Cl@ve error ends a session and starts the next one, as Python `run_assist` does.
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
    close_browser: &AtomicBool,
) -> Result<(), &'static str> {
    // The journal and the recovery counters belong to the whole search, not to one browser.
    let mut journal = PageJournal::new();
    let started_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    // The rows are kept between runs so the Work tab can show the profile's earlier log.
    let mut log = crate::journal_log::JournalLog::open(root, &profile.id, started_ms);
    let mut carry = ceh_core::flow::RecoveryCarry::default();
    loop {
        let end = run_session(
            root, profile.clone(), timings.clone(), appointment_policy.clone(), receiver, live, notifications,
            shared_gate, cancelled, state, window, sound, close_browser, &mut journal, &mut log, &mut carry,
        )?;
        match end {
            SessionEnd::Finished => return Ok(()),
            SessionEnd::NewSession(request) => {
                // The core waited the cooldown before asking (Python `cooldown_completed`): no further delay.
                if cancelled.load(Ordering::Acquire) {
                    return Ok(());
                }
                let _ = request;
                update(state, |s| {
                    s.page = None;
                    s.manual = false;
                    s.attention = false;
                    s.browser_pid = None;
                    s.paused = false;
                    s.deadline = None;
                })?;
            }
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn run_session(
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
    close_browser: &AtomicBool,
    journal: &mut PageJournal,
    log: &mut Option<crate::journal_log::JournalLog>,
    carry: &mut ceh_core::flow::RecoveryCarry,
) -> Result<SessionEnd, &'static str> {
    let engine = Engine::of(&profile.browser).ok_or("browser_unsupported")?;
    // One persistent browser profile per search profile, as in Python: cookies survive between runs and
    // "reset session" deletes it.
    let lease = ProfileLease::persistent_kind(root, &profile.id, engine.kind()).map_err(|error| match error {
        // The previous search's browser is still open (kept for the person, or its window just closed).
        ceh_browser::firefox::FirefoxError::ProfileBusy => "browser_profile_busy",
        _ => "browser_profile_unavailable",
    })?;
    let mut browser = match engine {
        Engine::Firefox => {
            let installation = FirefoxInstallation::discover().map_err(|_| "firefox_not_installed")?;
            OwnedFirefox::start(&installation, lease, false, cancelled).map_err(|error| {
                if error == ceh_browser::firefox::FirefoxError::CertificateCache {
                    "browser_certificate_cache_failed"
                } else {
                    "browser_start_failed"
                }
            })?
        }
        Engine::Chromium(kind) => {
            let installation = ceh_browser::chromium::ChromiumInstallation::discover(kind).map_err(|_| "browser_not_installed")?;
            OwnedFirefox::start_chromium(&installation, lease, cancelled).map_err(|_| "browser_start_failed")?
        }
    };
    update(state, |s| {
        s.browser_pid = browser.process_id();
        s.warning = (browser.certificate_cache() == ceh_browser::firefox::CertificateCache::Deferred)
            .then_some("browser_certificate_cache_deferred");
    })?;
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
        journal,
        log,
        carry,
    );
    let result = crate::closed_window_stop(result, SessionEnd::Finished, || browser.page_open());
    // Window closing during an automatic search closes the app's own browser; anything else (a stop, an
    // error, a booking handed over) keeps it open for the person.
    // A page the person closed leaves nothing to preserve: quit the browser so its profile is free again.
    let restart = match &result {
        Ok(SessionEnd::NewSession(request)) => Some(request.clone()),
        _ => None,
    };
    let closed = if restart.is_some() {
        // A new session replaces this browser: close it (the core never restarts while the person owns it).
        browser.stop().is_ok()
    } else {
        (close_browser.load(Ordering::Acquire) && browser.stop().is_ok()) || browser.quit_if_page_gone()
    };
    if !closed {
        browser.handoff();
    }
    if let Some(request) = &restart {
        drop(browser);
        if request.clear_profile {
            // Python `reset_browser_profile`: a blocked session's cookies are what the site holds against it.
            let _ = ProfileLease::reset_persistent_kind(root, &profile.id, engine.kind());
        }
    }
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
    journal: &mut PageJournal,
    log: &mut Option<crate::journal_log::JournalLog>,
    carry: &mut ceh_core::flow::RecoveryCarry,
) -> Result<SessionEnd, &'static str> {
    // Saved profile changes and budget commands replace these during the run.
    let mut timings = timings.clone();
    let mut appointment_policy = appointment_policy.clone();
    let session =
        SessionId::new(uuid::Uuid::new_v4().to_string()).map_err(|_| "search_unavailable")?;
    let mut gate = DispatchGate::new(session.clone());
    *shared_gate.lock().map_err(|_| "search_unavailable")? = Some(gate.clone());
    let samples = ceh_browser::session_driver::SessionSamples{first_bootstrap:unit_sample(),second_bootstrap:unit_sample(),reload:unit_sample(),contact_typing:unit_sample(),typing:unit_sample()};
    let mut driver = SessionDriver::with_samples(
        browser.context(),
        session.clone(),
        profile.clone(),
        timings.clone(),
        gate.clone(),
        samples,
    )
    .map_err(|_| "profile_invalid")?;
    driver.apply_recovery_carry(carry.clone());
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
    journal
        .event(JournalEvent::Start, None, None, false, wall()?)
        .map_err(|_| "search_unavailable")?;
    let mut diagnostics = crate::search_diagnostics::SearchDiagnostics::new(root, session.as_str().to_owned(), profile);
    let mut presenter = crate::session_presentation::SessionPresentation::new(session.clone(), 3000);
    let mut sequence = 0u64;
    let context = browser.context().to_owned();
    if cancelled.load(Ordering::Acquire) {
        stats
            .finish(&mut db, wall()?, mono(), "stopped")
            .map_err(|_| "statistics_unavailable")?;
        return Ok(SessionEnd::Finished);
    }
    {
        use ceh_core::timings::TimingKind;
        let ms = |kind, unit: f64| timings.sample(kind, unit).map(|s| (s * 1000.0).round() as u64).unwrap_or(0);
        let range = timings.action_delay_seconds();
        diagnostics.set_header(
            crate::session_timeline::SessionHeader {
                app_version: env!("CARGO_PKG_VERSION").to_owned(),
                firefox_version: match Engine::of(&profile.browser).unwrap_or(Engine::Firefox) {
                    Engine::Firefox => FirefoxInstallation::discover().map(|i| i.version().to_owned()).unwrap_or_default(),
                    Engine::Chromium(kind) => ceh_browser::chromium::ChromiumInstallation::discover(kind).map(|i| format!("{kind:?} {}", i.version())).unwrap_or_default(),
                },
                os: std::env::consts::OS.to_owned(),
                profile_persistent: true,
                profile_age_days: profile_age_days(root, &profile.id, &profile.browser),
                certificate_cache: format!("{:?}", browser.certificate_cache()).to_lowercase(),
                action_delay_ms: [(range[0] * 1000.0).round() as u64, (range[1] * 1000.0).round() as u64],
                first_bootstrap_ms: ms(TimingKind::Bootstrap, samples.first_bootstrap),
                // Typing samples are already milliseconds; bootstrap is seconds.
                typing_ms: timings.sample(TimingKind::Typing, samples.typing).map(|v| v.round() as u64).unwrap_or(0),
                contact_typing_ms: timings.sample(TimingKind::ContactTyping, samples.contact_typing).map(|v| v.round() as u64).unwrap_or(0),
            },
        );
    }
    let initial_preferences = ceh_store::profiles::read_profiles(&root.join("tasks.yml"), crate::catalog(), "ru").ok();
    diagnostics.set_enabled(initial_preferences.as_ref().is_some_and(|bundle| bundle.preferences.diagnostics),
        browser.connection(), &mut driver, &context);
    // Only the new owned about:blank context receives this initial GET.
    let navigated = browser.connection().call("browsingContext.navigate",json!({"context":context,"url":"https://icp.administracionelectronica.gob.es/icpplus/index.html","wait":"none"}));
    if navigated.is_err() {
        let deferred = browser.certificate_cache() == ceh_browser::firefox::CertificateCache::Deferred;
        return match crate::navigation_failure(browser.page_open(), deferred) {
            // The window was closed while the page loaded: a stop, as when it closes later.
            None => stats
                .finish(&mut db, wall()?, mono(), "stopped")
                .map(|()| SessionEnd::Finished)
                .map_err(|_| "statistics_unavailable"),
            Some(error) => Err(error),
        };
    }
    let result = (|| {
        let mut wait = false;
        let mut manual = false;
        let mut operation = 0u64;
        let mut read_failures = 0u32;
        let mut retry_at = Instant::now();
        let mut preferences_at = Instant::now() - Duration::from_secs(3);
        let mut sound_enabled = false;
        let mut page_checked = Instant::now();
        loop {
            while let Ok(message) = receiver.try_recv() {
                match message.command {
                    Control::Stop => return Ok(SessionEnd::Finished),
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
                            // Navigate first: if the original tab is gone the person keeps the browser.
                            let navigated = browser.connection().call(
                                "browsingContext.navigate",
                                json!({"context": context, "url": "https://icp.administracionelectronica.gob.es/icpplus/index.html", "wait": "none"}),
                            );
                            if navigated.is_err() {
                                browser.handoff();
                            } else {
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
                                        typing: unit_sample(),
                                    },
                                )
                                .map_err(|_| "profile_invalid")?;
                                presenter = crate::session_presentation::SessionPresentation::new(next, 3000);
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
                                        typing: unit_sample(),
                        };
                        if driver.apply_timings(next.clone(), samples).is_ok() {
                            timings = next;
                        }
                    }
                }
            }
            if cancelled.load(Ordering::Acquire) {
                return Ok(SessionEnd::Finished);
            }
            // While the app does not drive the page (handed over or paused) nothing else notices that the
            // person closed the browser: the search ends then, and Start returns (Python: the worker
            // finishes when its page is closed).
            let paused_now = driver.control().mode() == ControlMode::Paused;
            if (manual || paused_now) && page_checked.elapsed() >= Duration::from_secs(2) {
                page_checked = Instant::now();
                if !browser.page_open() {
                    return Ok(SessionEnd::Finished);
                }
            }
            publish_budget(state, &mut budget, crate::current_time()?, &appointment_policy, observer.hot_retry())?;
            let paused = driver.control().mode() == ControlMode::Paused;
            let mut effects = Vec::new();
            let mut action_delay_ms: Option<u64> = None;
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
                if std::env::var("CEH_PREVIEW_FIREFOX").as_deref() == Ok("applicant")
                    && matches!(before.observation.kind, PageKind::Applicant | PageKind::ApplicantNoSlots)
                {
                    browser.handoff();
                    update(state, |s| s.page = Some(before.observation.kind))?;
                    return Ok(SessionEnd::Finished);
                }
                effects = before.effects.clone();
                let observed_kind = before.observation.kind;
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
                    diagnostics.environment("before_action", browser.connection(), &context);
                    let bytes = uuid::Uuid::new_v4();
                    let sample = u64::from_le_bytes(
                        bytes.as_bytes()[..8]
                            .try_into()
                            .map_err(|_| "search_unavailable")?,
                    ) as f64
                        / u64::MAX as f64;
                    let pause = timings.sample(ceh_core::timings::TimingKind::Action,sample).map_err(|_|"profile_invalid")?;
                    action_delay_ms = Some((pause * 1000.0).round() as u64);
                    let until = Instant::now() + Duration::from_secs_f64(pause);
                    while Instant::now() < until && !cancelled.load(Ordering::Acquire) {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    if cancelled.load(Ordering::Acquire) {
                        return Ok(SessionEnd::Finished);
                    }
                }
                let executing_since = Instant::now();
                let entering_auth = effects.iter().any(|effect| matches!(effect, FlowEffect::EnterAuthentication { .. }));
                let submits_appointment = effects.iter().any(|effect| matches!(effect, FlowEffect::Activate { action: ActionKind::Request, .. }));
                let mut prior_budget = if submits_appointment {
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
                        // Nothing was sent: the counted attempt goes back.
                        if let Some((prior_budget, prior_observer)) = prior_budget.take() {
                            budget = prior_budget;
                            observer = prior_observer;
                            budget_store.save(&budget).map_err(|_| "appointment_budget_unavailable")?;
                        }
                        continue;
                    }
                    Err(
                        ceh_browser::BrowserError::StaleDocument
                        | ceh_browser::BrowserError::NoObservation,
                    ) if browser.connection().available() => {
                        diagnostics.step(crate::session_timeline::step_error_row(&effects, action_delay_ms, executing_since.elapsed().as_millis() as u64, "stale_document"));
                        if entering_auth {
                            return Err("browser_auth_entry_changed");
                        }
                        // Do not replay this linear plan. Only a fresh observation may advance core.
                        if let Some((prior_budget, prior_observer)) = prior_budget.take() {
                            budget = prior_budget;
                            observer = prior_observer;
                            budget_store.save(&budget).map_err(|_| "appointment_budget_unavailable")?;
                        }
                        update(state, |s| s.attention = true)?;
                        std::thread::sleep(Duration::from_millis(500));
                        continue;
                    }
                    Err(_) => {
                        diagnostics.step(crate::session_timeline::step_error_row(&effects, action_delay_ms, executing_since.elapsed().as_millis() as u64, "browser_action_failed"));
                        return Err("browser_action_failed");
                    }
                };
                diagnostics.step(crate::session_timeline::step_row(
                    &effects,
                    action_delay_ms,
                    &after.receipts.iter().map(|r| (r.action, r.outcome)).collect::<Vec<_>>(),
                    executing_since.elapsed().as_millis() as u64,
                ));
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
                // Navigation recoveries ran inside `driver.execute`; a restart belongs to this lifecycle owner.
                if let Some(request) = effects.iter().find_map(|effect| match effect {
                    FlowEffect::Recover { action, .. } => crate::restart::RestartRequest::from_action(action, observed_kind),
                    _ => None,
                }) {
                    *carry = driver.recovery_carry();
                    journal
                        .event(JournalEvent::FlowRestart, None, None, false, wall()?)
                        .map_err(|_| "search_unavailable")?;
                    match request.kind() {
                        crate::restart::RestartKind::NewSession => return Ok(SessionEnd::NewSession(request)),
                        crate::restart::RestartKind::NewPass => {
                            // Same window and profile, a fresh flow from the entry page (Python new `AssistRunner`).
                            gate = DispatchGate::new(session.clone());
                            *shared_gate.lock().map_err(|_| "search_unavailable")? = Some(gate.clone());
                            driver = SessionDriver::with_samples(
                                browser.context(),
                                session.clone(),
                                profile.clone(),
                                timings.clone(),
                                gate.clone(),
                                ceh_browser::session_driver::SessionSamples {
                                    first_bootstrap: unit_sample(),
                                    second_bootstrap: unit_sample(),
                                    reload: unit_sample(),
                                    contact_typing: unit_sample(),
                                    typing: unit_sample(),
                                },
                            )
                            .map_err(|_| "profile_invalid")?;
                            driver.apply_recovery_carry(carry.clone());
                            diagnostics.rebind(&mut driver);
                            // The new gate starts a new control authority; the presenter follows it.
                            presenter = crate::session_presentation::SessionPresentation::new(session.clone(), 3000);
                            if browser
                                .connection()
                                .call("browsingContext.navigate", json!({"context":context,"url":ceh_browser::recovery_navigation::ENTRY_URL,"wait":"none"}))
                                .is_err()
                            {
                                if !browser.page_open() {
                                    return Ok(SessionEnd::Finished);
                                }
                                return Err("browser_navigation_failed");
                            }
                            wait = false;
                            retry_at = Instant::now();
                            update(state, |s| s.deadline = None)?;
                            continue;
                        }
                    }
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
            // Stored before it is published: the Work tab reads the saved log when the rows change.
            if let Some(log) = log.as_mut() {
                log.record(journal.rows());
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
            diagnostics.flush_timeline(browser.connection(), &mut driver);
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
    diagnostics.flush_timeline(browser.connection(), &mut driver);
    diagnostics.stop_collector(browser.connection(), &mut driver);
    diagnostics.finish(&mut driver);
    let at = wall()?;
    let reason = if result.is_err() { "error" } else { "stopped" };
    stats
        .finish(&mut db, at, mono(), reason)
        .map_err(|_| "statistics_unavailable")?;
    // The search goes on in a new browser session: its journal ends only with the whole search.
    if !matches!(result, Ok(SessionEnd::NewSession(_))) {
        journal
            .event(JournalEvent::Finish, None, None, true, at)
            .map_err(|_| "search_unavailable")?;
    }
    if let Some(log) = log.as_mut() {
        log.record(journal.rows());
    }
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

#[cfg(test)]
mod budget_snapshot_tests {
    use super::*;
    use ceh_core::appointment_budget::{AppointmentBudget, AppointmentPolicy};

    #[test]
    fn snapshot_carries_the_budget_summary_and_clears_it_when_the_search_ends() {
        let state = Mutex::new(SearchSnapshot::default());
        assert!(serde_json::to_value(state.lock().unwrap().clone()).unwrap()["appointment_budget"].is_null());
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-10T11:00:00Z").unwrap().to_utc();
        let mut budget = AppointmentBudget::default();
        let policy = AppointmentPolicy::default();
        budget.submitted(now, &policy);
        publish_budget(&state, &mut budget, now, &policy, false).unwrap();
        let json = serde_json::to_value(state.lock().unwrap().clone()).unwrap();
        let summary = &json["appointment_budget"];
        assert_eq!((summary["target"].as_u64(), summary["sent"].as_u64(), summary["wait_reason"].as_str()), (Some(5), Some(1), Some("minimum_pause")));
        assert!(summary["ready"].is_string());
        clear_budget(&state).unwrap();
        assert!(serde_json::to_value(state.lock().unwrap().clone()).unwrap()["appointment_budget"].is_null());
    }

    #[test]
    fn publishing_the_budget_does_not_touch_other_snapshot_fields() {
        let state = Mutex::new(SearchSnapshot { profile: Some("p".into()), active: true, ..Default::default() });
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-10T11:00:00Z").unwrap().to_utc();
        publish_budget(&state, &mut AppointmentBudget::default(), now, &AppointmentPolicy::default(), true).unwrap();
        let snapshot = state.lock().unwrap().clone();
        assert!(snapshot.active && snapshot.profile.as_deref() == Some("p") && snapshot.appointment_budget.is_some());
    }
}

#[cfg(test)]
mod close_plan_tests {
    use super::*;
    #[test]
    fn closing_the_window_never_kills_a_browser_handed_to_the_person() {
        // Python `closeEvent`: an idle window closes at once; a searching one stops first.
        assert_eq!(close_plan(false, false), ClosePlan::CloseNow);
        assert_eq!(close_plan(false, true), ClosePlan::CloseNow);
        assert_eq!(close_plan(true, false), ClosePlan::StopAndCloseBrowser);
        // A booking in progress stays open: the app only stops its own automation.
        assert_eq!(close_plan(true, true), ClosePlan::StopKeepBrowser);
    }
    #[test]
    fn shutting_down_an_idle_controller_needs_no_wait() {
        let controller = SearchController::default();
        assert_eq!(controller.shutdown(), ClosePlan::CloseNow);
    }
}
