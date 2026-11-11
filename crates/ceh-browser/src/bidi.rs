//! Bounded blocking BiDi transport. Run on the owned browser worker, never the UI thread.
//! No command retries, reconnects, message logging, or remote endpoint discovery.
use serde_json::{Value, json};
use std::{
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket, protocol::WebSocketConfig};

const MAX_MESSAGE: usize = 1024 * 1024;
const MAX_EVENTS: usize = 256;
const MAX_EVENT_BYTES: usize = 2 * MAX_MESSAGE;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolError {
    InvalidEndpoint,
    InvalidData,
    PendingCommand,
    Unavailable,
    Rejected,
    EventOverflow,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ticket(u64);
const MAX_COMMAND_LOG: usize = 5000;

/// A privacy-safe summary of a command for the diagnostics timeline: counts, durations and the hash of our
/// own script text. Typed values, key values, node ids, script arguments and URL queries never appear.
pub fn sanitize_command(method: &str, params: &Value) -> Value {
    match method {
        "input.performActions" => {
            let mut counts = serde_json::Map::new();
            let mut add = |key: &str, amount: u64| {
                let entry = counts.entry(key.to_owned()).or_insert(json!(0));
                *entry = json!(entry.as_u64().unwrap_or(0) + amount);
            };
            for source in params["actions"].as_array().into_iter().flatten() {
                for action in source["actions"].as_array().into_iter().flatten() {
                    let duration = action["duration"].as_u64().unwrap_or(0);
                    match action["type"].as_str().unwrap_or("") {
                        "keyDown" => add("key_down", 1),
                        "keyUp" => add("key_up", 1),
                        "pointerMove" => {
                            add("pointer_move", 1);
                            add("move_ms", duration);
                        }
                        "pointerDown" => add("pointer_down", 1),
                        "pointerUp" => add("pointer_up", 1),
                        "pause" => add("pause_ms", duration),
                        _ => {}
                    }
                }
            }
            Value::Object(counts)
        }
        "script.callFunction" | "script.evaluate" => {
            let text = params["functionDeclaration"].as_str().or_else(|| params["expression"].as_str()).unwrap_or("");
            // FNV-1a: stable between runs, so the same script of ours reads the same in every session.
            let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
            json!({"script_hash": format!("{hash:016x}"), "length": text.len()})
        }
        "browsingContext.navigate" => {
            // Scheme, host, port and path only: no credentials, query, fragment or `;jsessionid` path parameter.
            let clean = params["url"]
                .as_str()
                .and_then(|url| url::Url::parse(url).ok())
                .and_then(|parsed| {
                    let path: Vec<&str> = parsed.path().split('/').map(|segment| segment.split(';').next().unwrap_or("")).collect();
                    let port = parsed.port().map(|p| format!(":{p}")).unwrap_or_default();
                    Some(format!("{}://{}{}{}", parsed.scheme(), parsed.host_str()?, port, path.join("/")))
                })
                .unwrap_or_else(|| "invalid".to_owned());
            json!({"url": clean, "wait": params["wait"].as_str().unwrap_or("")})
        }
        _ => json!({}),
    }
}
fn epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
/// Raw protocol values may contain credentials; deliberately no Debug/serialization support.
pub struct BidiConnection {
    socket: Option<WebSocket<TcpStream>>,
    /// Chrome: the same commands answered over CDP (see `cdp.rs`); `socket` is then unused.
    cdp: Option<crate::cdp::CdpWire>,
    deferred: Option<(String, Value)>,
    next: u64,
    pending: Option<(Ticket, Instant)>,
    timeout: Duration,
    failed: bool,
    events: Vec<Value>,
    event_bytes: usize,
    native_select_commit:bool,
    /// Sanitized rows of the commands sent, drained by the diagnostics timeline.
    command_log: Vec<Value>,
    in_flight: Option<(Instant, i64, String, Value)>,
}
impl BidiConnection {
    pub fn connect(address: SocketAddr, timeout: Duration) -> Result<Self, ProtocolError> {
        if !address.ip().is_loopback() || address.port() == 0 {
            return Err(ProtocolError::InvalidEndpoint);
        }
        if timeout.is_zero() || timeout > Duration::from_secs(30) {
            return Err(ProtocolError::InvalidData);
        }
        let stream = TcpStream::connect_timeout(&address, timeout)
            .map_err(|_| ProtocolError::Unavailable)?;
        stream
            .set_read_timeout(Some(timeout))
            .map_err(|_| ProtocolError::Unavailable)?;
        stream
            .set_write_timeout(Some(timeout.min(Duration::from_millis(100))))
            .map_err(|_| ProtocolError::Unavailable)?;
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_MESSAGE))
            .max_frame_size(Some(MAX_MESSAGE));
        let (socket, _) = tungstenite::client::client_with_config(
            format!("ws://{address}/session"),
            stream,
            Some(config),
        )
        .map_err(|_| ProtocolError::Unavailable)?;
        Ok(Self {
            socket: Some(socket),
            cdp: None,
            deferred: None,
            next: 0,
            pending: None,
            timeout,
            failed: false,
            events: Vec::new(),
            event_bytes: 0,
            native_select_commit:false,
            command_log: Vec::new(),
            in_flight: None,
        })
    }
    /// Chrome/Brave: connect to the browser websocket named by `DevToolsActivePort` and attach to its page.
    pub fn connect_cdp(address: SocketAddr, path: &str, timeout: Duration) -> Result<Self, ProtocolError> {
        if !address.ip().is_loopback() || address.port() == 0 || !path.starts_with("/devtools/browser/") {
            return Err(ProtocolError::InvalidEndpoint);
        }
        if timeout.is_zero() || timeout > Duration::from_secs(30) {
            return Err(ProtocolError::InvalidData);
        }
        let stream = TcpStream::connect_timeout(&address, timeout).map_err(|_| ProtocolError::Unavailable)?;
        stream.set_read_timeout(Some(timeout)).map_err(|_| ProtocolError::Unavailable)?;
        stream.set_write_timeout(Some(timeout)).map_err(|_| ProtocolError::Unavailable)?;
        let config = WebSocketConfig::default().max_message_size(Some(16 * MAX_MESSAGE)).max_frame_size(Some(16 * MAX_MESSAGE));
        let (socket, _) = tungstenite::client::client_with_config(format!("ws://{address}{path}"), stream, Some(config))
            .map_err(|_| ProtocolError::Unavailable)?;
        let wire = crate::cdp::CdpWire::attach(socket, Instant::now() + timeout.max(Duration::from_secs(10)))?;
        Ok(Self {
            socket: None,
            cdp: Some(wire),
            deferred: None,
            next: 0,
            pending: None,
            timeout,
            failed: false,
            events: Vec::new(),
            event_bytes: 0,
            native_select_commit: false,
            command_log: Vec::new(),
            in_flight: None,
        })
    }
    /// Chrome: ask the browser to quit cleanly (it flushes its cookie store); a no-op for other connections.
    pub fn close_cdp_browser(&mut self) {
        if let Some(wire) = self.cdp.as_mut() {
            wire.close_browser();
        }
    }
    /// The browser target the CDP wire attached to (its id is the page context), when this is a Chrome connection.
    pub fn cdp_target(&self) -> Option<&str> {
        self.cdp.as_ref().map(crate::cdp::CdpWire::target)
    }
    /// A write error is ambiguous: the connection is poisoned, never automatically retried.
    /// Only the write happens here; callers may apply control before receive completes.
    pub fn send(&mut self, method: &str, params: Value) -> Result<Ticket, ProtocolError> {
        if self.failed {
            return Err(ProtocolError::Unavailable);
        }
        if self.pending.is_some() {
            return Err(ProtocolError::PendingCommand);
        }
        if method.is_empty() || method.len() > 128 || !params.is_object() {
            return Err(ProtocolError::InvalidData);
        }
        let id = self.next.checked_add(1).ok_or(ProtocolError::InvalidData)?;
        let packet = json!({"id":id,"method":method,"params":params}).to_string();
        if packet.len() > MAX_MESSAGE {
            return Err(ProtocolError::InvalidData);
        }
        self.next = id;
        self.in_flight = Some((Instant::now(), epoch_ms(), method.to_owned(), sanitize_command(method, &params)));
        let ticket = Ticket(id);
        self.pending = Some((ticket, Instant::now() + self.timeout));
        if self.cdp.is_some() {
            // Answered in `receive`, where the CDP round trips happen.
            self.deferred = Some((method.to_owned(), params));
            return Ok(ticket);
        }
        let Some(socket) = self.socket.as_mut() else { return Err(ProtocolError::Unavailable) };
        if socket.send(Message::Text(packet.into())).is_err() {
            self.failed = true;
            return Err(ProtocolError::Unavailable);
        }
        Ok(ticket)
    }
    pub fn receive(&mut self, ticket: Ticket) -> Result<Value, ProtocolError> {
        if self.failed {
            return Err(ProtocolError::Unavailable);
        }
        let Some((pending, deadline)) = self.pending else {
            return Err(ProtocolError::InvalidData);
        };
        if ticket != pending {
            return Err(ProtocolError::InvalidData);
        }
        let result = if self.cdp.is_some() { self.receive_cdp(deadline) } else { self.receive_until(ticket, deadline) };
        self.pending = None;
        if let Some((started, at, method, summary)) = self.in_flight.take() {
            if self.command_log.len() >= MAX_COMMAND_LOG {
                self.command_log.remove(0);
            }
            self.command_log.push(json!({
                "t_ms": at, "method": method, "summary": summary, "ok": result.is_ok(),
                "duration_ms": started.elapsed().as_millis() as u64,
            }));
        }
        if result.is_err() && result != Err(ProtocolError::Rejected) {
            self.failed = true;
        }
        result
    }
    fn receive_cdp(&mut self, deadline: Instant) -> Result<Value, ProtocolError> {
        let (method, params) = self.deferred.take().ok_or(ProtocolError::InvalidData)?;
        let wire = self.cdp.as_mut().ok_or(ProtocolError::Unavailable)?;
        let result = wire.handle(&method, &params, deadline);
        for event in wire.take_events() {
            // The CDP wire already keeps the newest events within its own cap.
            if self.events.len() >= 4096 {
                self.events.remove(0);
            }
            self.event_bytes += event.to_string().len();
            self.events.push(event);
        }
        result
    }
    fn receive_until(&mut self, ticket: Ticket, deadline: Instant) -> Result<Value, ProtocolError> {
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .filter(|d| !d.is_zero())
                .ok_or(ProtocolError::Unavailable)?;
            let socket = self.socket.as_mut().ok_or(ProtocolError::Unavailable)?;
            socket
                .get_mut()
                .set_read_timeout(Some(remaining))
                .map_err(|_| ProtocolError::Unavailable)?;
            let message = socket.read().map_err(|_| ProtocolError::Unavailable)?;
            let text = match message {
                Message::Text(text) => text,
                Message::Ping(_) | Message::Pong(_) => continue,
                _ => return Err(ProtocolError::InvalidData),
            };
            let value: Value =
                serde_json::from_str(&text).map_err(|_| ProtocolError::InvalidData)?;
            if value.get("id").is_some() {
                if value["id"].as_u64() != Some(ticket.0) {
                    return Err(ProtocolError::InvalidData);
                }
                if value["type"] == "error" {
                    return Err(ProtocolError::Rejected);
                }
                if value["type"] != "success" || value.get("result").is_none() {
                    return Err(ProtocolError::InvalidData);
                }
                return Ok(value["result"].clone());
            }
            if value["type"] != "event" || !value["method"].is_string() {
                return Err(ProtocolError::InvalidData);
            }
            if self.events.len() >= MAX_EVENTS || self.event_bytes + text.len() > MAX_EVENT_BYTES {
                return Err(ProtocolError::EventOverflow);
            }
            self.event_bytes += text.len();
            self.events.push(value);
        }
    }
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, ProtocolError> {
        let ticket = self.send(method, params)?;
        let result=self.receive(ticket)?;
        if method=="session.new"{self.native_select_commit=result["capabilities"]["browserName"]=="firefox";}
        Ok(result)
    }
    pub fn native_select_commit_supported(&self)->bool{self.native_select_commit}
    pub(crate) fn call_before(
        &mut self,
        method: &str,
        params: Value,
        deadline: Instant,
    ) -> Result<Value, ProtocolError> {
        if Instant::now() >= deadline {
            return Err(ProtocolError::Unavailable);
        }
        let ticket = self.send(method, params)?;
        if let Some((_, pending_deadline)) = &mut self.pending {
            *pending_deadline = (*pending_deadline).min(deadline);
        }
        let result=self.receive(ticket)?;
        if method=="session.new"{self.native_select_commit=result["capabilities"]["browserName"]=="firefox";}
        Ok(result)
    }
    /// A poisoned transport must never be retried or reused.
    pub fn available(&self) -> bool {
        !self.failed
    }
    /// Sanitized rows of the commands sent since the last call (see [`sanitize_command`]).
    pub fn take_command_log(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.command_log)
    }
    pub fn take_events(&mut self) -> Vec<Value> {
        self.event_bytes = 0;
        std::mem::take(&mut self.events)
    }
}
