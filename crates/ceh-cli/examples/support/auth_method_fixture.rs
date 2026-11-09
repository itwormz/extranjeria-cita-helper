//! Typed authentication method selection, synthetic provider origins only.
use super::{
    Client, Result,
    portal_fixture::{Fixture, seed_observer},
};
use ceh_browser::{
    BrowserError, DispatchGate,
    auth::{AuthenticationChoice, read_auth_facts, select_method},
    observer::PageObserver,
};
use ceh_core::{
    flow::{AuthenticationMode, ClaveMethod, FlowEffect, FlowEngine, PageFacts},
    model::*,
};
use serde_json::json;
const PROVIDER: &str = "https://pasarela.clave.gob.es/Clave/ServiceProvider";
const ALTERNATE: &str = "https://pasarela-ident.clave.gob.es/Clave/ServiceProvider";
const QR: &str = "https://www2.agenciatributaria.gob.es/wlpl/SSOP-JDIT/ObtenerClaveMovilQR";
const LOGIN: &str = "https://pasarela.clave.gob.es/Clave/login";
const RETURN: &str = "/icpplus/acEntrada";
const FOREIGN: &str = "https://untrusted.invalid/Clave/ServiceProvider";
fn choices() -> String {
    format!(
        "<body><form method='post' action='https://icp.administracionelectronica.gob.es{RETURN}'><button onclick='if(!event.isTrusted)event.preventDefault()'>Certificado electrónico</button></form><form method='post' action='{QR}'><button onclick='if(!event.isTrusted)event.preventDefault()'>Cl@ve Móvil</button></form><form method='post' action='{LOGIN}'><button onclick='if(!event.isTrusted)event.preventDefault()'>Cl@ve Permanente</button></form><button style='display:none'>Certificado electrónico</button></body>"
    )
}
pub fn run(client: &mut Client, context: &str) -> Result<()> {
    let mut fixture = Fixture::new(client, context, vec![])?;
    let mut cases = 0;
    for provider in [PROVIDER, ALTERNATE] {
        for choice in [
            AuthenticationChoice::Certificate,
            AuthenticationChoice::Movil,
            AuthenticationChoice::Permanente,
        ] {
            fixture.routes = vec![
                ("GET", provider, choices()),
                (
                    "POST",
                    RETURN,
                    "<body><input id='txtIdCitado'><button id='btnEnviar'>Aceptar</button></body>"
                        .into(),
                ),
                (
                    "POST",
                    QR,
                    "<body><canvas width='40' height='40'></canvas></body>".into(),
                ),
                (
                    "POST",
                    LOGIN,
                    "<body><input id='nie'><input type='password' id='pin'></body>".into(),
                ),
            ];
            fixture.navigate(client, context, provider)?;
            let mut observer = PageObserver::new(context).map_err(|_| "method observer invalid")?;
            let mut cursor = 0;
            seed_observer(client, &mut observer, &mut cursor)?;
            let page = observer
                .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
                .map_err(|_| "method observation failed")?;
            let metadata = read_auth_facts(client.bidi.as_mut().ok_or("missing BiDi")?, &page)
                .map_err(|_| "method facts failed")?;
            let session = SessionId::new(format!("method-fixture-{cases}"))?;
            let gate = DispatchGate::new(session.clone());
            let mut core = FlowEngine::new(session, SearchOptions::default());
            let facts = PageFacts {
                authentication: if choice == AuthenticationChoice::Certificate {
                    AuthenticationMode::Certificate
                } else {
                    AuthenticationMode::Clave
                },
                certificate_method_available: metadata.certificate,
                clave_method: if choice == AuthenticationChoice::Permanente {
                    ClaveMethod::Permanente
                } else {
                    ClaveMethod::Movil
                },
                clave_method_available: if choice == AuthenticationChoice::Permanente {
                    metadata.permanente
                } else {
                    metadata.movil
                },
                ..Default::default()
            };
            let effects = core
                .observe_with_facts(page.observation, facts, 0)
                .map_err(|_| "method core failed")?;
            let guard = match effects.first() {
                Some(FlowEffect::SelectCertificate { guard })
                    if choice == AuthenticationChoice::Certificate =>
                {
                    guard
                }
                Some(FlowEffect::SelectClaveMethod { guard, .. })
                    if choice != AuthenticationChoice::Certificate =>
                {
                    guard
                }
                _ => return Err("core method selection mismatch".into()),
            };
            let mut publication_count = 0;
            let sent = select_method(
                client.bidi.as_mut().ok_or("missing BiDi")?,
                &mut observer,
                &gate,
                guard,
                choice,
                || {
                    publication_count += 1;
                    core.certificate_published(guard, 1)
                        .expect("trusted certificate publication rejected");
                },
            )
            .map_err(|e| format!("native method failed: {e:?}"))?;
            if sent != DispatchOutcome::Dispatched
                || publication_count != usize::from(choice == AuthenticationChoice::Certificate)
            {
                return Err("method publication/receipt mismatch".into());
            }
            core.acknowledge_dispatch(guard, ActionKind::Auth, sent)
                .map_err(|_| "method ack failed")?;
            let target = match choice {
                AuthenticationChoice::Certificate => RETURN,
                AuthenticationChoice::Movil => QR,
                AuthenticationChoice::Permanente => LOGIN,
            };
            fixture.wait_load(client, context, target)?;
            seed_observer(client, &mut observer, &mut cursor)?;
            let page = observer
                .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
                .map_err(|_| "method result read failed")?;
            if choice == AuthenticationChoice::Certificate {
                if page.observation.kind != PageKind::Applicant {
                    return Err("certificate did not return to applicant".into());
                }
                if core
                    .observe(page.observation, 2)
                    .map_err(|_| "certificate return core failed")?
                    .iter()
                    .any(|e| matches!(e, FlowEffect::Alert))
                {
                    return Err("fast certificate return raised alarm".into());
                }
            } else {
                let metadata = read_auth_facts(client.bidi.as_mut().ok_or("missing BiDi")?, &page)
                    .map_err(|_| "interactive facts failed")?;
                let mut f = facts;
                f.clave_method_available = false;
                f.clave_requires_attention = metadata.requires_attention;
                f.clave_input_started = metadata.input_started;
                if !core
                    .observe_with_facts(page.observation, f, 2)
                    .map_err(|_| "interactive core failed")?
                    .iter()
                    .any(|e| matches!(e, FlowEffect::Alert))
                {
                    return Err("interactive auth did not call human".into());
                }
                if choice == AuthenticationChoice::Permanente {
                    client.call("script.evaluate",json!({"expression":"document.getElementById('nie').value='SYNTHETIC-STARTED';true","target":{"context":context},"awaitPromise":false}))?;
                    let page = observer
                        .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
                        .map_err(|_| "input acknowledgement read failed")?;
                    let m = read_auth_facts(client.bidi.as_mut().ok_or("missing BiDi")?, &page)
                        .map_err(|_| "input acknowledgement facts failed")?;
                    f.clave_input_started = m.input_started;
                    if !core
                        .observe_with_facts(page.observation, f, 3)
                        .map_err(|_| "input acknowledgement core failed")?
                        .iter()
                        .any(|e| matches!(e, FlowEffect::Silence))
                    {
                        return Err("input did not silence auth alarm".into());
                    }
                }
            }
            cases += 1;
        }
    }
    // Even a valid-looking button on another origin must not receive a click.
    {
        let (url, body, expected) = (FOREIGN, choices(), BrowserError::Unsupported);
        fixture.routes = vec![("GET", url, body)];
        fixture.navigate(client, context, url)?;
        let mut observer = PageObserver::new(context).map_err(|_| "foreign observer invalid")?;
        let mut cursor = 0;
        seed_observer(client, &mut observer, &mut cursor)?;
        let page = observer
            .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
            .map_err(|_| "foreign page read failed")?;
        let session = SessionId::new("foreign-method")?;
        let gate = DispatchGate::new(session.clone());
        let guard = DocumentGuard {
            session,
            epoch: 0,
            document: page.observation.document,
            expected_page: PageKind::AuthWait,
        };
        let result = select_method(
            client.bidi.as_mut().ok_or("missing BiDi")?,
            &mut observer,
            &gate,
            &guard,
            AuthenticationChoice::Certificate,
            || panic!("foreign publication"),
        );
        if result != Err(expected) {
            return Err("foreign provider action not rejected".into());
        }
    }
    // Refusal must not publish a certificate reminder or submit anything.
    for paused in [false, true] {
        let body = if paused {
            choices()
        } else {
            format!("{}<button>Certificado electrónico</button>", choices())
        };
        fixture.routes = vec![("GET", PROVIDER, body)];
        fixture.navigate(client, context, PROVIDER)?;
        let mut observer = PageObserver::new(context).map_err(|_| "refusal observer invalid")?;
        let mut cursor = 0;
        seed_observer(client, &mut observer, &mut cursor)?;
        let page = observer
            .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
            .map_err(|_| "refusal observation failed")?;
        let session = SessionId::new("refused-method")?;
        let gate = DispatchGate::new(session.clone());
        let guard = DocumentGuard {
            session: session.clone(),
            epoch: 0,
            document: page.observation.document,
            expected_page: PageKind::AuthWait,
        };
        if paused {
            let mut authority = ceh_core::control::Authority::new(session);
            gate.apply_control(&authority.pause().map_err(|_| "authority pause failed")?)
                .map_err(|_| "pause refused")?;
        }
        let result = select_method(
            client.bidi.as_mut().ok_or("missing BiDi")?,
            &mut observer,
            &gate,
            &guard,
            AuthenticationChoice::Certificate,
            || panic!("refused certificate publication"),
        );
        let expected = if paused {
            Err(BrowserError::Paused)
        } else {
            Ok(DispatchOutcome::NotDispatched)
        };
        if result != expected {
            return Err("ambiguous/paused method not refused".into());
        }
    }
    if fixture.posts != 6 {
        return Err("method POST repeated".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"no_live_site_requests":true,"no_real_certificate_or_credentials":true,"official_provider_variants":2,"typed_methods":3,"native_cases":cases,"post_requests":fixture.posts,"certificate_publication_hook_only":true,"fast_certificate_no_alarm":true,"clave_interactive_alarm_and_input_silence":true,"foreign_method_rejected":true,"ambiguous_and_paused_no_publication":true})
        )?
    );
    Ok(())
}
