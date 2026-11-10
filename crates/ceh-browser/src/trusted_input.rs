//! Internal typed-action primitive. Never accepts UI/page-provided selectors or scripts.
use crate::{
    BrowserError, DispatchGate, DispatchOutcome,
    bidi::{BidiConnection, ProtocolError},
    dom::{FrameSource, INSPECT_DOCUMENT, read_document},
    observer::{ObservedPage, PageObserver},
};
use ceh_core::model::{ActionKind, DocumentGuard};
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};
const GUARD: &str = include_str!("mutation-guard.js");
/// Resolve an element in the guarded realm, reobserve, then publish one trusted
/// pointer sequence bound to that element reference. No coordinate or JS fallback.
/// Acknowledgement is not evidence of a page transition; the core observes next.
#[derive(Clone, Copy)]
enum TargetOrigin {
    Icp,
    Provider,
}
impl TargetOrigin {
    fn predicate(self) -> &'static str {
        match self {
            Self::Icp => "origin=>origin==='https://icp.administracionelectronica.gob.es'",
            Self::Provider => {
                "origin=>origin==='https://pasarela.clave.gob.es'||origin==='https://pasarela-ident.clave.gob.es'"
            }
        }
    }
}
pub(crate) struct Target<'a> {
    action: ActionKind,
    payload: Value,
    script: &'static str,
    origin: TargetOrigin,
    source: Option<FrameSource>,
    published: Option<&'a mut (dyn FnMut() + Send)>,
}
impl Target<'static> {
    pub fn recovery(payload: Value, source: FrameSource) -> Self {
        Self {
            action: ActionKind::Recovery,
            payload,
            script: include_str!("recovery-step.js"),
            origin: TargetOrigin::Icp,
            source: Some(source),
            published: None,
        }
    }
    pub fn icp(action: ActionKind, payload: Value, script: &'static str) -> Self {
        Self {
            action,
            payload,
            script,
            origin: TargetOrigin::Icp,
            source: None,
            published: None,
        }
    }
}
impl<'a> Target<'a> {
    pub fn provider(
        payload: Value,
        script: &'static str,
        published: Option<&'a mut (dyn FnMut() + Send)>,
    ) -> Self {
        Self {
            action: ActionKind::Auth,
            payload,
            script,
            origin: TargetOrigin::Provider,
            source: None,
            published,
        }
    }
}
pub(crate) fn click(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    fresh: ObservedPage,
    target: Target<'_>,
) -> Result<DispatchOutcome, BrowserError> {
    activate(connection, observer, gate, guard, fresh, target, true)
}
pub(crate) fn focus(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    fresh: ObservedPage,
    target: Target<'_>,
) -> Result<DispatchOutcome, BrowserError> {
    activate(connection, observer, gate, guard, fresh, target, false)
}
fn activate(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    fresh: ObservedPage,
    target: Target<'_>,
    submission: bool,
) -> Result<DispatchOutcome, BrowserError> {
    let Target {
        action,
        mut payload,
        script: target_script,
        origin,
        source,
        mut published,
    } = target;
    let mut permit = gate.begin_preparation(guard, &fresh.observation, action)?;
    let fill_mode = if payload["fillMode"] == "copy" {
        "copy"
    } else {
        "manual"
    };
    let scoped_document = if let Some(source) = &source {
        if !fresh
            .page
            .sources
            .iter()
            .any(|s| s.context == source.context && s.realm == source.realm)
        {
            return Err(BrowserError::StaleDocument);
        }
        let document = read_document(connection, &source.context, fill_mode)
            .map_err(|_| BrowserError::NoObservation)?;
        if document.realm != source.realm {
            return Err(BrowserError::StaleDocument);
        }
        Some(document)
    } else {
        None
    };
    let snapshot = scoped_document
        .as_ref()
        .map(|d| &d.snapshot)
        .unwrap_or(&fresh.page.snapshot);
    let frame = snapshot.frames.first().ok_or(BrowserError::NoObservation)?;
    let context = source
        .as_ref()
        .or_else(|| fresh.page.sources.first())
        .ok_or(BrowserError::NoObservation)?
        .context
        .clone();
    let realm = source
        .as_ref()
        .map(|s| &s.realm)
        .unwrap_or(&fresh.observation.document);
    let allowed = origin.predicate();
    let declaration = format!(
        "raw=>{{const inspect={INSPECT_DOCUMENT};const validate={GUARD};const operation={target_script};const allowed={allowed};const target=validate(raw,inspect,operation,allowed);if(target instanceof Element)target.scrollIntoView({{block:'center',inline:'nearest',behavior:'instant'}});return target;}}"
    );
    payload["expectedText"] = json!(frame.text);
    payload["expectedVisible"] = json!(snapshot.visible);
    payload["expectedEndpoint"] = json!(snapshot.endpoint);
    let mut ticket = None;
    let sent=permit.send(&fresh.observation,||{
  let Some(now)=SystemTime::now().duration_since(UNIX_EPOCH).ok().and_then(|d|u64::try_from(d.as_millis()).ok()) else{return DispatchOutcome::NotDispatched;};
  payload["issuedAt"]=json!(now);
  let params=json!({"target":{"realm":realm.as_str()},"awaitPromise":false,"functionDeclaration":declaration,"arguments":[{"type":"string","value":payload.to_string()}]});
  send(connection,"script.callFunction",params,&mut ticket)
 })?;
    let Some(ticket) = ticket else {
        if sent == DispatchOutcome::Unknown {
            permit.complete_step(sent)?;
        }
        return Ok(permit.finish());
    };
    let result = match connection.receive(ticket) {
        Ok(result) => result,
        Err(_) => {
            permit.complete_step(DispatchOutcome::Unknown)?;
            return Ok(permit.finish());
        }
    };
    if result["type"] != "success" {
        permit.complete_step(DispatchOutcome::Unknown)?;
        return Ok(permit.finish());
    }
    if result["result"]["type"] != "node" {
        permit.complete_step(DispatchOutcome::NotDispatched)?;
        return Ok(permit.finish());
    }
    // Resolving an actionable node also scrolls it into view in this automatic
    // phase. Preserve that acknowledged change even if the later click is refused.
    permit.complete_step(DispatchOutcome::Dispatched)?;
    let Some(shared) = result["result"]["sharedId"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 1024)
    else {
        return Ok(DispatchOutcome::NotDispatched);
    };
    let shared = shared.to_owned();
    permit.finish();
    let current = observer
        .observe(connection, fill_mode)
        .map_err(|_| BrowserError::NoObservation)?;
    if current.page.snapshot.endpoint != fresh.page.snapshot.endpoint
        || current.page.snapshot.visible != fresh.page.snapshot.visible
        || current.page.snapshot.frames.first().map(|f| &f.text)
            != fresh.page.snapshot.frames.first().map(|f| &f.text)
    {
        return Err(BrowserError::StaleDocument);
    }
    if let Some(source) = &source {
        if current.observation.document != guard.document
            || current.observation.kind != guard.expected_page
            || !current
                .page
                .sources
                .iter()
                .any(|s| s.context == source.context && s.realm == source.realm)
        {
            return Err(BrowserError::StaleDocument);
        }
        let document = read_document(connection, &source.context, fill_mode)
            .map_err(|_| BrowserError::NoObservation)?;
        if document.realm != source.realm
            || document.snapshot.endpoint != snapshot.endpoint
            || document.snapshot.visible != snapshot.visible
            || document.snapshot.frames.first().map(|f| &f.text)
                != snapshot.frames.first().map(|f| &f.text)
        {
            return Err(BrowserError::StaleDocument);
        }
    }
    let mut permit = if submission {
        gate.begin(guard, &current.observation, action)?
    } else {
        gate.begin_preparation(guard, &current.observation, action)?
    };
    let params = json!({"context":context,"actions":[{"type":"pointer","id":"ceh-pointer","parameters":{"pointerType":"mouse"},"actions":click_actions(&shared, pointer_seed())}]});
    let mut ticket = None;
    let sent = permit.send(&current.observation, || {
        send(connection, "input.performActions", params, &mut ticket)
    })?;
    // Notify outside the authority lock and before waiting for a possible native
    // certificate dialog. Hook is trusted, bounded controller bookkeeping only.
    if sent == DispatchOutcome::Unknown
        && let Some(hook) = published.as_mut()
    {
        hook();
    }
    let Some(ticket) = ticket else {
        if sent == DispatchOutcome::Unknown {
            permit.complete_step(sent)?;
        }
        return Ok(permit.finish());
    };
    let outcome = match connection.receive(ticket) {
        Ok(_) => DispatchOutcome::Dispatched,
        Err(_) => DispatchOutcome::Unknown,
    };
    permit.complete_step(outcome)?;
    Ok(permit.finish())
}
/// The cursor travels a curved path to the element centre, then presses and releases there.
pub(crate) fn click_actions(shared: &str, seed: u64) -> Vec<Value> {
    let origin = json!({"type":"element","element":{"sharedId":shared}});
    let mut actions: Vec<Value> = ceh_core::input::pointer_path(seed)
        .into_iter()
        .map(|step| json!({"type":"pointerMove","duration":step.ms,"x":step.x,"y":step.y,"origin":origin}))
        .collect();
    actions.push(json!({"type":"pointerDown","button":0}));
    actions.push(json!({"type":"pause","duration":ceh_core::input::POINTER_TIMING.press_ms}));
    actions.push(json!({"type":"pointerUp","button":0}));
    actions
}
fn pointer_seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)
}
fn send(
    connection: &mut BidiConnection,
    method: &str,
    params: Value,
    ticket: &mut Option<crate::bidi::Ticket>,
) -> DispatchOutcome {
    match connection.send(method, params) {
        Ok(value) => {
            *ticket = Some(value);
            DispatchOutcome::Unknown
        }
        Err(ProtocolError::InvalidData | ProtocolError::PendingCommand) => {
            DispatchOutcome::NotDispatched
        }
        Err(_) => DispatchOutcome::Unknown,
    }
}

