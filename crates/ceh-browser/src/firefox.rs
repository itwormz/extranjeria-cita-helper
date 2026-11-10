//! Installed Firefox only. Profile paths and processes belong exclusively to CEH.
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirefoxError {
    NotInstalled,
    InvalidInstallation,
    InvalidProfile,
    ProfileBusy,
    Unavailable,
    StartupTimeout,
    CertificateCache,
    Cancelled,
    ManualOwnership,
}
/// Outcome of preparing the TLS intermediate cache: optional, so a failure defers it instead of
/// stopping the browser. Certificate validation stays strict either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CertificateCache {
    Ready(usize),
    Deferred,
}
impl CertificateCache {
    pub fn from_result(result: Result<usize, FirefoxError>) -> Self {
        result.map_or(Self::Deferred, Self::Ready)
    }
}
/// No Debug: paths can contain account names and must not enter user reports verbatim.
pub struct FirefoxInstallation {
    executable: PathBuf,
    version: String,
}
impl FirefoxInstallation {
    pub fn from_executable(path: &Path) -> Result<Self, FirefoxError> {
        let executable = path
            .canonicalize()
            .map_err(|_| FirefoxError::NotInstalled)?;
        if !executable.is_file() {
            return Err(FirefoxError::InvalidInstallation);
        }
        let parent = executable
            .parent()
            .ok_or(FirefoxError::InvalidInstallation)?;
        let candidates = [
            parent.join("application.ini"),
            parent.join("../Resources/application.ini"),
        ];
        for candidate in candidates {
            let Ok(metadata) = candidate.metadata() else {
                continue;
            };
            if metadata.len() > 65536 {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&candidate) else {
                continue;
            };
            let mut app = false;
            let mut name = None;
            let mut version = None;
            for line in raw.lines().map(str::trim) {
                if line.starts_with('[') {
                    app = line == "[App]";
                    continue;
                }
                if !app {
                    continue;
                }
                if let Some(v) = line.strip_prefix("Name=") {
                    name = Some(v);
                }
                if let Some(v) = line.strip_prefix("Version=") {
                    version = Some(v);
                }
            }
            if name == Some("Firefox")
                && let Some(version) = version.filter(|v| {
                    !v.is_empty()
                        && v.len() <= 32
                        && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '.')
                })
            {
                return Ok(Self {
                    executable,
                    version: version.into(),
                });
            }
        }
        Err(FirefoxError::InvalidInstallation)
    }
    pub fn version(&self) -> &str {
        &self.version
    }
    pub fn executable(&self) -> &Path {
        &self.executable
    }
    /// Fixed known install locations first; never inspect the user's Firefox profiles.
    pub fn discover() -> Result<Self, FirefoxError> {
        let mut candidates = Vec::new();
        #[cfg(target_os = "macos")]
        {
            candidates.push(PathBuf::from(
                "/Applications/Firefox.app/Contents/MacOS/firefox",
            ));
            if let Some(home) = std::env::var_os("HOME") {
                candidates.push(
                    PathBuf::from(home).join("Applications/Firefox.app/Contents/MacOS/firefox"),
                );
            }
        }
        #[cfg(target_os = "windows")]
        {
            for key in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
                if let Some(root) = std::env::var_os(key) {
                    candidates.push(PathBuf::from(root).join("Mozilla Firefox/firefox.exe"));
                }
            }
        }
        #[cfg(target_os = "linux")]
        {
            for path in [
                "/usr/lib/firefox/firefox",
                "/usr/lib64/firefox/firefox",
                "/opt/firefox/firefox",
            ] {
                candidates.push(PathBuf::from(path));
            }
        }
        candidates
            .iter()
            .find_map(|path| Self::from_executable(path).ok())
            .ok_or(FirefoxError::NotInstalled)
    }
}
pub struct ProfileLease {
    path: PathBuf,
    lock: Option<File>,
    temporary: Option<tempfile::TempDir>,
}
impl ProfileLease {
    fn root(runtime: &Path) -> Result<PathBuf, FirefoxError> {
        if !runtime.is_absolute() {
            return Err(FirefoxError::InvalidProfile);
        }
        let runtime = runtime
            .canonicalize()
            .map_err(|_| FirefoxError::InvalidProfile)?;
        let root = runtime.join("browser-profiles");
        if let Ok(meta) = fs::symlink_metadata(&root) {
            if meta.file_type().is_symlink() || !meta.is_dir() {
                return Err(FirefoxError::InvalidProfile);
            }
        } else {
            fs::create_dir(&root).map_err(|_| FirefoxError::InvalidProfile)?;
        }
        Ok(root)
    }
    pub fn temporary(runtime: &Path) -> Result<Self, FirefoxError> {
        let temporary = tempfile::Builder::new()
            .prefix("firefox-session-")
            .tempdir_in(Self::root(runtime)?)
            .map_err(|_| FirefoxError::InvalidProfile)?;
        let path = temporary.path().to_owned();
        Self::lease(path, Some(temporary))
    }
    pub fn persistent(runtime: &Path, key: &str) -> Result<Self, FirefoxError> {
        ceh_core::model::ProfileId::new(key).map_err(|_| FirefoxError::InvalidProfile)?;
        let path = Self::root(runtime)?.join(format!("firefox-{key}"));
        match fs::create_dir(&path) {
            Ok(()) => {
                fs::write(path.join("ceh-owned-profile"), b"CEH Firefox profile v1\n")
                    .map_err(|_| FirefoxError::InvalidProfile)?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let meta = fs::symlink_metadata(&path).map_err(|_| FirefoxError::InvalidProfile)?;
                if meta.file_type().is_symlink()
                    || !meta.is_dir()
                    || fs::read(path.join("ceh-owned-profile")).ok().as_deref()
                        != Some(b"CEH Firefox profile v1\n")
                {
                    return Err(FirefoxError::InvalidProfile);
                }
            }
            Err(_) => return Err(FirefoxError::InvalidProfile),
        }
        Self::lease(path, None)
    }
    fn lease(path: PathBuf, temporary: Option<tempfile::TempDir>) -> Result<Self, FirefoxError> {
        let lock_path = path.join("ceh-owner.lock");
        if fs::symlink_metadata(&lock_path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(FirefoxError::InvalidProfile);
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .map_err(|_| FirefoxError::InvalidProfile)?;
        lock.try_lock().map_err(|_| FirefoxError::ProfileBusy)?;
        Ok(Self {
            path,
            lock: Some(lock),
            temporary,
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for ProfileLease {
    fn drop(&mut self) {
        if let Some(lock) = self.lock.take() {
            let _ = lock.unlock();
            drop(lock);
        }
        if let Some(directory) = self.temporary.take() {
            let _ = directory.close();
        }
    }
}

use crate::bidi::BidiConnection;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::SocketAddr,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
/// Only used by the owned browser worker, not by UI code. No attach to personal sessions.
pub struct OwnedFirefox {
    child: Option<Child>,
    profile: Option<ProfileLease>,
    connection: Option<BidiConnection>,
    context: String,
    endpoint: SocketAddr,
    manual: bool,
    certificate_cache: CertificateCache,
}
impl OwnedFirefox {
    pub fn start(
        installation: &FirefoxInstallation,
        profile: ProfileLease,
        headless: bool,
        cancelled: &AtomicBool,
    ) -> Result<Self, FirefoxError> {
        if cancelled.load(Ordering::Acquire) {
            return Err(FirefoxError::Cancelled);
        }
        let prefs = json!({
         "security.enterprise_roots.enabled":true,"security.osclientcerts.autoload":true,
         "dom.disable_window_flip":true,"browser.tabs.loadDivertedInBackground":true,
         "intl.locale.requested":"es-ES","intl.accept_languages":"es-ES, es",
         "browser.shell.checkDefaultBrowser":false,"browser.startup.homepage":"about:blank",
         "browser.startup.homepage_override.mstone":"ignore","startup.homepage_welcome_url":"",
         "datareporting.policy.dataSubmissionEnabled":false,"toolkit.telemetry.enabled":false
        });
        let user_js = profile.path().join("user.js");
        if fs::symlink_metadata(&user_js).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(FirefoxError::InvalidProfile);
        }
        let certificate_cache = CertificateCache::from_result(crate::firefox_certificates::prepare(&profile, installation));
        let mut file = File::create(user_js).map_err(|_| FirefoxError::InvalidProfile)?;
        for (name, value) in prefs.as_object().ok_or(FirefoxError::Unavailable)? {
            writeln!(file, "user_pref({}, {});", json!(name), value)
                .map_err(|_| FirefoxError::InvalidProfile)?;
        }
        drop(file);
        let mut command = Command::new(installation.executable());
        command
            .args(["--no-remote", "--new-instance", "--profile"])
            .arg(profile.path())
            .args(["--remote-debugging-port", "0"]);
        if headless {
            command.arg("--headless");
        } else {
            command.env_remove("MOZ_HEADLESS");
        }
        // Camoufox-specific identity settings must not leak into stock Firefox.
        for (name, _) in std::env::vars_os() {
            if name
                .to_str()
                .is_some_and(|s| s.starts_with("CAMOU_CONFIG_") || s.starts_with("CAMOU_PREFS_"))
            {
                command.env_remove(name);
            }
        }
        command
            .arg("about:blank")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|_| FirefoxError::Unavailable)?;
        let stderr = child.stderr.take().ok_or(FirefoxError::Unavailable)?;
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut stderr = stderr;
            let mut buf = [0u8; 4096];
            let mut pending = Vec::new();
            let mut reported = false;
            while let Ok(count) = stderr.read(&mut buf) {
                if count == 0 {
                    break;
                }
                for byte in &buf[..count] {
                    if *byte == b'\n' {
                        if !reported
                            && let Ok(line) = std::str::from_utf8(&pending)
                            && let Some(address) = parse_endpoint(line)
                        {
                            let _ = sender.try_send(address);
                            reported = true;
                        }
                        pending.clear();
                    } else if pending.len() < 8192 {
                        pending.push(*byte);
                    }
                }
            }
        });
        let mut owned = Self {
            child: Some(child),
            profile: Some(profile),
            connection: None,
            context: String::new(),
            endpoint: SocketAddr::from(([127, 0, 0, 1], 0)),
            manual: false,
            certificate_cache,
        };
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if cancelled.load(Ordering::Acquire) {
                return Err(FirefoxError::Cancelled);
            }
            if owned
                .child
                .as_mut()
                .ok_or(FirefoxError::Unavailable)?
                .try_wait()
                .map_err(|_| FirefoxError::Unavailable)?
                .is_some()
            {
                return Err(FirefoxError::Unavailable);
            }
            if Instant::now() >= deadline {
                return Err(FirefoxError::StartupTimeout);
            }
            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(address) => {
                    owned.endpoint = address;
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return Err(FirefoxError::Unavailable),
            }
        }
        let mut connection = BidiConnection::connect(owned.endpoint, Duration::from_secs(5))
            .map_err(|_| FirefoxError::Unavailable)?;
        let session = connection
            .call_before(
                "session.new",
                json!({"capabilities":{"alwaysMatch":{"acceptInsecureCerts":false}}}),
                deadline,
            )
            .map_err(|_| FirefoxError::Unavailable)?;
        if session["capabilities"]["browserName"] != "firefox"
            || session["capabilities"]["acceptInsecureCerts"] != false
        {
            return Err(FirefoxError::InvalidInstallation);
        }
        connection.call_before("session.subscribe",json!({"events":["network.beforeRequestSent","network.responseCompleted","network.fetchError","browsingContext.load","browsingContext.navigationStarted"]}),deadline).map_err(|_|FirefoxError::Unavailable)?;
        let tree = connection
            .call_before("browsingContext.getTree", json!({}), deadline)
            .map_err(|_| FirefoxError::Unavailable)?;
        owned.context = tree["contexts"]
            .as_array()
            .filter(|v| v.len() == 1)
            .and_then(|v| v[0]["context"].as_str())
            .filter(|s| !s.is_empty() && s.len() <= 512)
            .ok_or(FirefoxError::Unavailable)?
            .into();
        if cancelled.load(Ordering::Acquire) {
            return Err(FirefoxError::Cancelled);
        }
        owned.connection = Some(connection);
        Ok(owned)
    }
    /// Whether the optional TLS intermediate cache is ready or was deferred (strict HTTPS either way).
    pub fn certificate_cache(&self) -> CertificateCache {
        self.certificate_cache
    }
    pub fn process_id(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }
    pub fn context(&self) -> &str {
        &self.context
    }
    pub fn endpoint(&self) -> SocketAddr {
        self.endpoint
    }
    pub fn profile_path(&self) -> &Path {
        self.profile.as_ref().expect("running profile").path()
    }
    pub fn connection(&mut self) -> &mut BidiConnection {
        self.connection.as_mut().expect("running connection")
    }
    /// Irreversible for this instance. The core's dispatch gate must also hand off.
    pub fn handoff(&mut self) {
        self.manual = true;
    }
    /// Take the browser back after a handoff, only on the person's explicit command ("continue searching").
    /// The browser keeps its session; it must still be running and reachable.
    pub fn resume_automation(&mut self) -> Result<(), FirefoxError> {
        let reachable = self.connection.as_ref().is_some_and(BidiConnection::available);
        if !self.manual || !reachable || self.child.is_none() {
            return Err(FirefoxError::Unavailable);
        }
        self.manual = false;
        Ok(())
    }
    pub fn stop(&mut self) -> Result<(), FirefoxError> {
        if self.manual {
            return Err(FirefoxError::ManualOwnership);
        }
        self.connection.take();
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.profile.take();
        Ok(())
    }
}
impl Drop for OwnedFirefox {
    fn drop(&mut self) {
        self.connection.take();
        if self.manual {
            if let Some(mut child) = self.child.take() {
                let profile = self.profile.take();
                std::thread::spawn(move || {
                    let _ = child.wait();
                    drop(profile);
                });
            }
        } else {
            let _ = self.stop();
        }
    }
}

