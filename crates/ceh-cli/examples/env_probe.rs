//! LOCAL environment probe: serves a measurement page on 127.0.0.1 and saves what a page can see of the
//! browser that opens it, so a person's own browser and the app's automated one can be compared. It never
//! contacts the appointment site and collects no personal data (only browser/environment properties).
//! Usage: `cargo run -p ceh-cli --example env_probe --release -- serve` (then open the printed URL with
//! `?label=manual` in a normal browser), or `-- automated` to open it in the app's own Firefox.
use ceh_browser::firefox::{FirefoxInstallation, OwnedFirefox, ProfileLease};
use std::{io::{Read, Write}, net::TcpListener, path::Path, sync::atomic::AtomicBool, time::{Duration, Instant}};

const PAGE: &str = include_str!("support/env-probe.html");
const PORT: u16 = 47831;

fn serve(listener: TcpListener, out: std::path::PathBuf) {
    for stream in listener.incoming().flatten() {
        let mut stream = stream;
        let mut data = Vec::new();
        let mut buffer = [0u8; 8192];
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        loop {
            let Ok(n) = stream.read(&mut buffer) else { break };
            if n == 0 { break; }
            data.extend_from_slice(&buffer[..n]);
            let text = String::from_utf8_lossy(&data).to_string();
            if let Some(split) = text.find("\r\n\r\n") {
                let length = text[..split].lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok())).unwrap_or(0);
                if data.len() >= split + 4 + length { break; }
            }
        }
        let text = String::from_utf8_lossy(&data).to_string();
        let first = text.lines().next().unwrap_or("").to_owned();
        if first.starts_with("POST /report") {
            let label: String = first.split("label=").nth(1).and_then(|r| r.split(' ').next()).unwrap_or("unlabeled").chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
            let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
            let _ = std::fs::write(out.join(format!("{label}.json")), body);
            println!("saved report: {label}.json ({} bytes)", body.len());
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
        } else {
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", PAGE.len(), PAGE);
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "serve".into());
    let out = Path::new(".local/builds/reports/env-probe").canonicalize()?;
    let listener = TcpListener::bind(("127.0.0.1", PORT))?;
    let url = format!("http://127.0.0.1:{PORT}/");
    println!("probe page: {url}?label=manual  (open it in your own browser; reports go to {})", out.display());
    if mode == "automated" {
        let label = std::env::args().nth(2).unwrap_or_else(|| "automated".into());
        std::thread::spawn({ let out = out.clone(); move || serve(listener, out) });
        let installation = FirefoxInstallation::discover().map_err(|_| "no firefox")?;
        let runtime = Path::new(".local/builds/work/env-probe").canonicalize().or_else(|_| { std::fs::create_dir_all(".local/builds/work/env-probe")?; Path::new(".local/builds/work/env-probe").canonicalize() })?;
        let lease = ProfileLease::temporary(&runtime).map_err(|_| "profile")?;
        let mut browser = OwnedFirefox::start(&installation, lease, false, &AtomicBool::new(false)).map_err(|e| format!("start {e:?}"))?;
        let context = browser.context().to_owned();
        if std::env::var("MASK").is_ok() {
            let f = "() => { const d = Object.getOwnPropertyDescriptor(Navigator.prototype, 'webdriver'); if (d && d.get) { Object.defineProperty(Navigator.prototype, 'webdriver', { ...d, get: new Proxy(d.get, { apply: () => false }) }); } }";
            browser.connection().call("script.addPreloadScript", serde_json::json!({"functionDeclaration":f})).map_err(|e| format!("preload {e:?}"))?;
        }
        browser.connection().call("browsingContext.navigate", serde_json::json!({"context":context,"url":format!("{url}?label={label}"),"wait":"complete"})).map_err(|_| "navigate")?;
        let file = out.join(format!("{label}.json"));
        let deadline = Instant::now() + Duration::from_secs(20);
        while !file.exists() && Instant::now() < deadline { std::thread::sleep(Duration::from_millis(200)); }
        let _ = browser.stop();
        println!("{}", if file.exists() { "automated report saved" } else { "no report received" });
    } else {
        serve(listener, out);
    }
    Ok(())
}
