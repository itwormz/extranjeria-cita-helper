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
/// Raw protocol values may contain credentials; deliberately no Debug/serialization support.
pub struct BidiConnection {
    socket: WebSocket<TcpStream>,
    next: u64,
    pending: Option<(Ticket, Instant)>,
    timeout: Duration,
    failed: bool,
    events: Vec<Value>,
    event_bytes: usize,
    native_select_commit:bool,
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
            socket,
            next: 0,
            pending: None,
            timeout,
            failed: false,
            events: Vec::new(),
            event_bytes: 0,
            native_select_commit:false,
        })
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
        let ticket = Ticket(id);
        self.pending = Some((ticket, Instant::now() + self.timeout));
        if self.socket.send(Message::Text(packet.into())).is_err() {
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
        let result = self.receive_until(ticket, deadline);
        self.pending = None;
        if result.is_err() && result != Err(ProtocolError::Rejected) {
            self.failed = true;
        }
        result
    }
    fn receive_until(&mut self, ticket: Ticket, deadline: Instant) -> Result<Value, ProtocolError> {
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .filter(|d| !d.is_zero())
                .ok_or(ProtocolError::Unavailable)?;
            self.socket
                .get_mut()
                .set_read_timeout(Some(remaining))
                .map_err(|_| ProtocolError::Unavailable)?;
            let message = self.socket.read().map_err(|_| ProtocolError::Unavailable)?;
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
    pub fn take_events(&mut self) -> Vec<Value> {
        self.event_bytes = 0;
        std::mem::take(&mut self.events)
    }
}
