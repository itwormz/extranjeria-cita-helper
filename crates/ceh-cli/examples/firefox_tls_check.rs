//! Read-only live ICP TLS check: no applicant data, clicks, authentication or screenshots.
use ceh_browser::firefox::{FirefoxInstallation, OwnedFirefox, ProfileLease};
use std::{path::Path, sync::atomic::AtomicBool};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = Path::new(".local/builds/work/firefox-tls-check");
    std::fs::create_dir_all(runtime)?;
    let runtime = runtime.canonicalize()?;
    let installation =
        FirefoxInstallation::discover().map_err(|_| "Firefox installation unavailable")?;
    let lease = ProfileLease::temporary(&runtime).map_err(|_| "Own profile unavailable")?;
    let mut browser = OwnedFirefox::start(&installation, lease, true, &AtomicBool::new(false))
        .map_err(|_| "Firefox startup unavailable")?;
    let context = browser.context().to_owned();
    browser.connection().call("browsingContext.navigate",serde_json::json!({"context":context,"url":"https://icp.administracionelectronica.gob.es/icpplus/index.html","wait":"complete"})).map_err(|_|"Strict TLS navigation rejected")?;
    println!("strict_tls_navigation_completed=true; no_personal_data_or_actions=true");
    browser.stop().map_err(|_| "Own browser cleanup failed")?;
    Ok(())
}
