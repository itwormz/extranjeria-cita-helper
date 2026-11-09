//! Bounded metadata from the owned protocol stream. No headers, cookies, bodies
//! or query strings retained. Collection is opt-in and does not initiate I/O.
use crate::bidi::ProtocolError;
use serde_json::Value;
use std::collections::{HashSet, VecDeque};
const MAX_PENDING: usize = 8;
const MAX_RECENT: usize = 256;
pub const MAX_RESPONSE_BYTES: usize = 128 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseKind {
    Document,
    Script,
}
/// No Debug/Serialize: IDs and even sanitized paths remain internal evidence.
pub struct ResponseCandidate {
    request: String,
    url: String,
    kind: ResponseKind,
    status: u16,
    mime: String,
    context: String,
    navigation: Option<String>,
    timestamp_millis: Option<u64>,
}
impl ResponseCandidate {
    pub fn request(&self) -> &str {
        &self.request
    }
    pub fn url(&self) -> &str {
        &self.url
    }
    pub fn context(&self) -> &str {
        &self.context
    }
    pub fn navigation(&self) -> Option<&str> {
        self.navigation.as_deref()
    }
    pub fn timestamp_millis(&self) -> Option<u64> {
        self.timestamp_millis
    }
    pub fn status(&self) -> u16 {
        self.status
    }
    pub fn mime(&self) -> &str {
        &self.mime
    }
    pub fn kind(&self) -> ResponseKind {
        self.kind
    }
}
#[derive(Default)]
pub struct ResponseBatch {
    pub responses: Vec<ResponseCandidate>,
    pub overflowed: bool,
}
pub struct ResponseQueue {
    context: String,
    pending: Vec<ResponseCandidate>,
    recent: VecDeque<String>,
    seen: HashSet<String>,
    overflowed: bool,
}
fn identifier(value: &str) -> bool {
    !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
}
impl ResponseQueue {
    pub fn new(context: &str) -> Result<Self, ProtocolError> {
        if !identifier(context) {
            return Err(ProtocolError::InvalidData);
        }
        Ok(Self {
            context: context.into(),
            pending: Vec::new(),
            recent: VecDeque::new(),
            seen: HashSet::new(),
            overflowed: false,
        })
    }
    /// Only events received from the same owned BiDi connection belong here.
    pub fn ingest(&mut self, event: &Value) {
        if event["method"] != "network.responseCompleted"
            || event["params"]["context"] != self.context
        {
            return;
        }
        let p = &event["params"];
        let Some(id) = p["request"]["request"].as_str().filter(|v| identifier(v)) else {
            return;
        };
        if self.seen.contains(id) {
            return;
        }
        let Some(raw) = p["response"]["url"].as_str().filter(|v| v.len() <= 8192) else {
            return;
        };
        let Ok(url) = url::Url::parse(raw) else {
            return;
        };
        if url.scheme() != "https"
            || url.host_str() != Some("icp.administracionelectronica.gob.es")
            || url.port_or_known_default() != Some(443)
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return;
        }
        let path = url
            .path()
            .split('/')
            .map(|part| part.split(';').next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("/");
        if !path.starts_with("/icpplus/") && !path.starts_with("/icpplustieb/") {
            return;
        }
        let Some(status) = p["response"]["status"]
            .as_u64()
            .filter(|s| (200..600).contains(s) && !(300..400).contains(s))
        else {
            return;
        };
        let Some(mime) = p["response"]["mimeType"]
            .as_str()
            .filter(|v| v.len() <= 256)
        else {
            return;
        };
        let mime = mime
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        let kind = if matches!(
            mime.as_str(),
            "application/javascript"
                | "text/javascript"
                | "application/ecmascript"
                | "text/ecmascript"
                | "application/x-javascript"
        ) {
            ResponseKind::Script
        } else if matches!(mime.as_str(), "text/html" | "application/xhtml+xml")
            && p["navigation"].as_str().is_some_and(identifier)
        {
            ResponseKind::Document
        } else {
            return;
        };
        self.seen.insert(id.into());
        self.recent.push_back(id.into());
        if self.recent.len() > MAX_RECENT
            && let Some(old) = self.recent.pop_front()
        {
            self.seen.remove(&old);
        }
        if self.pending.len() >= MAX_PENDING {
            self.overflowed = true;
            return;
        }
        self.pending.push(ResponseCandidate {
            context: self.context.clone(),
            navigation: p["navigation"]
                .as_str()
                .filter(|v| identifier(v))
                .map(str::to_owned),
            timestamp_millis: p["timestamp"]
                .as_f64()
                .filter(|v| v.is_finite() && *v >= 0.0 && *v <= 9_007_199_254_740_991.0)
                .map(|v| v.floor() as u64),
            request: id.into(),
            url: format!("https://icp.administracionelectronica.gob.es{path}"),
            kind,
            status: status as u16,
            mime,
        });
    }
    /// Overflow tells the collector owner to release/reset retained bodies rather
    /// than growing a backlog. This queue does not bound the browser collector itself.
    pub fn drain(&mut self) -> ResponseBatch {
        ResponseBatch {
            responses: std::mem::take(&mut self.pending),
            overflowed: std::mem::take(&mut self.overflowed),
        }
    }
}

/// Transient response bytes may contain private page data. No Debug/Serialize.
pub struct ResponseBody(Vec<u8>);
impl ResponseBody {
    pub fn decode(value: &Value) -> Result<Self, ProtocolError> {
        use base64::Engine;
        let map = value
            .as_object()
            .filter(|m| m.len() == 2)
            .ok_or(ProtocolError::InvalidData)?;
        let raw = map
            .get("value")
            .and_then(Value::as_str)
            .ok_or(ProtocolError::InvalidData)?;
        let bytes = match map.get("type").and_then(Value::as_str) {
            Some("string") if raw.len() <= MAX_RESPONSE_BYTES => raw.as_bytes().to_vec(),
            Some("base64") if raw.len() <= MAX_RESPONSE_BYTES.div_ceil(3) * 4 => {
                base64::engine::general_purpose::STANDARD
                    .decode(raw)
                    .map_err(|_| ProtocolError::InvalidData)?
            }
            _ => return Err(ProtocolError::InvalidData),
        };
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(ProtocolError::InvalidData);
        }
        Ok(Self(bytes))
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}
/// Retrieve an already observed response once; never fetch/reload/inspect a page.
/// Native collector must have the same128KiB cap. The controller owns its
/// bounded lifetime and must remove/reset it on overflow, disable and shutdown.
/// disown=true releases this retained body even if decoding subsequently fails.
pub fn read_response(
    connection: &mut crate::bidi::BidiConnection,
    collector: &str,
    candidate: &ResponseCandidate,
) -> Result<ResponseBody, ProtocolError> {
    if !identifier(collector) {
        return Err(ProtocolError::InvalidData);
    }
    let result = match connection.call(
        "network.getData",
        serde_json::json!({
            "request":candidate.request(),"dataType":"response","collector":collector,"disown":true
        }),
    ) {
        Ok(result) => result,
        Err(ProtocolError::Rejected) => {
            // Unavailable data is rejected before getData's disown step. Release
            // ownership separately, without fetching/retrying the response body.
            connection.call(
                "network.disownData",
                serde_json::json!({
                    "request":candidate.request(),"dataType":"response","collector":collector
                }),
            )?;
            return Err(ProtocolError::Rejected);
        }
        Err(error) => return Err(error), // A poisoned connection is never reused.
    };
    ResponseBody::decode(&result["bytes"])
}
