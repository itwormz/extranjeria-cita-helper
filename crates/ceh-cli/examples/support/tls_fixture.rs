//! macOS-only opt-in HTTPS test support. Python/OpenSSL are development tools,
//! not application dependencies. The CA exists solely in the owned temp profile.
use super::{Client, OwnedChild, Result};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
pub struct TlsFixture {
    pub port: u16,
    profile: PathBuf,
    _child: OwnedChild,
}
impl TlsFixture {
    pub fn new(profile: &Path, binary: &str) -> Result<Self> {
        if !cfg!(target_os = "macos") {
            return Err("TLS fixture currently supports macOS only".into());
        }
        let helper = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/support/tls_fixture.py");
        std::fs::write(profile.join(".ceh-owned-fixture"), "synthetic TLS fixture")?;
        let mut init = OwnedChild(
            Command::new("python3")
                .arg(&helper)
                .arg("init")
                .arg(profile)
                .arg(binary)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = init.0.try_wait()? {
                if !status.success() {
                    return Err("TLS fixture certificate initialization failed".into());
                }
                break;
            }
            if Instant::now() >= deadline {
                return Err("TLS fixture initialization timed out".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        std::fs::write(profile.join("routes.json"), "{}")?;
        let child = OwnedChild(
            Command::new("python3")
                .arg(helper)
                .arg("serve")
                .arg(profile)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        );
        let deadline = Instant::now() + Duration::from_secs(4);
        let port = loop {
            if let Ok(value) = std::fs::read_to_string(profile.join("proxy-port")) {
                break value.parse::<u16>()?;
            }
            if Instant::now() >= deadline {
                return Err("TLS fixture proxy did not start".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        Ok(Self {
            port,
            profile: profile.to_owned(),
            _child: child,
        })
    }
    pub fn routes(&self, routes: Vec<(&str, &str, &str)>) -> Result<()> {
        let values: serde_json::Map<String, Value> = routes
            .into_iter()
            .map(|(method, path, body)| (format!("{method} {path}"), json!(body)))
            .collect();
        let staged = self.profile.join("routes.next");
        std::fs::write(&staged, serde_json::to_vec(&values)?)?;
        std::fs::rename(staged, self.profile.join("routes.json"))?;
        Ok(())
    }
    pub fn requests(&self) -> Result<Vec<Value>> {
        let path = self.profile.join("requests.jsonl");
        if !path.exists() {
            return Ok(vec![]);
        }
        let bytes = std::fs::read(&path)?;
        if bytes.len() > 65536 {
            return Err("TLS fixture request budget exceeded".into());
        }
        String::from_utf8(bytes)?
            .lines()
            .map(|line| Ok(serde_json::from_str(line)?))
            .collect()
    }
    pub fn wait_load(
        &self,
        client: &mut Client,
        context: &str,
        path: &str,
        start: usize,
    ) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            client.call("session.status", json!({}))?;
            if client.events[start..].iter().any(|e| {
                e["method"] == "browsingContext.load"
                    && e["params"]["context"] == context
                    && e["params"]["url"]
                        == format!("https://icp.administracionelectronica.gob.es{path}")
            }) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("TLS fixture navigation did not load".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    pub fn navigate(&self, client: &mut Client, context: &str, path: &str) -> Result<()> {
        let start = client.events.len();
        client.call("browsingContext.navigate",json!({"context":context,"url":format!("https://icp.administracionelectronica.gob.es{path}"),"wait":"none"}))?;
        self.wait_load(client, context, path, start)
    }
}

impl Drop for TlsFixture {
    fn drop(&mut self) {
        if std::env::var_os("CEH_SPIKE_TRACE").is_some() {
            let rows = self.requests().unwrap_or_default();
            let requests = serde_json::json!({"get":rows.iter().filter(|r|r["method"]=="GET").count(),"post":rows.iter().filter(|r|r["method"]=="POST").count(),"ok":rows.iter().filter(|r|r["status"]==200).count()});
            let errors = std::fs::read_to_string(self.profile.join("server-errors.jsonl"))
                .unwrap_or_default();
            eprintln!(
                "TLS fixture requests={requests}, error-types={}",
                errors.chars().take(1024).collect::<String>()
            );
        }
    }
}
