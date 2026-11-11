//! Chrome and Brave as the owned browser (variant A of the F5 plan). Started like an ordinary browser:
//! own profile directory, loopback debug port read from the profile's `DevToolsActivePort`, no automation
//! switches, a window the person can see.
use crate::firefox::{FirefoxError, ProfileKind};
use crate::launch::TargetOs;
use std::path::{Path, PathBuf};

/// Flags of the launch. Deliberately short: every extra flag is a difference from a normal Chrome. Playwright and
/// Puppeteer defaults (`--enable-automation`, `--disable-component-update`, ...) are not used.
pub fn launch_arguments(profile: &Path, os: TargetOs) -> Vec<String> {
    let mut args = vec![
        "--remote-debugging-port=0".to_owned(),
        format!("--user-data-dir={}", profile.display()),
        "--no-first-run".to_owned(),
        "--no-default-browser-check".to_owned(),
        // Python's Camoufox identity is Spanish; the site is Spanish.
        "--lang=es-ES".to_owned(),
        // `navigator.webdriver` is true whenever a debug port is open; this switch turns it off the way Chrome's own
        // field trials do, and `--test-type` keeps Chrome from warning the person about the unsupported flag.
        "--disable-blink-features=AutomationControlled".to_owned(),
        "--test-type".to_owned(),
        // A window behind others or a tab in the background must keep answering input: without these Chrome
        // throttles it and a click is never acknowledged.
        "--disable-backgrounding-occluded-windows".to_owned(),
        "--disable-renderer-backgrounding".to_owned(),
        "--disable-background-timer-throttling".to_owned(),
    ];
    match os {
        // A fresh profile otherwise asks for the login keychain ("Chrome Safe Storage").
        TargetOs::Macos => args.push("--use-mock-keychain".to_owned()),
        TargetOs::Linux => args.push("--password-store=basic".to_owned()),
        TargetOs::Windows => {}
    }
    args.push("about:blank".to_owned());
    args
}

/// `DevToolsActivePort`: the port on the first line, the browser websocket path on the second.
pub fn parse_devtools_active_port(text: &str) -> Option<(u16, String)> {
    let mut lines = text.lines();
    let port: u16 = lines.next()?.trim().parse().ok().filter(|p| *p != 0)?;
    let path = lines.next()?.trim();
    (path.starts_with("/devtools/browser/") && !path.contains("..") && path.len() <= 200).then(|| (port, path.to_owned()))
}

/// The version number out of `chrome --version` ("Google Chrome 152.0.7977.65").
pub fn parse_version(line: &str) -> Option<String> {
    let last = line.split_whitespace().last()?;
    (last.chars().next()?.is_ascii_digit() && last.contains('.') && last.len() <= 32 && last.chars().all(|c| c.is_ascii_digit() || c == '.')).then(|| last.to_owned())
}

pub struct ChromiumInstallation {
    kind: ProfileKind,
    executable: PathBuf,
    version: String,
}
impl ChromiumInstallation {
    pub fn from_executable(kind: ProfileKind, path: &Path) -> Result<Self, FirefoxError> {
        if kind == ProfileKind::Firefox {
            return Err(FirefoxError::InvalidInstallation);
        }
        let executable = path.canonicalize().map_err(|_| FirefoxError::NotInstalled)?;
        if !executable.is_file() {
            return Err(FirefoxError::InvalidInstallation);
        }
        let output = std::process::Command::new(&executable)
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .map_err(|_| FirefoxError::InvalidInstallation)?;
        let version = parse_version(&String::from_utf8_lossy(&output.stdout)).ok_or(FirefoxError::InvalidInstallation)?;
        Ok(Self { kind, executable, version })
    }
    /// Fixed known install locations only; never inspect the person's browser profiles.
    pub fn discover(kind: ProfileKind) -> Result<Self, FirefoxError> {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if cfg!(target_os = "macos") {
            candidates.push(
                match kind {
                    ProfileKind::Chrome => "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
                    ProfileKind::Brave => "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
                    _ => "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
                }
                .into(),
            );
        } else if cfg!(target_os = "windows") {
            for variable in ["ProgramFiles", "ProgramFiles(x86)", "LocalAppData"] {
                if let Some(base) = std::env::var_os(variable) {
                    let base = PathBuf::from(base);
                    candidates.push(match kind {
                        ProfileKind::Chrome => base.join("Google/Chrome/Application/chrome.exe"),
                        ProfileKind::Brave => base.join("BraveSoftware/Brave-Browser/Application/brave.exe"),
                        _ => base.join("Microsoft/Edge/Application/msedge.exe"),
                    });
                }
            }
        } else {
            let names: &[&str] = match kind {
                ProfileKind::Chrome => &["google-chrome", "google-chrome-stable"],
                ProfileKind::Brave => &["brave-browser", "brave"],
                _ => &["microsoft-edge", "microsoft-edge-stable"],
            };
            candidates.extend(names.iter().map(|n| PathBuf::from("/usr/bin").join(n)));
        }
        candidates.iter().find_map(|path| Self::from_executable(kind, path).ok()).ok_or(FirefoxError::NotInstalled)
    }
    pub fn kind(&self) -> ProfileKind {
        self.kind
    }
    pub fn version(&self) -> &str {
        &self.version
    }
    pub fn executable(&self) -> &Path {
        &self.executable
    }
}

/// Spanish pages like Python's identity (`--lang` is ignored on macOS, the profile setting is not). Written
/// only into a profile that has no preferences yet; an existing profile keeps what Chrome stored.
pub fn seed_preferences(profile: &Path) -> std::io::Result<()> {
    let default = profile.join("Default");
    let file = default.join("Preferences");
    if std::fs::symlink_metadata(&file).is_ok() {
        return Ok(());
    }
    std::fs::create_dir_all(&default)?;
    std::fs::write(
        file,
        serde_json::json!({"intl":{"accept_languages":"es-ES,es","selected_languages":"es-ES,es"},"browser":{"check_default_browser":false}}).to_string(),
    )
}
