//! Chrome DevTools Protocol wire that answers the BiDi commands the driver uses (variant A of the F5 plan).
//!
//! The driver (observer, trusted input, selection, ...) speaks BiDi: contexts, realms, `sharedId`, `script.*`
//! results and `network.*` / `browsingContext.*` events. This module translates those commands to CDP on one
//! flattened page session and turns CDP events into the BiDi events the driver reads, so the driver is shared
//! between Firefox and Chrome unchanged.
//!
//! Rules of the F5 plan kept here: no `Runtime.enable`, no `Debugger`, no `Fetch`; the domains in use are
//! `Page`, `Network` (observation), `Input` and `DOM` (frame owner lookup only). Scripts run in an isolated
//! world; input is `Input.dispatch*` (trusted).
use crate::bidi::ProtocolError;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    net::TcpStream,
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket};

const MAX_EVENTS: usize = 4096;
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const MAX_REQUESTS: usize = 4096;
const WORLD: &str = "ceh";

#[derive(Clone)]
struct Request {
    frame: String,
    navigation: Option<String>,
    url: String,
    method: String,
    kind: String,
    redirects: u64,
    status: Option<u64>,
    response: Option<Value>,
}

pub(crate) struct CdpWire {
    socket: WebSocket<TcpStream>,
    next: u64,
    session: String,
    target: String,
    events: Vec<Value>,
    worlds: HashMap<String, (i64, u64)>,
    serial: u64,
    urls: HashMap<String, String>,
    dom_enabled: bool,
    loaders: HashMap<String, String>,
    requests: HashMap<String, Request>,
    pointer: (f64, f64),
    pub(crate) closed: bool,
}

