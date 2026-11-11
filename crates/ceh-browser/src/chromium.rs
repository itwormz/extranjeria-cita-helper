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
        // Closing the last window ends the browser instead of leaving a background process (Windows).
        "--disable-background-mode".to_owned(),
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

/// Windows: `chrome.exe`/`msedge.exe` print nothing for `--version`, but the installation keeps the
/// version in a numbered folder beside the executable (`Application\157.0.4341.46\`); the highest wins.
pub fn version_from_install_folder(application: &Path) -> Option<String> {
    std::fs::read_dir(application)
        .ok()?
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|name| {
            let parts: Vec<u64> = name.split('.').map(|p| p.parse().ok()).collect::<Option<_>>()?;
            (parts.len() == 4 && name.len() <= 32).then_some((parts, name))
        })
        .max()
        .map(|(_, name)| name)
}

/// Where each system keeps each browser: the program itself, not only the usual links to it.
pub fn candidate_paths(kind: ProfileKind, os: crate::launch::TargetOs, windows_bases: &[PathBuf]) -> Vec<PathBuf> {
    use crate::launch::TargetOs;
    match os {
        TargetOs::Macos => match kind {
            ProfileKind::Chrome => vec!["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".into()],
            ProfileKind::Brave => vec!["/Applications/Brave Browser.app/Contents/MacOS/Brave Browser".into()],
            ProfileKind::Edge => vec!["/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge".into()],
            ProfileKind::Chromium => vec!["/Applications/Chromium.app/Contents/MacOS/Chromium".into()],
            ProfileKind::Firefox => Vec::new(),
        },
        TargetOs::Windows => {
            let relative = match kind {
                ProfileKind::Chrome => "Google/Chrome/Application/chrome.exe",
                ProfileKind::Brave => "BraveSoftware/Brave-Browser/Application/brave.exe",
                ProfileKind::Edge => "Microsoft/Edge/Application/msedge.exe",
                ProfileKind::Chromium => "Chromium/Application/chrome.exe",
                ProfileKind::Firefox => return Vec::new(),
            };
            windows_bases.iter().map(|base| base.join(relative)).collect()
        }
        TargetOs::Linux => {
            let paths: &[&str] = match kind {
                ProfileKind::Chrome => &["/usr/bin/google-chrome", "/usr/bin/google-chrome-stable", "/opt/google/chrome/chrome", "/opt/google/chrome/google-chrome"],
                ProfileKind::Brave => &["/usr/bin/brave-browser", "/usr/bin/brave", "/opt/brave.com/brave/brave-browser", "/opt/brave.com/brave/brave"],
                ProfileKind::Edge => &["/usr/bin/microsoft-edge", "/usr/bin/microsoft-edge-stable", "/opt/microsoft/msedge/microsoft-edge", "/opt/microsoft/msedge/msedge"],
                // Ubuntu's `chromium-browser` is a stub for the snap: `is_snap_install` turns it away.
                ProfileKind::Chromium => &["/usr/lib/chromium/chromium", "/usr/lib/chromium-browser/chromium-browser", "/usr/bin/chromium", "/usr/bin/chromium-browser"],
                ProfileKind::Firefox => &[],
            };
            paths.iter().map(PathBuf::from).collect()
        }
    }
}
/// The program path out of a Windows `App Paths` value: an absolute `X:\...\name.exe`, possibly quoted, with no `..`,
/// no arguments and nothing else.
pub fn app_path_from_value(raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim();
    let inner = trimmed.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')).unwrap_or(trimmed);
    let bytes = inner.as_bytes();
    let drive = bytes.len() > 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/');
    let clean = !inner.contains('"') && inner.to_ascii_lowercase().ends_with(".exe") && !inner.split(['\\', '/']).any(|part| part == "..");
    (drive && clean).then(|| PathBuf::from(inner))
}
/// Chrome and Chromium both register `chrome.exe`: a registered path counts only for the browser whose folder it is.
pub fn registry_path_fits(kind: ProfileKind, path: &Path) -> bool {
    let text = path.to_string_lossy().to_ascii_lowercase().replace('\\', "/");
    match kind {
        ProfileKind::Chrome => text.contains("/google/") && !text.contains("chromium"),
        ProfileKind::Chromium => text.contains("/chromium/"),
        ProfileKind::Brave => text.contains("brave"),
        ProfileKind::Edge => text.contains("/microsoft/edge"),
        ProfileKind::Firefox => false,
    }
}
/// The program paths Windows has registered under `App Paths` for `exe` (HKLM 64 and 32 bit, then HKCU).
pub fn registry_app_paths(exe: &str) -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        use winreg::{RegKey, enums::*};
        let sub = format!("Software\\Microsoft\\Windows\\CurrentVersion\\App Paths\\{exe}");
        let mut found = Vec::new();
        for (hive, flags) in [(HKEY_LOCAL_MACHINE, KEY_READ | KEY_WOW64_64KEY), (HKEY_LOCAL_MACHINE, KEY_READ | KEY_WOW64_32KEY), (HKEY_CURRENT_USER, KEY_READ)] {
            if let Ok(key) = RegKey::predef(hive).open_subkey_with_flags(&sub, flags)
                && let Ok(value) = key.get_value::<String, _>("")
                && let Some(path) = app_path_from_value(&value)
                && !found.contains(&path)
            {
                found.push(path);
            }
        }
        found
    }
    #[cfg(not(windows))]
    {
        let _ = exe;
        Vec::new()
    }
}
fn registered_paths(kind: ProfileKind) -> Vec<PathBuf> {
    let exe = match kind {
        ProfileKind::Chrome | ProfileKind::Chromium => "chrome.exe",
        ProfileKind::Brave => "brave.exe",
        ProfileKind::Edge => "msedge.exe",
        ProfileKind::Firefox => return Vec::new(),
    };
    registry_app_paths(exe).into_iter().filter(|path| registry_path_fits(kind, path)).collect()
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
        // A snap cannot read the app's profile folder (confinement), so it is never started.
        if crate::discovery::is_snap_install(path) {
            return Err(FirefoxError::InvalidInstallation);
        }
        let executable = path.canonicalize().map_err(|_| FirefoxError::NotInstalled)?;
        if !executable.is_file() {
            return Err(FirefoxError::InvalidInstallation);
        }
        let version = if cfg!(windows) {
            executable.parent().and_then(version_from_install_folder).ok_or(FirefoxError::InvalidInstallation)?
        } else {
            let output = std::process::Command::new(&executable)
                .arg("--version")
                .stdin(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output()
                .map_err(|_| FirefoxError::InvalidInstallation)?;
            parse_version(&String::from_utf8_lossy(&output.stdout)).ok_or(FirefoxError::InvalidInstallation)?
        };
        Ok(Self { kind, executable, version })
    }
    /// Fixed known install locations (plus, on Windows, where the installer registered the program); never inspect the
    /// person's browser profiles and never search `PATH`.
    pub fn discover(kind: ProfileKind) -> Result<Self, FirefoxError> {
        let os = crate::launch::TargetOs::current();
        let mut bases: Vec<PathBuf> = Vec::new();
        if cfg!(target_os = "windows") {
            for variable in ["ProgramFiles", "ProgramFiles(x86)", "LocalAppData"] {
                bases.extend(std::env::var_os(variable).map(PathBuf::from));
            }
        }
        let mut candidates = candidate_paths(kind, os, &bases);
        if cfg!(target_os = "windows") {
            candidates.extend(registered_paths(kind));
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
