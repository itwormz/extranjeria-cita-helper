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
const KNOWN: [(BrowserFamily, &str, &str); 5] = [
    (BrowserFamily::Firefox, "Firefox", "firefox"),
    (BrowserFamily::Chrome, "Google Chrome", "Google Chrome"),
    (BrowserFamily::Brave, "Brave Browser", "Brave Browser"),
    (BrowserFamily::Edge, "Microsoft Edge", "Microsoft Edge"),
    (BrowserFamily::Safari, "Safari", "Safari"),
];
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
                    support: Support::Planned,
                },
            };
            found.push(entry);
            break;
        }
    }
    found
}