fn millis(seconds: Option<f64>) -> i64 {
    seconds.map_or_else(
        || {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as i64)
        },
        |s| (s * 1000.0) as i64,
    )
}
fn headers(raw: &Value) -> Value {
    Value::Array(
        raw.as_object()
            .into_iter()
            .flatten()
            .map(|(name, value)| json!({"name":name,"value":{"type":"string","value":value.as_str().unwrap_or("")}}))
            .collect(),
    )
}
/// BiDi `destination` / `initiatorType` of a CDP resource type (what the network journal reads).
pub(crate) fn request_kind(cdp_type: &str) -> (&'static str, &'static str) {
    match cdp_type {
        "Document" => ("document", ""),
        "Script" => ("script", ""),
        "Stylesheet" => ("style", ""),
        "Image" => ("image", ""),
        "Font" => ("font", ""),
        "XHR" => ("", "xmlhttprequest"),
        "Fetch" => ("", "fetch"),
        _ => ("other", ""),
    }
}
/// A CDP `Runtime` result as the BiDi remote value the driver checks (`string`, `number`, `boolean`, `node`).
pub(crate) fn remote_value(result: &Value) -> Value {
    let kind = result["type"].as_str().unwrap_or("undefined");
    match kind {
        "string" | "number" | "boolean" => json!({"type":kind,"value":result["value"]}),
        "undefined" => json!({"type":"undefined"}),
        "object" if result["subtype"] == "null" => json!({"type":"null"}),
        "object" if result["subtype"] == "node" => json!({"type":"node","sharedId":result["objectId"]}),
        "object" => json!({"type":"object"}),
        _ => json!({"type":kind}),
    }
}
/// A BiDi call argument as a CDP call argument.
pub(crate) fn call_argument(argument: &Value) -> Result<Value, ProtocolError> {
    if let Some(shared) = argument["sharedId"].as_str() {
        return Ok(json!({"objectId":shared}));
    }
    match argument["type"].as_str() {
        Some("string" | "number" | "boolean") => Ok(json!({"value":argument["value"]})),
        Some("undefined") => Ok(json!({})),
        Some("null") => Ok(json!({"value":null})),
        _ => Err(ProtocolError::InvalidData),
    }
}
/// WebDriver key code point → (`key`, `code`, Windows virtual key code, modifier bit).
fn special_key(value: &str) -> Option<(&'static str, &'static str, i64, u8)> {
    Some(match value {
        "\u{E003}" => ("Backspace", "Backspace", 8, 0),
        "\u{E004}" => ("Tab", "Tab", 9, 0),
        "\u{E006}" | "\u{E007}" => ("Enter", "Enter", 13, 0),
        "\u{E00C}" => ("Escape", "Escape", 27, 0),
        "\u{E00D}" => (" ", "Space", 32, 0),
        "\u{E012}" => ("ArrowLeft", "ArrowLeft", 37, 0),
        "\u{E013}" => ("ArrowUp", "ArrowUp", 38, 0),
        "\u{E014}" => ("ArrowRight", "ArrowRight", 39, 0),
        "\u{E015}" => ("ArrowDown", "ArrowDown", 40, 0),
        "\u{E017}" => ("Delete", "Delete", 46, 0),
        "\u{E00F}" => ("PageUp", "PageUp", 33, 0),
        "\u{E010}" => ("End", "End", 35, 0),
        "\u{E011}" => ("Home", "Home", 36, 0),
        "\u{E00A}" => ("Alt", "AltLeft", 18, 1),
        "\u{E009}" => ("Control", "ControlLeft", 17, 2),
        "\u{E03D}" => ("Meta", "MetaLeft", 91, 4),
        "\u{E008}" => ("Shift", "ShiftLeft", 16, 8),
        _ => return None,
    })
}
/// US-layout physical key of a punctuation character: (`code`, Windows virtual key, needs Shift).
fn punctuation(c: char) -> Option<(&'static str, i64, bool)> {
    Some(match c {
        ' ' => ("Space", 32, false),
        '-' => ("Minus", 189, false), '_' => ("Minus", 189, true),
        '=' => ("Equal", 187, false), '+' => ("Equal", 187, true),
        '[' => ("BracketLeft", 219, false), '{' => ("BracketLeft", 219, true),
        ']' => ("BracketRight", 221, false), '}' => ("BracketRight", 221, true),
        '\\' => ("Backslash", 220, false), '|' => ("Backslash", 220, true),
        ';' => ("Semicolon", 186, false), ':' => ("Semicolon", 186, true),
        '\'' => ("Quote", 222, false), '"' => ("Quote", 222, true),
        ',' => ("Comma", 188, false), '<' => ("Comma", 188, true),
        '.' => ("Period", 190, false), '>' => ("Period", 190, true),
        '/' => ("Slash", 191, false), '?' => ("Slash", 191, true),
        '`' => ("Backquote", 192, false), '~' => ("Backquote", 192, true),
        '!' => ("Digit1", 49, true), '@' => ("Digit2", 50, true), '#' => ("Digit3", 51, true),
        '$' => ("Digit4", 52, true), '%' => ("Digit5", 53, true), '^' => ("Digit6", 54, true),
        '&' => ("Digit7", 55, true), '*' => ("Digit8", 56, true), '(' => ("Digit9", 57, true), ')' => ("Digit0", 48, true),
        _ => return None,
    })
}
/// The CDP key events of one WebDriver key action, given the modifier bits held down so far.
pub(crate) fn key_event(kind: &str, value: &str, modifiers: u8) -> Option<Value> {
    let down = kind == "keyDown";
    if let Some((key, code, virtual_code, _)) = special_key(value) {
        return Some(json!({
            "type": if down { "rawKeyDown" } else { "keyUp" }, "key":key, "code":code,
            "windowsVirtualKeyCode":virtual_code, "nativeVirtualKeyCode":virtual_code, "modifiers":modifiers,
        }));
    }
    let mut chars = value.chars();
    let character = chars.next().filter(|_| chars.next().is_none())?;
    let upper = character.to_ascii_uppercase();
    let mut modifiers = modifiers;
    let (code, virtual_code) = if character.is_ascii_alphabetic() {
        if character.is_ascii_uppercase() {
            modifiers |= 8;
        }
        (format!("Key{upper}"), i64::from(upper as u8))
    } else if character.is_ascii_digit() {
        (format!("Digit{character}"), i64::from(character as u8))
    } else if let Some((code, virtual_code, shifted)) = punctuation(character) {
        if shifted {
            modifiers |= 8;
        }
        (code.to_owned(), virtual_code)
    } else {
        (String::new(), 0)
    };
    // With Control or Meta held a letter is a shortcut (select all, ...), not text.
    let shortcut = modifiers & 6 != 0;
    let mut event = json!({
        "type": if down { if shortcut { "rawKeyDown" } else { "keyDown" } } else { "keyUp" },
        "key":value, "code":code, "windowsVirtualKeyCode":virtual_code, "nativeVirtualKeyCode":virtual_code, "modifiers":modifiers,
    });
    if down && !shortcut {
        event["text"] = json!(value);
        event["unmodifiedText"] = json!(value);
    }
    if down && shortcut && character.eq_ignore_ascii_case(&'a') {
        event["commands"] = json!(["selectAll"]);
    }
    Some(event)
}

