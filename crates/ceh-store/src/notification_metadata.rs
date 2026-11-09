//! Only non-secret channel settings belong in YAML. Credentials live behind references.
use crate::{legacy_yaml::RawValue, profile_write::ProfileWriteError};
use serde::{Deserialize, Serialize};
const EVENTS: [&str; 4] = ["auth", "office", "booking", "error"];
fn events() -> Vec<String> {
    EVENTS.iter().map(|s| s.to_string()).collect()
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Recipient {
    pub id: String,
    pub label: String,
    #[serde(default = "private")]
    pub r#type: String,
    #[serde(default)]
    pub username: String,
}
fn private() -> String {
    "private".into()
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Channel {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "events")]
    pub events: Vec<String>,
    #[serde(default)]
    pub secret_ref: String,
}
impl Default for Channel {
    fn default() -> Self {
        Self {
            enabled: false,
            events: events(),
            secret_ref: String::new(),
        }
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Telegram {
    #[serde(flatten)]
    pub channel: Channel,
    #[serde(default)]
    pub recipients: Vec<Recipient>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct NotificationMetadata {
    #[serde(default)]
    pub telegram: Telegram,
    #[serde(default)]
    pub webhook: Channel,
}
fn text(value: Option<&RawValue>, default: &str) -> Result<String, ProfileWriteError> {
    match value {
        None => Ok(default.into()),
        Some(RawValue::String(s)) => Ok(s.clone()),
        _ => Err(ProfileWriteError::Read),
    }
}
fn channel(raw: Option<&RawValue>) -> Result<Channel, ProfileWriteError> {
    let Some(raw) = raw else {
        return Ok(Channel::default());
    };
    if !raw.is_mapping() {
        return Err(ProfileWriteError::Read);
    }
    let events = if let Some(v) = raw.get("events") {
        v.sequence()
            .ok_or(ProfileWriteError::Read)?
            .iter()
            .map(|v| text(Some(v), ""))
            .collect::<Result<_, _>>()?
    } else {
        events()
    };
    Ok(Channel {
        enabled: raw.get("enabled").is_some_and(RawValue::truthy),
        events,
        secret_ref: text(raw.get("secret_ref"), "")?,
    })
}
impl NotificationMetadata {
    pub fn from_task(task: Option<&RawValue>) -> Result<Self, ProfileWriteError> {
        let Some(raw) = task.and_then(|t| t.get("notifications")) else {
            return Ok(Self::default());
        };
        if !raw.is_mapping() {
            return Err(ProfileWriteError::Read);
        }
        let tg = raw.get("telegram");
        let mut result = Self {
            telegram: Telegram {
                channel: channel(tg)?,
                recipients: vec![],
            },
            webhook: channel(raw.get("webhook"))?,
        };
        if let Some(raw) = tg.and_then(|t| t.get("recipients")) {
            for r in raw.sequence().ok_or(ProfileWriteError::Read)? {
                result.telegram.recipients.push(Recipient {
                    id: r
                        .get("id")
                        .and_then(RawValue::scalar_text)
                        .ok_or(ProfileWriteError::Read)?,
                    label: text(r.get("label"), "")?,
                    r#type: text(r.get("type"), "private")?,
                    username: text(r.get("username"), "")?,
                });
            }
        }
        result.validate()?;
        Ok(result)
    }
    pub fn validate(&self) -> Result<(), ProfileWriteError> {
        let valid = [&self.telegram.channel, &self.webhook].iter().all(|c| {
            c.events.len() <= 4
                && c.events.iter().all(|s| EVENTS.contains(&s.as_str()))
                && c.events
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    == c.events.len()
                && (c.secret_ref.is_empty()
                    || (c.secret_ref.starts_with("notifications-")
                        && c.secret_ref.len() <= 256
                        && c.secret_ref
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-')))
        }) && self.telegram.recipients.len() <= 32
            && self.telegram.recipients.iter().all(|r| {
                !r.id.is_empty()
                    && r.id.len() <= 128
                    && r.label.len() <= 512
                    && r.username.len() <= 256
                    && ["private", "group", "supergroup", "channel"].contains(&r.r#type.as_str())
            });
        if valid {
            Ok(())
        } else {
            Err(ProfileWriteError::Validation(vec![
                "Invalid notification settings".into(),
            ]))
        }
    }
}
