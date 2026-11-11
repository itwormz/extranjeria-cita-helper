//! Which installed browsers exist and which of them the app can drive. Reads only the application
//! bundles' metadata, never a browser profile, and never starts anything.
use crate::firefox::FirefoxInstallation;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserFamily {
    Firefox,
    Chrome,
    Brave,
    Edge,
    Chromium,
    Safari,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    /// An adapter with the full capability set exists.
    Supported,
    /// A Chromium adapter is planned; the browser is shown but never started.
    Planned,
    Unsupported(&'static str),
}
impl BrowserFamily {
    pub fn id(self) -> &'static str {
        match self {
            Self::Firefox => "firefox",
            Self::Chrome => "chrome",
            Self::Brave => "brave",
            Self::Edge => "edge",
            Self::Chromium => "chromium",
            Self::Safari => "safari",
        }
    }
}
impl Support {
    pub fn id(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Planned => "planned",
            Self::Unsupported(_) => "unsupported",
        }
    }
}
/// No Debug on the path: it can contain account names.
pub struct InstalledBrowser {
    pub family: BrowserFamily,
    pub name: String,
    pub version: String,
    pub executable: PathBuf,
    pub support: Support,
}
const KNOWN: [(BrowserFamily, &str, &str); 6] = [
    (BrowserFamily::Firefox, "Firefox", "firefox"),
    (BrowserFamily::Chrome, "Google Chrome", "Google Chrome"),
    (BrowserFamily::Brave, "Brave Browser", "Brave Browser"),
    (BrowserFamily::Edge, "Microsoft Edge", "Microsoft Edge"),
    (BrowserFamily::Chromium, "Chromium", "Chromium"),
    (BrowserFamily::Safari, "Safari", "Safari"),
];
/// Browsers the app can drive on this machine: macOS from the application bundles, other systems from the
/// known install locations. Edge is offered on Windows only (it is not tested elsewhere).
pub fn discover_installed() -> Vec<InstalledBrowser> {
    if cfg!(target_os = "macos") {
        return discover_browsers(&system_roots());
    }
    let mut found = Vec::new();
    if let Ok(installation) = FirefoxInstallation::discover() {
        found.push(InstalledBrowser { family: BrowserFamily::Firefox, name: "Firefox".into(), version: installation.version().into(), executable: installation.executable().to_owned(), support: Support::Supported });
    }
    for (family, kind, name) in [
        (BrowserFamily::Chrome, crate::firefox::ProfileKind::Chrome, "Google Chrome"),
        (BrowserFamily::Brave, crate::firefox::ProfileKind::Brave, "Brave Browser"),
        (BrowserFamily::Edge, crate::firefox::ProfileKind::Edge, "Microsoft Edge"),
        (BrowserFamily::Chromium, crate::firefox::ProfileKind::Chromium, "Chromium"),
    ] {
        if family == BrowserFamily::Edge && !cfg!(target_os = "windows") {
            continue;
        }
        if let Ok(installation) = crate::chromium::ChromiumInstallation::discover(kind) {
            found.push(InstalledBrowser { family, name: name.into(), version: installation.version().into(), executable: installation.executable().to_owned(), support: Support::Supported });
        }
    }
    found
}
/// Whether `path` is a snap package or the launcher stub of one (Ubuntu's `chromium-browser` and `firefox`). A snap
/// cannot read the hidden folder the app keeps its browser profile in, so such a browser is never started.
pub fn is_snap_install(path: &Path) -> bool {
    if path.starts_with("/snap") || path.canonicalize().is_ok_and(|real| real.starts_with("/snap")) {
        return true;
    }
    let Ok(meta) = fs::metadata(path) else { return false };
    if !meta.is_file() || meta.len() > 16 * 1024 {
        return false;
    }
    let Ok(bytes) = fs::read(path) else { return false };
    let text = String::from_utf8_lossy(&bytes);
    text.starts_with("#!") && text.contains("/snap/")
}
pub fn system_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }
    roots
}
fn bundle_version(contents: &Path) -> String {
    let path = contents.join("Info.plist");
    let Ok(metadata) = path.metadata() else { return String::new() };
    if metadata.len() > 1024 * 1024 {
        return String::new();
    }
    let Ok(raw) = fs::read_to_string(&path) else { return String::new() };
    let Some(start) = raw.find("<key>CFBundleShortVersionString</key>") else { return String::new() };
    let rest = &raw[start..];
    let Some(open) = rest.find("<string>") else { return String::new() };
    let rest = &rest[open + "<string>".len()..];
    let Some(close) = rest.find("</string>") else { return String::new() };
    let version = &rest[..close];
    if !version.is_empty() && version.len() <= 32 && version.chars().all(|c| c.is_ascii_alphanumeric() || c == '.') {
        version.to_owned()
    } else {
        String::new()
    }
}
/// Browsers found under `roots`, in a fixed family order; the first root that has a family wins.
pub fn discover_browsers(roots: &[PathBuf]) -> Vec<InstalledBrowser> {
    let mut found = Vec::new();
    for (family, bundle, executable) in KNOWN {
        for root in roots {
            let contents = root.join(format!("{bundle}.app/Contents"));
            let binary = contents.join("MacOS").join(executable);
            if !binary.is_file() {
                continue;
            }
            let entry = match family {
                BrowserFamily::Firefox => match FirefoxInstallation::from_executable(&binary) {
                    Ok(installation) => InstalledBrowser {
                        family,
                        name: bundle.into(),
                        version: installation.version().into(),
                        executable: binary,
                        support: Support::Supported,
                    },
                    Err(_) => continue,
                },
                BrowserFamily::Safari => InstalledBrowser {
                    family,
                    name: bundle.into(),
                    version: bundle_version(&contents),
                    executable: binary,
                    support: Support::Unsupported("no_input_automation"),
                },
                _ => InstalledBrowser {
                    family,
                    name: bundle.into(),
                    version: bundle_version(&contents),
                    executable: binary,
                    // Chrome, Brave and Chromium are driven over CDP; Edge only on Windows.
                    support: if matches!(family, BrowserFamily::Chrome | BrowserFamily::Brave | BrowserFamily::Chromium) { Support::Supported } else { Support::Planned },
                },
            };
            found.push(entry);
            break;
        }
    }
    found
}