impl CdpWire {
    /// Attach to the one page of the browser: the first `page` target, flattened session.
    pub(crate) fn attach(mut socket: WebSocket<TcpStream>, deadline: Instant) -> Result<Self, ProtocolError> {
        let _ = socket.get_mut().set_nodelay(true);
        let mut wire = Self {
            socket,
            next: 0,
            session: String::new(),
            target: String::new(),
            events: Vec::new(),
            worlds: HashMap::new(),
            serial: 0,
            urls: HashMap::new(),
            dom_enabled: false,
            loaders: HashMap::new(),
            requests: HashMap::new(),
            pointer: (0.0, 0.0),
            closed: false,
        };
        let targets = wire.browser("Target.getTargets", json!({}), deadline)?;
        let target = targets["targetInfos"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|t| t["type"] == "page")
            .filter_map(|t| t["targetId"].as_str())
            .next()
            .ok_or(ProtocolError::Unavailable)?
            .to_owned();
        let attached = wire.browser("Target.attachToTarget", json!({"targetId":target,"flatten":true}), deadline)?;
        wire.session = attached["sessionId"].as_str().ok_or(ProtocolError::InvalidData)?.to_owned();
        wire.target = target;
        Ok(wire)
    }
    /// Ask the browser to quit cleanly so it flushes cookies and leaves no "restore pages" prompt behind.
    pub(crate) fn close_browser(&mut self) {
        let _ = self.browser("Browser.close", json!({}), Instant::now() + Duration::from_secs(2));
    }
    pub(crate) fn target(&self) -> &str {
        &self.target
    }
    pub(crate) fn take_events(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.events)
    }
    fn push(&mut self, event: Value) {
        // The oldest go first: the driver drains these at every observation, and the newest facts matter most.
        if self.events.len() >= MAX_EVENTS {
            self.events.remove(0);
        }
        self.events.push(event);
    }

    // ---- transport -------------------------------------------------------------------------------------
    fn browser(&mut self, method: &str, params: Value, deadline: Instant) -> Result<Value, ProtocolError> {
        self.request(method, params, false, deadline)
    }
    fn page(&mut self, method: &str, params: Value, deadline: Instant) -> Result<Value, ProtocolError> {
        self.request(method, params, true, deadline)
    }
    fn request(&mut self, method: &str, params: Value, session: bool, deadline: Instant) -> Result<Value, ProtocolError> {
        self.next += 1;
        let id = self.next;
        let mut packet = json!({"id":id,"method":method,"params":params});
        if session {
            packet["sessionId"] = json!(self.session);
        }
        let text = packet.to_string();
        if text.len() > MAX_MESSAGE_BYTES {
            return Err(ProtocolError::InvalidData);
        }
        self.socket.send(Message::Text(text.into())).map_err(|_| ProtocolError::Unavailable)?;
        loop {
            let message = self.read(deadline)?;
            if message["id"].as_u64() == Some(id) {
                return if message.get("error").is_some() { Err(ProtocolError::Rejected) } else { Ok(message["result"].clone()) };
            }
            if message.get("id").is_none() {
                self.on_event(&message);
            }
        }
    }
    fn read(&mut self, deadline: Instant) -> Result<Value, ProtocolError> {
        loop {
            let remaining = deadline.checked_duration_since(Instant::now()).filter(|d| !d.is_zero()).ok_or(ProtocolError::Unavailable)?;
            self.socket.get_mut().set_read_timeout(Some(remaining)).map_err(|_| ProtocolError::Unavailable)?;
            match self.socket.read().map_err(|_| ProtocolError::Unavailable)? {
                Message::Text(text) => return serde_json::from_str(&text).map_err(|_| ProtocolError::InvalidData),
                Message::Ping(_) | Message::Pong(_) => continue,
                _ => return Err(ProtocolError::InvalidData),
            }
        }
    }
    /// Read events for up to `wait` (the page may load, a click may navigate) without sending anything.
    fn pump(&mut self, wait: Duration) {
        let deadline = Instant::now() + wait;
        while let Ok(message) = self.read(deadline) {
            if message.get("id").is_none() {
                self.on_event(&message);
            }
        }
    }

    // ---- events ------------------------------------------------------------------------------------------
    pub(crate) fn on_event(&mut self, message: &Value) {
        let params = &message["params"];
        match message["method"].as_str().unwrap_or("") {
            "Target.targetDestroyed" | "Target.detachedFromTarget" => {
                let gone = params["targetId"].as_str() == Some(self.target.as_str()) || params["sessionId"].as_str() == Some(self.session.as_str());
                if gone {
                    self.closed = true;
                }
            }
            "Page.frameNavigated" => {
                if let (Some(frame), Some(loader)) = (params["frame"]["id"].as_str(), params["frame"]["loaderId"].as_str()) {
                    self.loaders.insert(frame.to_owned(), loader.to_owned());
                    self.urls.insert(frame.to_owned(), params["frame"]["url"].as_str().unwrap_or("").to_owned());
                    self.worlds.remove(frame);
                }
            }
            "Page.frameDetached" => {
                if let Some(frame) = params["frameId"].as_str() {
                    self.worlds.remove(frame);
                    self.loaders.remove(frame);
                    self.push(json!({"type":"event","method":"browsingContext.contextDestroyed","params":{"context":frame,"url":""}}));
                }
            }
            "Page.lifecycleEvent" => {
                let name = params["name"].as_str().unwrap_or("");
                let method = match name {
                    "load" => "browsingContext.load",
                    "DOMContentLoaded" => "browsingContext.domContentLoaded",
                    _ => return,
                };
                if let (Some(frame), Some(loader)) = (params["frameId"].as_str(), params["loaderId"].as_str()) {
                    let url = self.urls.get(frame).cloned().unwrap_or_default();
                    self.push(json!({"type":"event","method":method,"params":{
                        "context":frame,"navigation":loader,"url":url,"timestamp":millis(None)}}));
                }
            }
            "Page.javascriptDialogOpening" => {
                // An open dialog blocks every script call; Firefox BiDi dismisses it and notifies.
                self.push(json!({"type":"event","method":"browsingContext.userPromptOpened","params":{
                    "context":self.target,"type":params["type"],"message":""}}));
                self.next += 1;
                let packet = json!({"id":self.next,"method":"Page.handleJavaScriptDialog","params":{"accept":false},"sessionId":self.session});
                let _ = self.socket.send(Message::Text(packet.to_string().into()));
            }
            "Network.requestWillBeSent" => self.request_will_be_sent(params),
            "Network.responseReceived" => {
                if let Some(request) = params["requestId"].as_str().and_then(|id| self.requests.get_mut(id)) {
                    request.status = params["response"]["status"].as_u64();
                    request.response = Some(params["response"].clone());
                }
            }
            "Network.loadingFinished" => {
                if let Some(id) = params["requestId"].as_str() {
                    self.finish(id, params["timestamp"].as_f64().map(|_| millis(None)).unwrap_or_else(|| millis(None)));
                }
            }
            "Network.loadingFailed" => {
                if let Some(request) = params["requestId"].as_str().and_then(|id| self.requests.remove(id)) {
                    let (destination, initiator) = request_kind(&request.kind);
                    self.push(json!({"type":"event","method":"network.fetchError","params":{
                        "context":request.frame,"navigation":request.navigation,"redirectCount":request.redirects,
                        "request":{"request":params["requestId"],"url":request.url,"method":request.method,"destination":destination,"initiatorType":initiator,"headers":[],"cookies":[]},
                        "errorText":params["errorText"],"timestamp":millis(None)}}));
                }
            }
            _ => {}
        }
    }
    fn request_will_be_sent(&mut self, params: &Value) {
        let (Some(id), Some(frame)) = (params["requestId"].as_str(), params["frameId"].as_str()) else { return };
        let kind = params["type"].as_str().unwrap_or("Other").to_owned();
        let url = params["request"]["url"].as_str().unwrap_or("").to_owned();
        let method = params["request"]["method"].as_str().unwrap_or("GET").to_owned();
        let document = kind == "Document";
        let mut redirects = 0;
        if params.get("redirectResponse").is_some_and(|v| !v.is_null()) {
            // The previous hop of this request ended in a redirect: report it as completed first.
            if let Some(previous) = self.requests.get_mut(id) {
                previous.status = params["redirectResponse"]["status"].as_u64();
                previous.response = Some(params["redirectResponse"].clone());
                redirects = previous.redirects + 1;
            }
            self.finish(id, millis(None));
        }
        let navigation = document.then(|| params["loaderId"].as_str().unwrap_or(id).to_owned());
        let timestamp = millis(params["wallTime"].as_f64());
        if document && redirects == 0 {
            self.push(json!({"type":"event","method":"browsingContext.navigationStarted","params":{
                "context":frame,"navigation":navigation,"url":url,"timestamp":timestamp}}));
        }
        if self.requests.len() >= MAX_REQUESTS {
            // Only requests that never finished remain; forget the oldest-registered half is not tracked, so a
            // pathological page starts over rather than growing without bound.
            self.requests.clear();
        }
        let (destination, initiator) = request_kind(&kind);
        self.push(json!({"type":"event","method":"network.beforeRequestSent","params":{
            "context":frame,"navigation":navigation,"redirectCount":redirects,"timestamp":timestamp,
            "request":{"request":id,"url":url,"method":method,"destination":destination,"initiatorType":initiator,
                       "headers":headers(&params["request"]["headers"]),"cookies":[]}}}));
        self.requests.insert(id.to_owned(), Request { frame: frame.to_owned(), navigation, url, method, kind, redirects, status: None, response: None });
    }
    /// A request that completed: reported once and forgotten (a redirect hop re-registers the request).
    fn finish(&mut self, id: &str, timestamp: i64) {
        let Some(request) = self.requests.remove(id) else { return };
        let Some(response) = request.response.clone() else { return };
        let (destination, initiator) = request_kind(&request.kind);
        self.push(json!({"type":"event","method":"network.responseCompleted","params":{
            "context":request.frame,"navigation":request.navigation,"redirectCount":request.redirects,"timestamp":timestamp,
            "request":{"request":id,"url":request.url,"method":request.method,"destination":destination,"initiatorType":initiator,"headers":[],"cookies":[]},
            "response":{"url":response["url"],"status":request.status.unwrap_or(0),"mimeType":response["mimeType"],"headers":headers(&response["headers"])}}}));
    }

    // ---- BiDi commands -----------------------------------------------------------------------------------
    pub(crate) fn handle(&mut self, method: &str, params: &Value, deadline: Instant) -> Result<Value, ProtocolError> {
        match method {
            "session.new" => Ok(json!({"sessionId":"cdp","capabilities":{"browserName":"chrome","acceptInsecureCerts":false}})),
            "session.status" => Ok(json!({"ready":true,"message":""})),
            "session.end" => Ok(json!({})),
            "session.subscribe" => {
                self.page("Page.enable", json!({}), deadline)?;
                self.page("Page.setLifecycleEventsEnabled", json!({"enabled":true}), deadline)?;
                self.page("Network.enable", json!({}), deadline)?;
                // A window opened behind others reads as hidden/unfocused to the site (owner's Camoufox
                // observation: early pages in the background got blocked more often).
                let _ = self.page("Page.bringToFront", json!({}), deadline);
                Ok(json!({"subscription":"cdp"}))
            }
            "browsingContext.getTree" => self.tree(deadline),
            "browsingContext.navigate" => self.navigate(params, deadline, false),
            "browsingContext.reload" => self.navigate(params, deadline, true),
            "browsingContext.create" => {
                let created = self.browser("Target.createTarget", json!({"url":"about:blank"}), deadline)?;
                Ok(json!({"context":created["targetId"]}))
            }
            "browsingContext.close" => {
                let context = params["context"].as_str().ok_or(ProtocolError::InvalidData)?;
                self.browser("Target.closeTarget", json!({"targetId":context}), deadline)?;
                Ok(json!({}))
            }
            "browsingContext.captureScreenshot" => {
                let shot = self.page("Page.captureScreenshot", json!({"format":"png"}), deadline)?;
                Ok(json!({"data":shot["data"]}))
            }
            "browsingContext.locateNodes" => self.locate(params, deadline),
            "script.evaluate" => self.script(params, deadline, true),
            "script.callFunction" => self.script(params, deadline, false),
            "script.addPreloadScript" => {
                let source = params["functionDeclaration"].as_str().ok_or(ProtocolError::InvalidData)?;
                let added = self.page("Page.addScriptToEvaluateOnNewDocument", json!({"source":format!("({source})()")}), deadline)?;
                Ok(json!({"script":added["identifier"]}))
            }
            "storage.getCookies" => {
                let all = self.page("Network.getAllCookies", json!({}), deadline)?;
                let domain = params["filter"]["domain"].as_str().unwrap_or("");
                let cookies: Vec<Value> = all["cookies"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|c| domain.is_empty() || c["domain"].as_str().is_some_and(|d| { let (d, wanted) = (d.trim_start_matches('.'), domain.trim_start_matches('.')); d == wanted || d.ends_with(&format!(".{wanted}")) }))
                    .map(|c| json!({"name":c["name"],"domain":c["domain"],"path":c["path"],"value":{"type":"string","value":""}}))
                    .collect();
                Ok(json!({"cookies":cookies}))
            }
            "input.performActions" => self.perform(params, deadline),
            "input.releaseActions" => Ok(json!({})),
            // Response collectors, intercepts, user contexts: not available on this wire.
            _ => Err(ProtocolError::Rejected),
        }
    }
    fn tree(&mut self, deadline: Instant) -> Result<Value, ProtocolError> {
        let targets = self.browser("Target.getTargets", json!({}), deadline)?;
        let listed = targets["targetInfos"].as_array().into_iter().flatten().any(|t| t["targetId"].as_str() == Some(self.target.as_str()));
        if !listed || self.closed {
            return Ok(json!({"contexts":[]}));
        }
        let frames = self.page("Page.getFrameTree", json!({}), deadline)?;
        fn walk(node: &Value) -> Value {
            let children: Vec<Value> = node["childFrames"].as_array().into_iter().flatten().map(walk).collect();
            json!({"context":node["frame"]["id"],"url":node["frame"]["url"],"children":children})
        }
        Ok(json!({"contexts":[walk(&frames["frameTree"])]}))
    }
    fn navigate(&mut self, params: &Value, deadline: Instant, reload: bool) -> Result<Value, ProtocolError> {
        let context = params["context"].as_str().ok_or(ProtocolError::InvalidData)?.to_owned();
        let wait = params["wait"].as_str().unwrap_or("none");
        let (navigation, url) = if reload {
            self.page("Page.reload", json!({}), deadline)?;
            (String::new(), String::new())
        } else {
            let url = params["url"].as_str().ok_or(ProtocolError::InvalidData)?;
            let result = self.page("Page.navigate", json!({"url":url,"frameId":context}), deadline)?;
            if result.get("errorText").is_some_and(|e| !e.is_null()) {
                return Err(ProtocolError::Rejected);
            }
            (result["loaderId"].as_str().unwrap_or("").to_owned(), url.to_owned())
        };
        let wanted = match wait {
            "complete" => Some("browsingContext.load"),
            "interactive" => Some("browsingContext.domContentLoaded"),
            _ => None,
        };
        if let Some(wanted) = wanted {
            let seen = |events: &Vec<Value>| events.iter().any(|e| e["method"] == wanted && e["params"]["context"] == context.as_str());
            while !seen(&self.events) {
                let message = self.read(deadline)?;
                if message.get("id").is_none() {
                    self.on_event(&message);
                }
            }
        }
        Ok(json!({"navigation":if navigation.is_empty() { Value::Null } else { json!(navigation) },"url":url}))
    }

    // ---- scripts ---------------------------------------------------------------------------------------
    /// The isolated world of a frame: its execution context id and a serial that is never reused, so a realm
    /// name stays tied to one document even when a new renderer process hands out the same context id again.
    fn world(&mut self, frame: &str, deadline: Instant) -> Result<(i64, u64), ProtocolError> {
        if let Some(world) = self.worlds.get(frame) {
            return Ok(*world);
        }
        let created = self.page("Page.createIsolatedWorld", json!({"frameId":frame,"worldName":WORLD}), deadline)?;
        let id = created["executionContextId"].as_i64().ok_or(ProtocolError::InvalidData)?;
        self.serial += 1;
        self.worlds.insert(frame.to_owned(), (id, self.serial));
        Ok((id, self.serial))
    }
    /// The execution context a BiDi `target` names: a realm (`r<serial>:<context>:<frame>`, bound to one
    /// document; stale once the frame moved on) or a context (the frame's current world).
    fn resolve(&mut self, target: &Value, deadline: Instant) -> Result<(i64, u64, String, bool), ProtocolError> {
        if let Some(realm) = target["realm"].as_str() {
            let mut parts = realm.strip_prefix('r').ok_or(ProtocolError::InvalidData)?.splitn(3, ':');
            let serial: u64 = parts.next().and_then(|n| n.parse().ok()).ok_or(ProtocolError::InvalidData)?;
            let id: i64 = parts.next().and_then(|n| n.parse().ok()).ok_or(ProtocolError::InvalidData)?;
            let frame = parts.next().filter(|f| !f.is_empty()).ok_or(ProtocolError::InvalidData)?.to_owned();
            // Take in what the page already reported (a navigation) before trusting the realm.
            self.pump(Duration::from_millis(2));
            return if self.worlds.get(&frame) == Some(&(id, serial)) { Ok((id, serial, frame, false)) } else { Err(ProtocolError::Rejected) };
        }
        let frame = target["context"].as_str().ok_or(ProtocolError::InvalidData)?.to_owned();
        let (id, serial) = self.world(&frame, deadline)?;
        Ok((id, serial, frame, true))
    }
    fn script(&mut self, params: &Value, deadline: Instant, evaluate: bool) -> Result<Value, ProtocolError> {
        let mut attempt = 0;
        loop {
            let (context, serial, frame, by_context) = self.resolve(&params["target"], deadline)?;
            let await_promise = params["awaitPromise"].as_bool().unwrap_or(false);
            let reply = if evaluate {
                let expression = params["expression"].as_str().ok_or(ProtocolError::InvalidData)?;
                self.page("Runtime.evaluate", json!({"expression":expression,"contextId":context,"awaitPromise":await_promise,"returnByValue":false}), deadline)
            } else {
                let declaration = params["functionDeclaration"].as_str().ok_or(ProtocolError::InvalidData)?;
                let arguments: Result<Vec<Value>, _> = params["arguments"].as_array().into_iter().flatten().map(call_argument).collect();
                self.page(
                    "Runtime.callFunctionOn",
                    json!({"functionDeclaration":declaration,"executionContextId":context,"arguments":arguments?,"awaitPromise":await_promise,"returnByValue":false}),
                    deadline,
                )
            };
            return match reply {
                Ok(reply) => {
                    let realm = format!("r{serial}:{context}:{frame}");
                    if reply.get("exceptionDetails").is_some_and(|e| !e.is_null()) {
                        Ok(json!({"type":"exception","realm":realm,"exceptionDetails":{"text":reply["exceptionDetails"]["text"]}}))
                    } else {
                        Ok(json!({"type":"success","realm":realm,"result":remote_value(&reply["result"])}))
                    }
                }
                // The document navigated since the world was made: a new world for a context target, a stale
                // realm (fail closed) for a realm target.
                Err(ProtocolError::Rejected) if by_context && attempt == 0 => {
                    attempt += 1;
                    self.worlds.remove(&frame);
                    continue;
                }
                Err(error) => Err(error),
            };
        }
    }
    fn enable_dom(&mut self, deadline: Instant) -> Result<(), ProtocolError> {
        if !self.dom_enabled {
            self.page("DOM.enable", json!({}), deadline)?;
            self.dom_enabled = true;
        }
        Ok(())
    }
    fn locate(&mut self, params: &Value, deadline: Instant) -> Result<Value, ProtocolError> {
        let parent = params["context"].as_str().ok_or(ProtocolError::InvalidData)?.to_owned();
        let child = params["locator"]["value"]["context"].as_str().ok_or(ProtocolError::InvalidData)?;
        let (world, _) = self.world(&parent, deadline)?;
        self.enable_dom(deadline)?;
        let owner = self.page("DOM.getFrameOwner", json!({"frameId":child}), deadline)?;
        let resolved = self.page("DOM.resolveNode", json!({"backendNodeId":owner["backendNodeId"],"executionContextId":world}), deadline)?;
        Ok(json!({"nodes":[{"type":"node","sharedId":resolved["object"]["objectId"]}]}))
    }

    // ---- input -----------------------------------------------------------------------------------------
    fn center(&mut self, shared: &str, deadline: Instant) -> Result<(f64, f64), ProtocolError> {
        // Quads are in the main frame's viewport, also for an element inside an iframe.
        if self.enable_dom(deadline).is_ok()
            && let Ok(quads) = self.page("DOM.getContentQuads", json!({"objectId":shared}), deadline)
            && let Some(quad) = quads["quads"][0].as_array().filter(|q| q.len() == 8)
        {
            let at = |i: usize| quad[i].as_f64().unwrap_or(0.0);
            return Ok(((at(0) + at(2) + at(4) + at(6)) / 4.0, (at(1) + at(3) + at(5) + at(7)) / 4.0));
        }
        let result = self.page(
            "Runtime.callFunctionOn",
            json!({"objectId":shared,"functionDeclaration":"function(){const r=this.getBoundingClientRect();return [r.left+r.width/2,r.top+r.height/2];}","returnByValue":true}),
            deadline,
        )?;
        let value = &result["result"]["value"];
        Ok((value[0].as_f64().ok_or(ProtocolError::InvalidData)?, value[1].as_f64().ok_or(ProtocolError::InvalidData)?))
    }
    fn perform(&mut self, params: &Value, deadline: Instant) -> Result<Value, ProtocolError> {
        let mut modifiers: u8 = 0;
        for source in params["actions"].as_array().into_iter().flatten() {
            for action in source["actions"].as_array().into_iter().flatten() {
                let duration = Duration::from_millis(action["duration"].as_u64().unwrap_or(0).min(10_000));
                match action["type"].as_str().unwrap_or("") {
                    "pause" => std::thread::sleep(duration),
                    "pointerMove" => {
                        let (dx, dy) = (action["x"].as_f64().unwrap_or(0.0), action["y"].as_f64().unwrap_or(0.0));
                        let base = match action["origin"]["element"]["sharedId"].as_str() {
                            Some(shared) => self.center(shared, deadline)?,
                            None if action["origin"] == "pointer" => self.pointer,
                            None => (0.0, 0.0),
                        };
                        std::thread::sleep(duration);
                        let (x, y) = (base.0 + dx, base.1 + dy);
                        self.page("Input.dispatchMouseEvent", json!({"type":"mouseMoved","x":x,"y":y,"modifiers":modifiers}), deadline)?;
                        self.pointer = (x, y);
                    }
                    "pointerDown" | "pointerUp" => {
                        let down = action["type"] == "pointerDown";
                        let (x, y) = self.pointer;
                        self.page(
                            "Input.dispatchMouseEvent",
                            json!({"type":if down { "mousePressed" } else { "mouseReleased" },"x":x,"y":y,"button":"left","buttons":if down { 1 } else { 0 },"clickCount":1,"modifiers":modifiers}),
                            deadline,
                        )?;
                    }
                    kind @ ("keyDown" | "keyUp") => {
                        let value = action["value"].as_str().ok_or(ProtocolError::InvalidData)?;
                        if let Some((_, _, _, bit)) = special_key(value) {
                            if kind == "keyDown" {
                                modifiers |= bit;
                            } else {
                                modifiers &= !bit;
                            }
                        }
                        let event = key_event(kind, value, modifiers).ok_or(ProtocolError::InvalidData)?;
                        self.page("Input.dispatchKeyEvent", event, deadline)?;
                    }
                    _ => return Err(ProtocolError::InvalidData),
                }
            }
        }
        // A click may navigate: take what the page reports in the next moments into the event queue.
        self.pump(Duration::from_millis(15));
        Ok(json!({}))
    }
}
