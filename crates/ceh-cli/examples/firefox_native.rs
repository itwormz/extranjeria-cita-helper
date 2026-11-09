//! Isolated native launcher/lifecycle proof, no appointment website.
use ceh_browser::firefox::{FirefoxError, FirefoxInstallation, OwnedFirefox, ProfileLease};
use serde_json::json;
use std::{
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(".local/builds/work/firefox-native");
    std::fs::create_dir_all(root)?;
    let root = root.canonicalize()?;
    let installation =
        FirefoxInstallation::discover().map_err(|e| format!("installation: {e:?}"))?;
    let cancelled = AtomicBool::new(false);
    let visible = std::env::args().any(|arg| arg == "--window");
    let mut first = OwnedFirefox::start(
        &installation,
        ProfileLease::temporary(&root).map_err(|e| format!("profile: {e:?}"))?,
        !visible,
        &cancelled,
    )
    .map_err(|e| format!("start: {e:?}"))?;
    let first_path = first.profile_path().to_owned();
    let pid = first.process_id().ok_or("missing owned PID")?;
    let context = first.context().to_owned();
    let value=first.connection().call("script.evaluate",json!({"expression":"JSON.stringify({language:navigator.language,timezone:Intl.DateTimeFormat().resolvedOptions().timeZone})","target":{"context":context},"awaitPromise":false})).map_err(|e|format!("facts: {e:?}"))?;
    let locale: serde_json::Value =
        serde_json::from_str(value["result"]["value"].as_str().ok_or("locale result")?)?;
    if locale["language"] != "es-ES" {
        return Err("Firefox locale mismatch".into());
    }
    if visible {
        first.connection().call("script.evaluate",json!({"expression":"document.title='CEH — Firefox local check';document.body.innerHTML='<main style=\"font:20px system-ui;padding:48px\"><h1>CEH · Firefox</h1><p>Отдельный тестовый профиль. Сайт записи не открыт.</p><input placeholder=\"Проверка ручного ввода\"><button onclick=\"this.textContent=event.isTrusted?`Доверенный клик`:`Синтетический клик`\">Проверить клик</button></main>';true","target":{"context":context},"awaitPromise":false})).map_err(|_|"visible fixture")?;
        println!("visible_fixture_ready");
        std::thread::sleep(Duration::from_secs(30));
    }
    let mut second = OwnedFirefox::start(
        &installation,
        ProfileLease::temporary(&root).map_err(|e| format!("second profile: {e:?}"))?,
        true,
        &cancelled,
    )
    .map_err(|e| format!("second start: {e:?}"))?;
    if first_path == second.profile_path() {
        return Err("profiles shared".into());
    }
    second.stop().map_err(|e| format!("second stop: {e:?}"))?;
    drop(second);
    first
        .connection()
        .call(
            "script.evaluate",
            json!({"expression":"true","target":{"context":context},"awaitPromise":false}),
        )
        .map_err(|_| "other instance closed")?;
    first.handoff();
    if !matches!(first.stop(), Err(FirefoxError::ManualOwnership)) {
        return Err("manual instance can be closed automatically".into());
    }
    drop(first);
    // Test-only liveness/cleanup of this exact owned PID. Firefox does not
    // support a new session on /session while the detached session remains alive.
    if !std::process::Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .status()?
        .success()
    {
        return Err("manual browser was closed".into());
    }
    if !std::process::Command::new("/bin/kill")
        .args(["-TERM", &pid.to_string()])
        .status()?
        .success()
    {
        return Err("fixture cleanup failed".into());
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while first_path.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    if first_path.exists() {
        return Err("temporary profile not reaped after manual browser exit".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"browser":"system_firefox","version":installation.version(),"two_isolated_processes":true,"locale":locale,"handoff_survives_controller_drop":true,"temporary_profile_removed_after_browser_exit":true,"no_appointment_site_navigation":true})
        )?
    );
    Ok(())
}
