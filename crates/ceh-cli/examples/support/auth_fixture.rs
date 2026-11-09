//! Synthetic external auth origins. No real accounts/certificates are used.
use super::{
    Client, Result,
    portal_fixture::{Fixture, seed_observer},
};
use ceh_browser::{
    BrowserError, DispatchGate,
    auth::{AuthFacts, enter_portal, read_auth_facts},
    observer::PageObserver,
};
use ceh_core::{
    control::Authority,
    flow::{FlowEffect, FlowEngine},
    model::*,
};
use serde_json::json;
const ENTRY: &str = "/icpplus/acInfo";
const PROVIDER: &str = "https://pasarela.clave.gob.es/Clave/ServiceProvider";
const LOGIN: &str = "https://pasarela.clave.gob.es/Clave/login";
const QR: &str = "https://www2.agenciatributaria.gob.es/wlpl/SSOP-JDIT/ObtenerClaveMovilQR";
const FOREIGN: &str = "https://untrusted.invalid/login";
const CHOICES: &str = "<body><button>Acceso DNIe / Certificado electrónico</button><button>Cl@ve Móvil</button><button>Cl@ve Permanente</button></body>";
fn inspect(
    client: &mut Client,
    observer: &mut PageObserver,
    cursor: &mut usize,
) -> Result<AuthFacts> {
    seed_observer(client, observer, cursor)?;
    let page = observer
        .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
        .map_err(|_| "auth page read failed")?;
    read_auth_facts(client.bidi.as_mut().ok_or("missing BiDi")?, &page)
        .map_err(|_| "auth facts read failed".into())
}
pub fn run(client: &mut Client, context: &str) -> Result<()> {
    let mut fixture = Fixture::new(
        client,
        context,
        vec![
            (
                "GET",
                ENTRY,
                format!(
                    "<body><form method='post' action='{PROVIDER}'><div id='btnAccesoClave' style='width:100%;height:180px'><button onclick='if(!event.isTrusted)event.preventDefault()'>Acceder con Cl@ve</button></div></form></body>"
                ),
            ),
            ("POST", PROVIDER, CHOICES.into()),
            (
                "GET",
                LOGIN,
                "<body><input id='nie'><input id='pin' type='password'></body>".into(),
            ),
            (
                "GET",
                QR,
                "<body><canvas width='40' height='40'></canvas></body>".into(),
            ),
            (
                "GET",
                FOREIGN,
                "<body><input value='not-exported'><button>Cl@ve Permanente</button></body>".into(),
            ),
        ],
    )?;
    fixture.navigate(client, context, ENTRY)?;
    let mut observer = PageObserver::new(context).map_err(|_| "auth observer invalid")?;
    let mut cursor = 0;
    let facts = inspect(client, &mut observer, &mut cursor)?;
    if !facts.portal_entry || facts.requires_attention {
        return Err("portal entry facts incorrect".into());
    }
    let page = observer
        .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
        .map_err(|_| "auth initial read failed")?;
    let session = SessionId::new("auth-entry-fixture")?;
    let gate = DispatchGate::new(session.clone());
    let mut flow = FlowEngine::new(session, SearchOptions::default());
    let effects = flow
        .observe(page.observation, 0)
        .map_err(|_| "auth core failed")?;
    let Some(FlowEffect::EnterAuthentication { guard, .. }) = effects.first() else {
        return Err("core did not enter authentication".into());
    };
    let probe_session = SessionId::new("auth-manual-probe")?;
    let probe = DispatchGate::new(probe_session.clone());
    let mut authority = Authority::new(probe_session.clone());
    probe
        .apply_control(
            &authority
                .handoff()
                .map_err(|_| "auth probe handoff failed")?,
        )
        .map_err(|_| "probe control failed")?;
    let mut manual = guard.clone();
    manual.session = probe_session;
    if !matches!(
        enter_portal(
            client.bidi.as_mut().ok_or("missing BiDi")?,
            &mut observer,
            &probe,
            &manual
        ),
        Err(BrowserError::ManualOwnership)
    ) {
        return Err("manual auth entry was not blocked".into());
    }
    let sent = enter_portal(
        client.bidi.as_mut().ok_or("missing BiDi")?,
        &mut observer,
        &gate,
        guard,
    )
    .map_err(|_| "portal entry failed")?;
    if sent != DispatchOutcome::Dispatched {
        return Err("portal entry unacknowledged".into());
    }
    flow.acknowledge_dispatch(guard, ActionKind::Auth, sent)
        .map_err(|_| "auth dispatch ack failed")?;
    fixture.wait_load(client, context, PROVIDER)?;
    let facts = inspect(client, &mut observer, &mut cursor)?;
    if !facts.provider
        || !facts.certificate
        || !facts.movil
        || !facts.permanente
        || facts.requires_attention
        || facts.input_started
    {
        return Err("method choice page looked interactive".into());
    }
    fixture.navigate(client, context, LOGIN)?;
    let facts = inspect(client, &mut observer, &mut cursor)?;
    if !facts.requires_attention || facts.input_started {
        return Err("empty credentials not recognized".into());
    }
    client.call("script.evaluate",json!({"expression":"document.getElementById('nie').value='SYNTHETIC-PRIVATE';document.getElementById('pin').value='DO-NOT-EXPORT';true","target":{"context":context},"awaitPromise":false}))?;
    if !inspect(client, &mut observer, &mut cursor)?.input_started {
        return Err("credential input boolean missing".into());
    }
    fixture.navigate(client, context, QR)?;
    if !inspect(client, &mut observer, &mut cursor)?.requires_attention {
        return Err("QR did not require attention".into());
    }
    fixture.navigate(client, context, FOREIGN)?;
    if inspect(client, &mut observer, &mut cursor)? != AuthFacts::default() {
        return Err("foreign host accepted as auth provider".into());
    }
    if fixture.posts != 1 {
        return Err("auth entry repeated POST".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"no_live_site_requests":true,"no_real_certificate_or_credentials":true,"trusted_portal_entry":true,"manual_entry_refused":true,"methods_not_misclassified_as_credentials":true,"editable_and_qr_attention":true,"input_boolean_without_values":true,"foreign_origin_rejected":true,"post_requests":fixture.posts})
        )?
    );
    Ok(())
}
