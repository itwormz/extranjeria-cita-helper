//! Explicit UI-requested notification tests and pairing. Does not touch browser/profile files.
use crate::{
    notification_editor::{WebhookConfig, validate_webhook_config},
    notification_pairing::{PairError, PairRegistry},
};
use ceh_services::notifications::{self, HttpDelivery, NotificationError, Telegram};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
    sync::Mutex,
    time::Instant,
};
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    ValidateWebhook {
        config: WebhookConfig,
    },
    CheckBot {
        token: String,
    },
    Chat {
        token: String,
        chat_id: String,
    },
    TelegramTest {
        token: String,
        chat_id: String,
        profile: String,
        province: String,
        language: String,
    },
    WebhookTest {
        config: WebhookConfig,
        profile: String,
        province: String,
        language: String,
    },
    BeginPair {
        token: String,
        group: bool,
    },
    PollPair {
        id: String,
    },
    CancelPair {
        id: String,
    },
    OpenPair {
        id: String,
    },
    CopyPair {
        id: String,
    },
}
#[derive(Debug, Serialize)]
pub struct ActionError {
    pub code: &'static str,
    pub status: Option<u16>,
}
fn error(code: &'static str) -> ActionError {
    ActionError { code, status: None }
}
impl From<NotificationError> for ActionError {
    fn from(e: NotificationError) -> Self {
        match e {
            NotificationError::ExistingWebhook => error("existing_webhook"),
            NotificationError::Expired | NotificationError::Stale => error("pair_expired"),
            NotificationError::InvalidConfig => error("invalid_config"),
            NotificationError::Http(status) => ActionError {
                code: "http",
                status: Some(status),
            },
            NotificationError::Network => error("network"),
            _ => error("invalid_response"),
        }
    }
}
impl From<PairError> for ActionError {
    fn from(e: PairError) -> Self {
        match e {
            PairError::Busy => error("pair_busy"),
            PairError::Missing | PairError::Expired => error("pair_expired"),
            PairError::Invalid => error("invalid_response"),
            PairError::Transport(e) => e.into(),
        }
    }
}
pub struct Actions {
    started: Instant,
    pairs: Mutex<PairRegistry>,
}
impl Default for Actions {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            pairs: Mutex::new(PairRegistry::default()),
        }
    }
}
fn milliseconds(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u64::MAX as u128) as u64
}
fn event(profile: &str, province: &str, language: &str) -> Result<Value, ActionError> {
    if profile.len() > 256 || province.len() > 256 || !["ru", "en", "es"].contains(&language) {
        return Err(error("invalid_config"));
    }
    Ok(notifications::make_event(
        "test", profile, province, language,
    )?)
}
fn client(token: &str) -> Result<Telegram, ActionError> {
    if token.len() > 256 {
        return Err(error("invalid_config"));
    }
    Ok(Telegram::new(token)?)
}
fn valid_chat(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && (id.strip_prefix('@').is_some_and(|s| {
            s.len() >= 5 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        }) || id
            .strip_prefix('-')
            .unwrap_or(id)
            .bytes()
            .all(|b| b.is_ascii_digit())
            && id != "-")
}
fn recipient(value: Value) -> Result<Value, ActionError> {
    let r: ceh_store::notification_metadata::Recipient =
        serde_json::from_value(value).map_err(|_| error("invalid_response"))?;
    if !valid_chat(&r.id) || r.label.chars().count() > 512 || r.username.len() > 256 {
        return Err(error("invalid_response"));
    }
    serde_json::to_value(r).map_err(|_| error("invalid_response"))
}
impl Actions {
    pub fn execute(&self, action: Action) -> Result<Value, ActionError> {
        match action {
            Action::ValidateWebhook { config } => {
                validate_webhook_config(&config).map_err(|_| error("invalid_config"))?;
                Ok(Value::Null)
            }
            Action::CheckBot { token } => {
                let token = zeroize::Zeroizing::new(token);
                let value = client(&token)?.check(&HttpDelivery::new()?)?;
                let username = value["username"]
                    .as_str()
                    .filter(|v| v.len() <= 64)
                    .ok_or_else(|| error("invalid_response"))?;
                Ok(json!({"username":username}))
            }
            Action::Chat { token, chat_id } => {
                let token = zeroize::Zeroizing::new(token);
                if !valid_chat(&chat_id) {
                    return Err(error("invalid_config"));
                }
                recipient(client(&token)?.chat(&HttpDelivery::new()?, &chat_id)?)
            }
            Action::TelegramTest {
                token,
                chat_id,
                profile,
                province,
                language,
            } => {
                let token = zeroize::Zeroizing::new(token);
                if !valid_chat(&chat_id) {
                    return Err(error("invalid_config"));
                }
                let data = event(&profile, &province, &language)?;
                let elapsed = client(&token)?.send(
                    &HttpDelivery::new()?,
                    &chat_id,
                    data["message"]
                        .as_str()
                        .ok_or_else(|| error("invalid_config"))?,
                    || true,
                )?;
                Ok(json!({"elapsed_ms":elapsed}))
            }
            Action::WebhookTest {
                config,
                profile,
                province,
                language,
            } => {
                validate_webhook_config(&config).map_err(|_| error("invalid_config"))?;
                let data = event(&profile, &province, &language)?;
                let config = ceh_services::secrets::SecretData::new(
                    serde_json::to_value(&config).map_err(|_| error("invalid_config"))?,
                )
                .map_err(|_| error("invalid_config"))?;
                let request = notifications::webhook_request(config.value(), &data)?;
                let receipt = HttpDelivery::new()?.send(&request, 1, || true)?;
                Ok(json!({"status":receipt.status,"elapsed_ms":receipt.elapsed_ms}))
            }
            Action::BeginPair { token, group } => {
                let token = zeroize::Zeroizing::new(token);
                let _ = client(&token)?;
                let ticket = self.pairs.lock().map_err(|_| error("unavailable"))?.start(
                    &token,
                    group,
                    milliseconds(self.started),
                    |token, _| {
                        Telegram::new(token)?
                            .begin_pairing(&HttpDelivery::new()?, milliseconds(self.started))
                    },
                )?;
                serde_json::to_value(ticket).map_err(|_| error("unavailable"))
            }
            Action::PollPair { id } => {
                if id.len() > 64 {
                    return Err(error("pair_expired"));
                }
                let found = self.pairs.lock().map_err(|_| error("unavailable"))?.poll(
                    &id,
                    milliseconds(self.started),
                    |token, pair, group, _| {
                        Telegram::new(token)?.poll(&HttpDelivery::new()?, pair, group, || {
                            milliseconds(self.started)
                        })
                    },
                )?;
                found
                    .map(|r| serde_json::to_value(r).map_err(|_| error("invalid_response")))
                    .unwrap_or(Ok(Value::Null))
            }
            Action::CancelPair { id } => {
                self.pairs
                    .lock()
                    .map_err(|_| error("unavailable"))?
                    .cancel(&id);
                Ok(Value::Null)
            }
            Action::OpenPair { id } => {
                let link = self
                    .pairs
                    .lock()
                    .map_err(|_| error("unavailable"))?
                    .link(&id, milliseconds(self.started))?;
                open(std::ffi::OsStr::new(&link))?;
                Ok(Value::Null)
            }
            Action::CopyPair { id } => {
                let link = self
                    .pairs
                    .lock()
                    .map_err(|_| error("unavailable"))?
                    .link(&id, milliseconds(self.started))?;
                clipboard(&link)?;
                Ok(Value::Null)
            }
        }
    }
}
pub fn open_directory(path: &std::path::Path) -> Result<(), ActionError> {
    open(path.as_os_str())
}
pub(crate) fn open(target: &std::ffi::OsStr) -> Result<(), ActionError> {
    #[cfg(target_os = "macos")]
    let mut command = Command::new("/usr/bin/open");
    #[cfg(windows)]
    let mut command = Command::new("explorer.exe");
    #[cfg(target_os = "linux")]
    let mut command = Command::new("xdg-open");
    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    return Err(error("unavailable"));
    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    {
        let mut child = command
            .arg(target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| error("unavailable"))?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}
pub(crate) fn clipboard(text: &str) -> Result<(), ActionError> {
    #[cfg(target_os = "macos")]
    let programs: Vec<(&str, Vec<&str>)> = vec![("/usr/bin/pbcopy", vec![])];
    #[cfg(windows)]
    let programs: Vec<(&str, Vec<&str>)> = vec![(
        "powershell.exe",
        vec![
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Console]::In.ReadToEnd() | Set-Clipboard",
        ],
    )];
    #[cfg(target_os = "linux")]
    let programs: Vec<(&str, Vec<&str>)> = vec![
        ("wl-copy", vec![]),
        ("xclip", vec!["-selection", "clipboard"]),
    ];
    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    return Err(error("unavailable"));
    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    {
        for (program, args) in programs {
            if let Ok(mut child) = Command::new(program)
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                let wrote = child
                    .stdin
                    .take()
                    .is_some_and(|mut input| input.write_all(text.as_bytes()).is_ok());
                let completed = child.wait().is_ok_and(|s| s.success());
                if wrote && completed {
                    return Ok(());
                }
            }
        }
        Err(error("unavailable"))
    }
}

/// The caller resolves a fixed application-owned URL before reaching this native adapter.
pub fn open_url(url: &str) -> Result<(), &'static str> {
    open(std::ffi::OsStr::new(url)).map_err(|_| "open_failed")
}
