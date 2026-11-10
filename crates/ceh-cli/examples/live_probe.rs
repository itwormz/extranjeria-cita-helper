//! LIVE probe of the real ICP site, development only: runs the app's own session driver in a fresh
//! temporary Firefox profile from the start page through province and procedure up to the authentication
//! page, then stops. It never enters authentication, certificates or personal data (the profile is fake).
//! Each run is one clean browser session, so a block only affects that session.
//! Usage: cargo run -p ceh-cli --example live_probe --release -- [--headless] [--runs N] [--pause SECONDS]
use ceh_browser::{
    DispatchGate,
    firefox::{FirefoxInstallation, OwnedFirefox, ProfileLease},
    session_driver::{SessionDriver, SessionSamples},
};
use ceh_core::{flow::FlowEffect, model::{PageKind, SessionId}, profiles::ProfileDraft, timings::{AssistTimings, TimingKind}};
use std::{path::Path, sync::atomic::AtomicBool, time::{Duration, Instant}};

fn profile() -> ProfileDraft {
    ProfileDraft {
        id: "probe".into(), label: "probe".into(), enabled: true,
        procedure: "huellas".into(),
        province: "Barcelona".into(), office: String::new(), document_type: "NIE".into(),
        document_number: "X0000000T".into(), full_name: "FICTITIOUS PERSON".into(), nationality: "RUSIA".into(),
        phone: "600000000".into(), email: "fixture@example.test".into(), access_mode: "certificate".into(),
        clave_method: "movil".into(), fill_mode: "manual".into(), no_slots_mode: "salir".into(),
        applicant_no_slots_mode: "wait".into(), subprocedure: None, timezone: "Europe/Madrid".into(),
        browser_session: "persistent".into(), browser: "firefox".into(),
    }
}
fn unit() -> f64 {
    let b = uuid_like();
    u64::from_le_bytes(b) as f64 / u64::MAX as f64
}
fn uuid_like() -> [u8; 8] {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64;
    (n.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)).to_le_bytes()
}
fn once(headless: bool, persistent: bool) -> Result<String, String> {
    let installation = FirefoxInstallation::discover().map_err(|e| format!("{e:?}"))?;
    let runtime = Path::new(".local/builds/work/live-probe");
    std::fs::create_dir_all(runtime).map_err(|e| e.to_string())?;
    let runtime = runtime.canonicalize().map_err(|e| e.to_string())?;
    let lease = if persistent { ProfileLease::persistent(&runtime, "probe") } else { ProfileLease::temporary(&runtime) }.map_err(|e| format!("{e:?}"))?;
    let mut browser = OwnedFirefox::start(&installation, lease, headless, &AtomicBool::new(false)).map_err(|e| format!("{e:?}"))?;
    let context = browser.context().to_owned();
    let session = SessionId::new(format!("probe-{}", uuid_like()[0])).map_err(|e| format!("{e:?}"))?;
    let gate = DispatchGate::new(session.clone());
    let timings = AssistTimings::default();
    let samples = SessionSamples { first_bootstrap: unit(), second_bootstrap: unit(), reload: unit(), contact_typing: unit(), typing: unit() };
    let mut driver = SessionDriver::with_samples(&context, session, profile(), timings.clone(), gate, samples).map_err(|e| format!("{e:?}"))?;
    browser.connection().call("browsingContext.navigate", serde_json::json!({"context":context,"url":"https://icp.administracionelectronica.gob.es/icpplus/index.html","wait":"none"})).map_err(|e| format!("navigate {e:?}"))?;
    let clock = Instant::now();
    let mono = || clock.elapsed().as_millis() as u64;
    let mut trail: Vec<String> = Vec::new();
    let mut last_kind: Option<String> = None;
    let mut last_note = String::new();
    let outcome = loop {
        if clock.elapsed() > Duration::from_secs(70) { break "timeout".to_owned(); }
        let step = match driver.plan(browser.connection(), mono()) {
            Ok(step) => step,
            Err(_) => { std::thread::sleep(Duration::from_millis(300)); continue; }
        };
        let update = step.update();
        let kind = update.observation.kind;
        let label = format!("{kind:?}");
        if last_kind.as_deref() != Some(label.as_str()) { trail.push(format!("{label}@{}s", clock.elapsed().as_secs())); last_kind = Some(label); }
        if matches!(kind, PageKind::Blocked | PageKind::RateLimited) { break format!("{kind:?}"); }
        let stops = update.effects.iter().any(|e| matches!(e, FlowEffect::EnterAuthentication { .. } | FlowEffect::SelectCertificate { .. } | FlowEffect::SelectClaveMethod { .. } | FlowEffect::FetchApplicant { .. }))
            || matches!(kind, PageKind::Auth | PageKind::AuthWait | PageKind::Applicant | PageKind::ApplicantNoSlots | PageKind::Captcha);
        if stops { break format!("reached {kind:?}"); }
        if update.effects.iter().any(|e| matches!(e, FlowEffect::Recover { .. })) { break format!("recovery needed at {kind:?}"); }
        if update.effects.iter().any(|e| matches!(e, FlowEffect::Prepare { .. } | FlowEffect::Activate { .. } | FlowEffect::AcceptCookies { .. })) {
            let until = Instant::now() + Duration::from_secs_f64(timings.sample(TimingKind::Action, unit()).unwrap_or(0.5));
            while Instant::now() < until { std::thread::sleep(Duration::from_millis(10)); }
        }
        let names: Vec<String> = update.effects.iter().map(|e| format!("{e:?}").chars().take(28).collect()).collect();
        let note = format!("{names:?}");
        if note != last_note { trail.push(format!("effects{note}")); last_note = note; }
        if let Err(error) = driver.execute(browser.connection(), step) { trail.push(format!("exec-error {error:?}")); std::thread::sleep(Duration::from_millis(300)); }
        std::thread::sleep(Duration::from_millis(200));
    };
    let _ = browser.stop();
    Ok(format!("{outcome} | {}", trail.join(" > ")))
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let headless = args.iter().any(|a| a == "--headless");
    let persistent = args.iter().any(|a| a == "--persistent");
    let runs: usize = args.iter().position(|a| a == "--runs").and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(1);
    let pause: u64 = args.iter().position(|a| a == "--pause").and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(45);
    for run in 1..=runs {
        println!("run {run}/{runs}: {}", once(headless, persistent).unwrap_or_else(|e| format!("error {e}")));
        if run < runs { std::thread::sleep(Duration::from_secs(pause)); }
    }
}
