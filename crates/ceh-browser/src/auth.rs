//! Authentication entry and public booleans only; credentials never leave the page.
use crate::{
    BrowserError, DispatchGate, DispatchOutcome,
    bidi::BidiConnection,
    observer::{ObservedPage, PageObserver},
    trusted_input::{self, Target},
};
use ceh_core::model::{ActionKind, DocumentGuard, PageKind};
use serde_json::{Value, json};
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthFacts {
    pub portal_entry: bool,
    pub provider: bool,
    pub certificate: bool,
    pub movil: bool,
    pub permanente: bool,
    pub requires_attention: bool,
    pub input_started: bool,
}
impl AuthFacts {
    pub fn decode(raw: &str) -> Result<Self, BrowserError> {
        if raw.len() > 2048 {
            return Err(BrowserError::NoObservation);
        }
        let value: Value = serde_json::from_str(raw).map_err(|_| BrowserError::NoObservation)?;
        let map = value
            .as_object()
            .filter(|v| v.len() == 7)
            .ok_or(BrowserError::NoObservation)?;
        let field = |name: &str| {
            map.get(name)
                .and_then(Value::as_bool)
                .ok_or(BrowserError::NoObservation)
        };
        Ok(Self {
            portal_entry: field("portal_entry")?,
            provider: field("provider")?,
            certificate: field("certificate")?,
            movil: field("movil")?,
            permanente: field("permanente")?,
            requires_attention: field("requires_attention")?,
            input_started: field("input_started")?,
        })
    }
}
pub fn read_auth_facts(
    connection: &mut BidiConnection,
    page: &ObservedPage,
) -> Result<AuthFacts, BrowserError> {
    if !matches!(page.observation.kind, PageKind::Auth | PageKind::AuthWait) {
        return Err(BrowserError::Unsupported);
    }
    let result=connection.call("script.callFunction",json!({"target":{"realm":page.observation.document.as_str()},"functionDeclaration":include_str!("auth-facts.js"),"awaitPromise":false})).map_err(|_|BrowserError::NoObservation)?;
    if result["type"] != "success" || result["result"]["type"] != "string" {
        return Err(BrowserError::NoObservation);
    }
    AuthFacts::decode(
        result["result"]["value"]
            .as_str()
            .ok_or(BrowserError::NoObservation)?,
    )
}
/// Both certificate and Cl@ve first enter the same official portal button.
pub fn enter_portal(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
) -> Result<DispatchOutcome, BrowserError> {
    for attempt in 0..3 {
        let fresh = observer
            .observe(connection, "manual")
            .map_err(|_| BrowserError::NoObservation)?;
        if !fresh.page.snapshot.official {
            return Err(BrowserError::Unsupported);
        }
        if fresh.observation.kind != PageKind::Auth || fresh.observation.document != guard.document {
            return Err(BrowserError::StaleDocument);
        }
        match trusted_input::click(
            connection,
            observer,
            gate,
            guard,
            fresh,
            Target::icp(ActionKind::Auth, json!({}), include_str!("auth-entry.js")),
        ) {
            // These paths precede input.performActions. Any acknowledged or
            // ambiguous click returns immediately and is never repeated.
            Ok(DispatchOutcome::NotDispatched) | Err(BrowserError::StaleDocument)
                if attempt < 2 =>
            {
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            result => return result,
        }
    }
    Ok(DispatchOutcome::NotDispatched)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthenticationChoice {
    Certificate,
    Movil,
    Permanente,
}
/// Only public method controls on the two known Cl@ve provider origins.
/// Callback must not block. It records publication, never permission for a write.
pub fn select_method(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    choice: AuthenticationChoice,
    mut on_published: impl FnMut() + Send,
) -> Result<DispatchOutcome, BrowserError> {
    let fresh = observer
        .observe(connection, "manual")
        .map_err(|_| BrowserError::NoObservation)?;
    if fresh.observation.kind != PageKind::AuthWait {
        return Err(BrowserError::StaleDocument);
    }
    let facts = read_auth_facts(connection, &fresh)?;
    if !facts.provider {
        return Err(BrowserError::Unsupported);
    }
    let (available, method) = match choice {
        AuthenticationChoice::Certificate => (facts.certificate, "certificate"),
        AuthenticationChoice::Movil => (facts.movil, "movil"),
        AuthenticationChoice::Permanente => (facts.permanente, "permanente"),
    };
    if !available {
        return Ok(DispatchOutcome::NotDispatched);
    }
    let hook = if choice == AuthenticationChoice::Certificate {
        Some(&mut on_published as &mut (dyn FnMut() + Send))
    } else {
        None
    };
    trusted_input::click(
        connection,
        observer,
        gate,
        guard,
        fresh,
        Target::provider(
            json!({"method":method}),
            include_str!("auth-method.js"),
            hook,
        ),
    )
}
