//! Camoufox startup transport. Identity generation is a separate build-time contract.
//! No profile values, environment or identity contents are exposed through errors/logs.
use serde_json::{Map, Value, json};
use std::path::PathBuf;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetOs {
    Macos,
    Windows,
    Linux,
}
impl TargetOs {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Macos => "macos",
            Self::Windows => "windows",
            Self::Linux => "linux",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchError {
    InvalidIdentity,
    IncompatibleBrowser,
    InvalidPath,
    Unavailable,
    StartupTimeout,
}
/// Trusted SDK output only; deliberately no Debug/Serialize implementation.
pub struct LaunchIdentity {
    config: Map<String, Value>,
    preferences: Map<String, Value>,
    os: TargetOs,
}
impl LaunchIdentity {
    pub fn decode(raw: &str, browser_major: u32, os: TargetOs) -> Result<Self, LaunchError> {
        if raw.len() > 256 * 1024 || browser_major < 100 {
            return Err(LaunchError::InvalidIdentity);
        }
        let data: Value = serde_json::from_str(raw).map_err(|_| LaunchError::InvalidIdentity)?;
        let data = data
            .as_object()
            .filter(|v| v.len() == 6)
            .ok_or(LaunchError::InvalidIdentity)?;
        if data.get("schema") != Some(&json!(1))
            || data.get("sdk").and_then(Value::as_str) != Some("0.5.7")
        {
            return Err(LaunchError::InvalidIdentity);
        }
        if data.get("browser_major").and_then(Value::as_u64) != Some(u64::from(browser_major))
            || data.get("target_os").and_then(Value::as_str) != Some(os.name())
        {
            return Err(LaunchError::IncompatibleBrowser);
        }
        let config = data
            .get("config")
            .and_then(Value::as_object)
            .filter(|v| !v.is_empty() && v.len() <= 512)
            .ok_or(LaunchError::InvalidIdentity)?
            .clone();
        let preferences = data
            .get("preferences")
            .and_then(Value::as_object)
            .filter(|v| v.len() <= 512)
            .ok_or(LaunchError::InvalidIdentity)?
            .clone();
        let ua = config
            .get("navigator.userAgent")
            .and_then(Value::as_str)
            .ok_or(LaunchError::InvalidIdentity)?;
        if !ua.contains(&format!("Firefox/{browser_major}.")) {
            return Err(LaunchError::IncompatibleBrowser);
        }
        if preferences.iter().any(|(k, v)| {
            k.is_empty()
                || k.len() > 256
                || k.chars().any(char::is_control)
                || !matches!(v, Value::String(_) | Value::Bool(_) | Value::Number(_))
        }) {
            return Err(LaunchError::InvalidIdentity);
        }
        Ok(Self {
            config,
            preferences,
            os,
        })
    }
    /// Only the fixed launcher chooses identity/platform/add-ons; never page or UI IPC data.
    pub fn environment(
        &self,
        timezone: &str,
        os: TargetOs,
        addons: &[PathBuf],
    ) -> Result<Vec<(String, String)>, LaunchError> {
        if os != self.os {
            return Err(LaunchError::IncompatibleBrowser);
        }
        if timezone.is_empty()
            || timezone.len() > 128
            || !timezone
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"/_+-".contains(&c))
        {
            return Err(LaunchError::InvalidIdentity);
        }
        if addons.len() > 8
            || addons
                .iter()
                .any(|p| !p.is_absolute() || !p.join("manifest.json").is_file())
        {
            return Err(LaunchError::InvalidPath);
        }
        let mut config = self.config.clone();
        let mut prefs = self.preferences.clone();
        config.insert("timezone".into(), json!(timezone));
        config.insert("locale:language".into(), json!("es"));
        config.insert("locale:region".into(), json!("ES"));
        config.insert("humanize".into(), json!(true));
        config.insert("humanize:maxTime".into(), json!(1.5));
        config.insert("allowMainWorld".into(), json!(true));
        // Build-machine absolute addon paths must never survive relocation.
        config.remove("addons");
        if !addons.is_empty() {
            config.insert(
                "addons".into(),
                serde_json::to_value(addons).map_err(|_| LaunchError::InvalidPath)?,
            );
        }
        for key in [
            "security.enterprise_roots.enabled",
            "security.osclientcerts.autoload",
            "dom.disable_window_flip",
            "browser.tabs.loadDivertedInBackground",
        ] {
            prefs.insert(key.into(), json!(true));
        }
        prefs.insert("intl.locale.requested".into(), json!("es-ES"));
        let mut out = chunks("CAMOU_CONFIG", &Value::Object(config), os)?;
        out.extend(chunks("CAMOU_PREFS", &Value::Object(prefs), os)?);
        Ok(out)
    }
}
fn chunks(prefix: &str, value: &Value, os: TargetOs) -> Result<Vec<(String, String)>, LaunchError> {
    let raw = serde_json::to_string(value).map_err(|_| LaunchError::InvalidIdentity)?;
    // ASCII JSON survives Windows' autoconfig getenv ANSI conversion. Escape UTF-16
    // code units, including surrogate pairs, so chunk boundaries cannot corrupt UTF-8.
    let mut ascii = String::with_capacity(raw.len());
    for c in raw.chars() {
        if c.is_ascii() {
            ascii.push(c);
        } else {
            let mut buf = [0; 2];
            for unit in c.encode_utf16(&mut buf) {
                use std::fmt::Write;
                write!(&mut ascii, "\\u{unit:04x}").map_err(|_| LaunchError::InvalidIdentity)?;
            }
        }
    }
    if ascii.len() > 1024 * 1024 {
        return Err(LaunchError::InvalidIdentity);
    }
    let size = if os == TargetOs::Windows { 2047 } else { 32767 };
    Ok(ascii
        .as_bytes()
        .chunks(size)
        .enumerate()
        .map(|(i, c)| {
            (
                format!("{prefix}_{}", i + 1),
                String::from_utf8(c.to_vec()).expect("ASCII chunks"),
            )
        })
        .collect())
}
