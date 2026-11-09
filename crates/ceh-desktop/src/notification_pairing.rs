//! Own pairing sessions only; no persisted tokens, single poller per bot, bounded lifetime.
use ceh_services::notifications::{NotificationError, Pairing, Telegram};
use ceh_store::notification_metadata::Recipient;
use serde::Serialize;
use std::collections::HashMap;
use zeroize::Zeroizing;
#[derive(Debug)]
pub enum PairError {
    Busy,
    Missing,
    Expired,
    Invalid,
    Transport(NotificationError),
}
impl From<NotificationError> for PairError {
    fn from(error: NotificationError) -> Self {
        Self::Transport(error)
    }
}
#[derive(Serialize)]
pub struct PairTicket {
    pub id: String,
    pub link: String,
    pub expires_in_seconds: u32,
}
struct Session {
    token: Zeroizing<String>,
    pair: Pairing,
    group: bool,
}
#[derive(Default)]
pub struct PairRegistry {
    sessions: HashMap<String, Session>,
}
impl PairRegistry {
    pub fn start(
        &mut self,
        token: &str,
        group: bool,
        now: u64,
        create: impl FnOnce(&str, u64) -> Result<Pairing, NotificationError>,
    ) -> Result<PairTicket, PairError> {
        if token.len() > 256 || Telegram::new(token).is_err() {
            return Err(PairError::Invalid);
        }
        self.sessions.retain(|_, s| !s.pair.expired(now));
        if self.sessions.len() >= 4 || self.sessions.values().any(|s| s.token.as_str() == token) {
            return Err(PairError::Busy);
        }
        let pair = create(token, now)?;
        let id = uuid::Uuid::new_v4().to_string();
        let link = pair.link(group);
        if link.len() > 512 {
            return Err(PairError::Invalid);
        }
        self.sessions.insert(
            id.clone(),
            Session {
                token: Zeroizing::new(token.into()),
                pair,
                group,
            },
        );
        Ok(PairTicket {
            id,
            link,
            expires_in_seconds: 120,
        })
    }
    pub fn cancel(&mut self, id: &str) {
        self.sessions.remove(id);
    }
    pub fn link(&mut self, id: &str, now: u64) -> Result<String, PairError> {
        let session = self.sessions.get(id).ok_or(PairError::Missing)?;
        if session.pair.expired(now) {
            self.sessions.remove(id);
            return Err(PairError::Expired);
        }
        Ok(session.pair.link(session.group))
    }
    pub fn poll(
        &mut self,
        id: &str,
        now: u64,
        poll: impl FnOnce(
            &str,
            &mut Pairing,
            bool,
            u64,
        ) -> Result<Option<serde_json::Value>, NotificationError>,
    ) -> Result<Option<Recipient>, PairError> {
        let session = self.sessions.get_mut(id).ok_or(PairError::Missing)?;
        if session.pair.expired(now) {
            self.sessions.remove(id);
            return Err(PairError::Expired);
        }
        let found = poll(&session.token, &mut session.pair, session.group, now)?;
        let result = found
            .map(|value| serde_json::from_value::<Recipient>(value).map_err(|_| PairError::Invalid))
            .transpose()?;
        if let Some(recipient) = &result {
            if recipient.id.len() > 128
                || recipient.id.is_empty()
                || !(recipient.id.strip_prefix('@').is_some_and(|s| {
                    s.len() >= 5 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                }) || (recipient.id != "-"
                    && recipient
                        .id
                        .strip_prefix('-')
                        .unwrap_or(&recipient.id)
                        .bytes()
                        .all(|b| b.is_ascii_digit())))
                || !["private", "group", "supergroup", "channel"]
                    .contains(&recipient.r#type.as_str())
                || recipient.label.chars().count() > 512
                || recipient.username.len() > 256
            {
                return Err(PairError::Invalid);
            }
            self.sessions.remove(id);
        }
        Ok(result)
    }
}