pub(crate) fn key(
    connection: &mut BidiConnection,
    permit: &mut crate::ActionPermit,
    fresh: &ObservedPage,
    value: char,
) -> Result<DispatchOutcome, BrowserError> {
    perform_keys(
        connection,
        permit,
        fresh,
        vec![
            json!({"type":"keyDown","value":value.to_string()}),
            json!({"type":"keyUp","value":value.to_string()}),
        ],
    )
}
/// Key actions that type `text` one character at a time (a select's type-ahead needs them in one chain).
const BACKSPACE: &str = "\u{E003}";
const META: &str = "\u{E03D}";
const CONTROL: &str = "\u{E009}";
/// Key actions that select the whole field and delete it: the platform modifier held around `a`, then Backspace.
pub(crate) fn clear_actions(mac: bool) -> Vec<Value> {
    let modifier = if mac { META } else { CONTROL };
    vec![
        json!({"type":"keyDown","value":modifier}),
        json!({"type":"keyDown","value":"a"}),
        json!({"type":"keyUp","value":"a"}),
        json!({"type":"keyUp","value":modifier}),
        json!({"type":"keyDown","value":BACKSPACE}),
        json!({"type":"keyUp","value":BACKSPACE}),
    ]
}
/// Select-all and delete in the focused field with the browser's own keyboard input.
pub(crate) fn clear_field(
    connection: &mut BidiConnection,
    permit: &mut crate::ActionPermit,
    fresh: &ObservedPage,
) -> Result<DispatchOutcome, BrowserError> {
    perform_keys(connection, permit, fresh, clear_actions(cfg!(target_os = "macos")))
}
fn perform_keys(
    connection: &mut BidiConnection,
    permit: &mut crate::ActionPermit,
    fresh: &ObservedPage,
    actions: Vec<Value>,
) -> Result<DispatchOutcome, BrowserError> {
    let context = &fresh
        .page
        .sources
        .first()
        .ok_or(BrowserError::NoObservation)?
        .context;
    let params = json!({"context":context,"actions":[{"type":"key","id":"ceh-keyboard","actions":actions}]});
    let mut ticket = None;
    let sent = permit.send(&fresh.observation, || {
        send(connection, "input.performActions", params, &mut ticket)
    })?;
    let Some(ticket) = ticket else {
        if sent == DispatchOutcome::Unknown {
            permit.complete_step(sent)?;
        }
        return Ok(sent);
    };
    let outcome = match connection.receive(ticket) {
        Ok(_) => DispatchOutcome::Dispatched,
        Err(_) => DispatchOutcome::Unknown,
    };
    permit.complete_step(outcome)?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_click_moves_along_a_curve_then_presses_on_the_element() {
        let actions = super::click_actions("node-1", 42);
        let moves: Vec<_> = actions.iter().filter(|a| a["type"] == "pointerMove").collect();
        assert!(moves.len() >= 8, "several moves, not one jump");
        assert!(moves.iter().all(|m| m["origin"]["element"]["sharedId"] == "node-1"));
        let last = moves.last().unwrap();
        assert_eq!((last["x"].as_i64(), last["y"].as_i64()), (Some(0), Some(0)));
        let kinds: Vec<_> = actions.iter().rev().take(3).map(|a| a["type"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["pointerUp", "pause", "pointerDown"]);
    }

    use super::*;
    #[test]
    fn clear_holds_the_platform_modifier_around_a_then_deletes() {
        for (mac, modifier) in [(true, META), (false, CONTROL)] {
            let actions = clear_actions(mac);
            let sequence: Vec<_> = actions.iter().map(|a| (a["type"].as_str().unwrap(), a["value"].as_str().unwrap())).collect();
            assert_eq!(sequence, vec![
                ("keyDown", modifier), ("keyDown", "a"), ("keyUp", "a"), ("keyUp", modifier),
                ("keyDown", BACKSPACE), ("keyUp", BACKSPACE),
            ]);
        }
    }

    /// Launches the installed Firefox (headless, temporary profile) on a 127.0.0.1 page: run with `-- --ignored`.
    #[test]
    #[ignore = "launches the installed Firefox"]
    fn real_firefox_reports_only_trusted_events_for_clear_text_and_select() {
        use crate::firefox::{FirefoxInstallation, OwnedFirefox, ProfileLease};
        use std::io::{Read, Write};
        let Ok(installation) = FirefoxInstallation::discover() else { return };
        let page = include_str!("../tests/fixtures/trusted-events.html");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer);
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}", page.len());
            }
        });
        let runtime = tempfile::tempdir().unwrap();
        let lease = ProfileLease::temporary(runtime.path()).unwrap();
        let mut browser = OwnedFirefox::start(&installation, lease, true, &std::sync::atomic::AtomicBool::new(false)).unwrap();
        let context = browser.context().to_owned();
        let call = |browser: &mut OwnedFirefox, method: &str, params: Value| {
            let result = browser.connection().call(method, params).unwrap();
            let _ = browser.connection().take_events();
            result
        };
        call(&mut browser, "browsingContext.navigate", json!({"context":context,"url":url,"wait":"complete"}));
        let eval = |browser: &mut OwnedFirefox, expression: &str| -> Value {
            let result = call(browser, "script.evaluate", json!({"expression":format!("JSON.stringify({expression})"),"target":{"context":context},"awaitPromise":false}));
            serde_json::from_str(result["result"]["value"].as_str().unwrap()).unwrap()
        };
        let text_actions = |text: &str| -> Vec<Value> {
            text.chars().flat_map(|c| [json!({"type":"keyDown","value":c.to_string()}), json!({"type":"keyUp","value":c.to_string()})]).collect()
        };
        let keys = |browser: &mut OwnedFirefox, actions: Vec<Value>| {
            browser.connection().call("input.performActions", json!({"context":context,"actions":[{"type":"key","id":"k","actions":actions}]})).unwrap();
            let _ = browser.connection().take_events();
            std::thread::sleep(std::time::Duration::from_millis(150));
        };
        // Test-only focus; production focus is a trusted pointer press.
        let focus = |browser: &mut OwnedFirefox, id: &str| {
            let _ = browser.connection().call("script.evaluate", json!({"expression":format!("document.getElementById('{id}').focus()"),"target":{"context":context},"awaitPromise":false}));
        };
        focus(&mut browser, "txtIdCitado");
        keys(&mut browser, text_actions("Y1234567X"));
        assert_eq!(eval(&mut browser, "document.getElementById('txtIdCitado').value"), json!("Y1234567X"));
        keys(&mut browser, clear_actions(cfg!(target_os = "macos")));
        assert_eq!(eval(&mut browser, "document.getElementById('txtIdCitado').value"), json!(""));
        let events = eval(&mut browser, "window.__events.filter(e=>['input'].includes(e.type)).map(e=>e.trusted)");
        assert!(events.as_array().unwrap().len() >= 9 && events.as_array().unwrap().iter().all(|v| v == true), "{events}");
        assert_eq!(eval(&mut browser, "window.__events.filter(e=>!e.trusted).length"), json!(0));
        let _ = browser.stop();
    }
}
