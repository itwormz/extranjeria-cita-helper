//! Lazy credential editing; no paths or credential references are accepted from UI drafts.
use ceh_core::{
    profiles::{ProfileDraft, validate_desktop_profile},
    timings::parse_timings,
};
use ceh_services::{
    notifications::{Telegram, webhook_request},
    secrets::{Backend, SecretData, Vault, VaultError},
};
use ceh_store::{
    notification_metadata::{NotificationMetadata, Recipient},
    profile_write::{ProfileEdits, ProfileSaved, ProfileWriteError, save_profile_changes},
    profiles::read_profiles,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{path::Path, sync::LazyLock};
use zeroize::Zeroize;
const EVENTS: [&str; 4] = ["auth", "office", "booking", "error"];
fn events() -> Vec<String> {
    EVENTS.iter().map(|s| s.to_string()).collect()
}
fn post() -> String {
    "POST".into()
}
fn standard() -> String {
    "standard".into()
}
fn hidden() -> bool {
    true
}
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TelegramDraft {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "events")]
    pub events: Vec<String>,
    #[serde(default)]
    pub recipients: Vec<Recipient>,
    #[serde(default)]
    pub token: String,
}
impl Drop for TelegramDraft {
    fn drop(&mut self) {
        self.token.zeroize();
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedValue {
    pub name: String,
    pub value: String,
    #[serde(default = "hidden")]
    pub secret: bool,
}
impl Drop for NamedValue {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebhookConfig {
    #[serde(default)]
    pub url: String,
    #[serde(default = "post")]
    pub method: String,
    #[serde(default)]
    pub headers: Vec<NamedValue>,
    #[serde(default)]
    pub query: Vec<NamedValue>,
    #[serde(default = "standard")]
    pub body_mode: String,
    #[serde(default)]
    pub body: String,
}
impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            method: post(),
            headers: vec![],
            query: vec![],
            body_mode: standard(),
            body: String::new(),
        }
    }
}
impl Drop for WebhookConfig {
    fn drop(&mut self) {
        self.url.zeroize();
        self.body.zeroize();
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebhookDraft {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "events")]
    pub events: Vec<String>,
    #[serde(default)]
    pub config: WebhookConfig,
}
impl Default for WebhookDraft {
    fn default() -> Self {
        Self {
            enabled: false,
            events: events(),
            config: WebhookConfig::default(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotificationDraft {
    pub telegram: TelegramDraft,
    pub webhook: WebhookDraft,
}
#[derive(Serialize)]
pub struct NotificationEditor {
    pub revision: String,
    pub draft: NotificationDraft,
}
#[derive(Debug)]
pub enum EditError {
    Profile(ProfileWriteError),
    Secret(VaultError),
    Invalid,
}
impl From<ProfileWriteError> for EditError {
    fn from(error: ProfileWriteError) -> Self {
        Self::Profile(error)
    }
}
impl From<VaultError> for EditError {
    fn from(error: VaultError) -> Self {
        Self::Secret(error)
    }
}
fn metadata(
    root: &Path,
    id: Option<&str>,
    revision: &str,
) -> Result<NotificationMetadata, EditError> {
    let bundle = read_profiles(&root.join("tasks.yml"), crate::catalog(), "ru")
        .map_err(|_| ProfileWriteError::Read)?;
    if bundle.revision() != revision {
        return Err(ProfileWriteError::Conflict.into());
    }
    let task = if let Some(id) = id {
        Some(
            bundle
                .raw()
                .root()
                .get("tasks")
                .and_then(|v| v.sequence())
                .and_then(|items| {
                    items
                        .iter()
                        .find(|t| t.get("id").and_then(|s| s.scalar_text()).as_deref() == Some(id))
                })
                .ok_or(ProfileWriteError::Conflict)?
                .as_ref(),
        )
    } else {
        None
    };
    Ok(NotificationMetadata::from_task(task)?)
}
fn secret<B: Backend>(vault: &Vault<B>, reference: &str) -> Result<SecretData, EditError> {
    Ok(vault.get(reference)?)
}
fn projected_config(value: &Value) -> Result<WebhookConfig, EditError> {
    let mut output = json!({});
    for key in ["url", "method", "headers", "query", "body_mode", "body"] {
        if let Some(v) = value.get(key) {
            output[key] = v.clone();
        }
    }
    for key in ["headers", "query"] {
        if let Some(items) = output.get_mut(key).and_then(Value::as_array_mut) {
            for row in items {
                if let Some(object) = row.as_object_mut() {
                    object.retain(|key, _| ["name", "value", "secret"].contains(&key.as_str()));
                }
            }
        }
    }
    serde_json::from_value(output).map_err(|_| EditError::Invalid)
}
fn from_secrets(
    metadata: &NotificationMetadata,
    tg: &Value,
    webhook: &Value,
) -> Result<NotificationDraft, EditError> {
    Ok(NotificationDraft {
        telegram: TelegramDraft {
            enabled: metadata.telegram.channel.enabled,
            events: metadata.telegram.channel.events.clone(),
            recipients: metadata.telegram.recipients.clone(),
            token: tg.get("token").map_or(Ok(String::new()), |v| {
                v.as_str().map(str::to_owned).ok_or(EditError::Invalid)
            })?,
        },
        webhook: WebhookDraft {
            enabled: metadata.webhook.enabled,
            events: metadata.webhook.events.clone(),
            config: projected_config(webhook)?,
        },
    })
}
pub fn editor<B: Backend>(
    root: &Path,
    id: Option<&str>,
    revision: &str,
    vault: &Vault<B>,
) -> Result<NotificationEditor, EditError> {
    let metadata = metadata(root, id, revision)?;
    let tg = secret(vault, &metadata.telegram.channel.secret_ref)?;
    let hook = secret(vault, &metadata.webhook.secret_ref)?;
    Ok(NotificationEditor {
        revision: revision.into(),
        draft: from_secrets(&metadata, tg.value(), hook.value())?,
    })
}
fn validate(draft: &NotificationDraft) -> Result<(), EditError> {
    let valid_events = |items: &[String]| {
        items.len() <= 4
            && items.iter().all(|s| EVENTS.contains(&s.as_str()))
            && items
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == items.len()
    };
    static CHAT: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^(?:-?[0-9]+|@[A-Za-z0-9_]{5,})$").unwrap());
    let tg = &draft.telegram;
    let hook = &draft.webhook;
    let config = &hook.config;
    if !valid_events(&tg.events)
        || !valid_events(&hook.events)
        || tg.token.len() > 256
        || tg.recipients.len() > 32
        || tg.recipients.iter().any(|r| {
            !CHAT.is_match(&r.id)
                || r.id.len() > 128
                || r.label.chars().count() > 512
                || r.username.len() > 256
                || !["private", "group", "supergroup", "channel"].contains(&r.r#type.as_str())
        })
        || config.url.len() > 4096
        || config.body.len() > 16384
        || config.headers.len() > 32
        || config.query.len() > 32
        || config
            .headers
            .iter()
            .chain(&config.query)
            .any(|r| r.name.len() > 256 || r.value.len() > 4096)
        || !["POST", "GET"].contains(&config.method.as_str())
        || !["standard", "custom"].contains(&config.body_mode.as_str())
    {
        return Err(EditError::Invalid);
    }
    if tg.enabled && (tg.recipients.is_empty() || Telegram::new(tg.token.trim()).is_err()) {
        return Err(EditError::Invalid);
    }
    if hook.enabled {
        webhook_request(&serde_json::to_value(config).map_err(|_|EditError::Invalid)?,&json!({"message":"Fixture","event":"test","profile":"Fixture","province":"Fixture","timestamp":"2026-10-31T12:00:00Z","event_id":"fixture"})).map_err(|_|EditError::Invalid)?;
    }
    Ok(())
}
struct Prepared<'a, B: Backend> {
    vault: &'a Vault<B>,
    created: Vec<String>,
    committed: bool,
}
impl<B: Backend> Drop for Prepared<'_, B> {
    fn drop(&mut self) {
        if !self.committed {
            for reference in &self.created {
                self.vault.delete(reference);
            }
        }
    }
}
impl<B: Backend> Prepared<'_, B> {
    fn put(&mut self, value: Value) -> Result<String, EditError> {
        if value.as_object().is_some_and(|o| o.is_empty()) {
            return Ok(String::new());
        }
        let reference = self.vault.put(&SecretData::new(value)?)?;
        self.created.push(reference.clone());
        Ok(reference)
    }
}
/// No notification draft means zero vault access, including construction/reading old secrets.
pub fn save<B: Backend>(
    root: &Path,
    id: Option<&str>,
    draft: ProfileDraft,
    revision: &str,
    timings: Option<&Value>,
    notifications: Option<&NotificationDraft>,
    vault: &Vault<B>,
) -> Result<ProfileSaved, EditError> {
    let mut validation_draft = draft.clone();
    validation_draft.id = id.unwrap_or("new_profile").into();
    let report = validate_desktop_profile(validation_draft, crate::catalog());
    if !report.errors.is_empty() {
        return Err(ProfileWriteError::Validation(report.errors).into());
    }
    if timings.is_some_and(|v| parse_timings(v).is_err()) {
        return Err(ProfileWriteError::Validation(vec!["Invalid profile timings".into()]).into());
    }
    let Some(draft_notifications) = notifications else {
        return Ok(save_profile_changes(
            &root.join("tasks.yml"),
            id,
            draft,
            revision,
            crate::catalog(),
            ProfileEdits {
                timings,
                notifications: None,
            },
        )?);
    };
    validate(draft_notifications)?;
    let mut metadata = metadata(root, id, revision)?;
    let tg = secret(vault, &metadata.telegram.channel.secret_ref)?;
    let hook = secret(vault, &metadata.webhook.secret_ref)?;
    let original = from_secrets(&metadata, tg.value(), hook.value())?;
    let mut prepared = Prepared {
        vault,
        created: vec![],
        committed: false,
    };
    if draft_notifications.telegram.token != original.telegram.token {
        let mut next = tg.value().clone();
        if draft_notifications.telegram.token.trim().is_empty() {
            next.as_object_mut()
                .ok_or(EditError::Invalid)?
                .remove("token");
        } else {
            next["token"] = json!(draft_notifications.telegram.token.trim());
        }
        metadata.telegram.channel.secret_ref = prepared.put(next)?;
    }
    if serde_json::to_value(&draft_notifications.webhook.config).map_err(|_| EditError::Invalid)?
        != serde_json::to_value(&original.webhook.config).map_err(|_| EditError::Invalid)?
    {
        let mut next = hook.value().clone();
        next.as_object_mut().ok_or(EditError::Invalid)?.extend(
            serde_json::to_value(&draft_notifications.webhook.config)
                .map_err(|_| EditError::Invalid)?
                .as_object()
                .ok_or(EditError::Invalid)?
                .clone(),
        );
        metadata.webhook.secret_ref = prepared.put(next)?;
    }
    metadata.telegram.channel.enabled = draft_notifications.telegram.enabled;
    metadata.telegram.channel.events = draft_notifications.telegram.events.clone();
    metadata.telegram.recipients = draft_notifications.telegram.recipients.clone();
    metadata.webhook.enabled = draft_notifications.webhook.enabled;
    metadata.webhook.events = draft_notifications.webhook.events.clone();
    let saved = save_profile_changes(
        &root.join("tasks.yml"),
        id,
        draft,
        revision,
        crate::catalog(),
        ProfileEdits {
            timings,
            notifications: Some(&metadata),
        },
    )?;
    // Retain old references: other profiles/backups may share them. Only failed new refs are rolled back.
    prepared.committed = true;
    Ok(saved)
}

/// Same bounds/validation for an explicit test request as for persisted settings.
pub fn validate_webhook_config(config: &WebhookConfig) -> Result<(), EditError> {
    validate(&NotificationDraft {
        telegram: TelegramDraft::default(),
        webhook: WebhookDraft {
            enabled: true,
            events: events(),
            config: config.clone(),
        },
    })
}