fn parse_endpoint(line: &str) -> Option<SocketAddr> {
    let raw = line.strip_prefix("WebDriver BiDi listening on ")?;
    let url = url::Url::parse(raw.trim()).ok()?;
    if url.scheme() != "ws"
        || url.host_str() != Some("127.0.0.1")
        || !matches!(url.path(), "" | "/" | "/session")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    Some(SocketAddr::from((
        [127, 0, 0, 1],
        url.port().filter(|p| *p != 0)?,
    )))
}

#[cfg(test)]
mod endpoint_tests {
    use super::*;
    #[test]
    fn firefox_endpoint_accepts_actual_root_advertisement_and_rejects_foreign_addresses() {
        assert_eq!(
            parse_endpoint("WebDriver BiDi listening on ws://127.0.0.1:52525")
                .unwrap()
                .port(),
            52525
        );
        assert_eq!(
            parse_endpoint("WebDriver BiDi listening on ws://127.0.0.1:52525/session")
                .unwrap()
                .port(),
            52525
        );
        for url in [
            "ws://0.0.0.0:52525",
            "ws://example.com:52525",
            "ws://127.0.0.1:52525/other",
            "ws://user:pass@127.0.0.1:52525",
            "ws://127.0.0.1:52525?x=1",
        ] {
            assert!(parse_endpoint(&format!("WebDriver BiDi listening on {url}")).is_none());
        }
    }
}
