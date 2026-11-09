//! Correct recovery-message ownership, including visible/hidden iframes; read only.
use super::{
    Client, Result,
    portal_fixture::{Fixture, seed_observer},
};
use ceh_browser::{BrowserError, observer::PageObserver, recovery_controls};
use ceh_core::{
    flow::{FlowEffect, FlowEngine, PageFacts},
    model::*,
    recovery::RecoveryAction,
};
use serde_json::json;
const ROOT: &str = "/icpplus/acInfo";
const FRAME: &str = "/icpplus/recovery-frame";
fn surrounding() -> &'static str {
    "<form><input id='txtIdCitado' value='SYNTHETIC-NIE'><button type='button' onclick='window.recoveryClicks++'>Aceptar</button></form><input id='manual' value='synthetic'>"
}
pub fn run(client: &mut Client, context: &str, activate: bool) -> Result<()> {
    let mut fixture = Fixture::new(client, context, vec![])?;
    let mut cases = 0;
    for (nested, message, kind) in [
        (false, "Sesión caducada", PageKind::Expired),
        (true, "Sesión caducada", PageKind::Expired),
        (false, "500 Internal Server Error", PageKind::ServerError),
    ] {
        let region = if activate {
            format!(
                "<section><p>{message}</p><form method='post' action='https://icp.administracionelectronica.gob.es/icpplus/recovered' target='_top'><button onclick='if(!event.isTrusted)event.preventDefault()'>Volver</button></form></section>"
            )
        } else {
            format!(
                "<div><p>{message}</p><button type='button' onclick='window.recoveryClicks++'>Volver</button></div>"
            )
        };
        let body = if nested {
            format!("{}<iframe src='{FRAME}'></iframe>", surrounding())
        } else {
            format!("{}{region}", surrounding())
        };
        fixture.routes=vec![("GET",ROOT,format!("<body style='min-height:1600px'><script>window.recoveryClicks=0</script>{body}</body>")),("GET",FRAME,format!("<body>{region}</body>")),("POST","/icpplus/recovered","<body><select id='form'><option>Madrid</option></select><button id='btnAceptar'>Aceptar</button></body>".into())];
        fixture.navigate(client, context, ROOT)?;
        client.call("script.evaluate",json!({"expression":"document.getElementById('manual').focus({preventScroll:true});window.scrollTo(0,200);true","target":{"context":context},"awaitPromise":false}))?;
        let mut observer = PageObserver::new(context).map_err(|_| "recovery observer")?;
        let mut cursor = 0;
        seed_observer(client, &mut observer, &mut cursor)?;
        let page = observer
            .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
            .map_err(|_| "recovery observation")?;
        if page.observation.kind != kind {
            return Err("recovery kind mismatch".into());
        }
        let read = recovery_controls::read(
            client.bidi.as_mut().ok_or("missing BiDi")?,
            &page,
            None,
            None,
            false,
        )
        .map_err(|e| format!("recovery ownership failed: {e:?}"))?;
        if !read.facts.own_control
            || read.label() != Some("Volver")
            || (read.source.context != context) != nested
        {
            return Err("wrong recovery-message owner".into());
        }
        let mut core = FlowEngine::new(
            SessionId::new(format!("recovery-facts-{cases}"))?,
            SearchOptions::default(),
        );
        let facts = PageFacts {
            recovery: read.facts,
            ..Default::default()
        };
        let effects = core
            .observe_with_facts(page.observation.clone(), facts, 0)
            .map_err(|_| "recovery plan failed")?;
        let Some(FlowEffect::WaitUntil(deadline)) = effects.first() else {
            return Err("recovery bypassed cooldown".into());
        };
        if *deadline > 0
            && !core
                .observe_with_facts(page.observation.clone(), facts, deadline - 1)
                .map_err(|_| "cooldown check")?
                .is_empty()
        {
            return Err("early recovery".into());
        }
        let due = core
            .observe_with_facts(page.observation, facts, *deadline)
            .map_err(|_| "due recovery")?;
        let [
            FlowEffect::Recover {
                guard,
                action: RecoveryAction::ActivateOwnControl,
            },
        ] = due.as_slice()
        else {
            return Err("own recovery control not planned".into());
        };
        let guard = guard.clone();
        let kept=client.call("script.evaluate",json!({"expression":"document.activeElement.id==='manual'&&document.getElementById('manual').value==='synthetic'&&window.scrollY===200&&window.recoveryClicks===0","target":{"context":context},"awaitPromise":false}))?;
        if kept["result"]["value"] != true {
            return Err("read-only recovery changed manual page state".into());
        }
        if activate {
            let gate = ceh_browser::DispatchGate::new(core.control().session().clone());
            let outcome = recovery_controls::activate_own_control(
                client.bidi.as_mut().ok_or("missing BiDi")?,
                &mut observer,
                &gate,
                &guard,
                &read,
            )
            .map_err(|e| format!("scoped recovery failed: {e:?}"))?;
            if outcome != DispatchOutcome::Dispatched {
                return Err("recovery click not acknowledged".into());
            }
            core.acknowledge_dispatch(&guard, ActionKind::Recovery, outcome)
                .map_err(|_| "recovery ack failed")?;
            fixture.wait_load(client, context, "/icpplus/recovered")?;
        }
        cases += 1;
    }
    if activate {
        for change in [
            "pause",
            "handoff",
            "hide",
            "replace",
            "ambiguous",
            "changed-text",
        ] {
            fixture.routes = vec![
                ("GET", ROOT, format!("<body>{}<iframe src='{FRAME}'></iframe></body>", surrounding())),
                ("GET", FRAME, "<body><section><p>Sesión caducada</p><form method='post' target='_top' action='https://icp.administracionelectronica.gob.es/icpplus/recovered'><button>Volver</button></form></section></body>".into()),
                ("POST", "/icpplus/recovered", "<body>unexpected recovery</body>".into()),
            ];
            fixture.navigate(client, context, ROOT)?;
            let mut observer = PageObserver::new(context).map_err(|_| "negative observer")?;
            let mut cursor = 0;
            seed_observer(client, &mut observer, &mut cursor)?;
            let page = observer
                .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
                .map_err(|_| "negative observation")?;
            let read = recovery_controls::read(
                client.bidi.as_mut().ok_or("missing BiDi")?,
                &page,
                None,
                None,
                false,
            )
            .map_err(|_| "negative ownership")?;
            if !read.facts.own_control {
                return Err("negative fixture missing original control".into());
            }
            let session = SessionId::new(format!("recovery-negative-{change}"))?;
            let gate = ceh_browser::DispatchGate::new(session.clone());
            let guard = DocumentGuard {
                session: session.clone(),
                epoch: 0,
                document: page.observation.document,
                expected_page: PageKind::Expired,
            };
            let mut authority = ceh_core::control::Authority::new(session);
            match change {
                "pause" => gate
                    .apply_control(&authority.pause().map_err(|_| "pause")?)
                    .map_err(|_| "pause gate")?,
                "handoff" => gate
                    .apply_control(&authority.handoff().map_err(|_| "handoff")?)
                    .map_err(|_| "handoff gate")?,
                _ => {
                    let (target, expression) = match change {
                        "hide" => (
                            json!({"context":context}),
                            "document.querySelector('iframe').style.display='none';true",
                        ),
                        "replace" => (
                            json!({"context":context}),
                            "document.querySelector('iframe').remove();true",
                        ),
                        "ambiguous" => (
                            json!({"realm":read.source.realm.as_str()}),
                            "document.querySelector('form').appendChild(document.createElement('button')).textContent='Volver';true",
                        ),
                        _ => (
                            json!({"realm":read.source.realm.as_str()}),
                            "document.querySelector('p').textContent='Sesión caducada — changed';true",
                        ),
                    };
                    client.call(
                        "script.evaluate",
                        json!({"expression":expression,"target":target,"awaitPromise":false}),
                    )?;
                }
            }
            let result = recovery_controls::activate_own_control(
                client.bidi.as_mut().ok_or("missing BiDi")?,
                &mut observer,
                &gate,
                &guard,
                &read,
            );
            if !matches!(result, Err(_) | Ok(DispatchOutcome::NotDispatched)) {
                return Err(format!("unsafe recovery after {change}").into());
            }
            let untouched = client.call("script.evaluate",json!({"expression":"document.getElementById('manual').value==='synthetic'","target":{"context":context},"awaitPromise":false}))?;
            if untouched["result"]["value"] != true {
                return Err("negative recovery touched applicant".into());
            }
        }
    }
    fixture.routes = vec![
        (
            "GET",
            ROOT,
            format!(
                "<body>{}<iframe style='display:none' src='{FRAME}'></iframe></body>",
                surrounding()
            ),
        ),
        (
            "GET",
            FRAME,
            "<body>Sesión caducada<button>Volver</button></body>".into(),
        ),
    ];
    fixture.navigate(client, context, ROOT)?;
    let mut observer = PageObserver::new(context).map_err(|_| "hidden observer")?;
    let mut cursor = 0;
    seed_observer(client, &mut observer, &mut cursor)?;
    let page = observer
        .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
        .map_err(|_| "hidden read")?;
    if page.observation.kind != PageKind::Applicant {
        return Err("hidden error frame took authority".into());
    }
    if !matches!(
        recovery_controls::read(
            client.bidi.as_mut().ok_or("missing BiDi")?,
            &page,
            None,
            None,
            false
        ),
        Err(BrowserError::Unsupported)
    ) {
        return Err("normal applicant offered recovery".into());
    }
    if fixture.posts != if activate { 3 } else { 0 } {
        return Err("recovery POST count mismatch".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"native_cases":cases,"negative_cases":if activate {6}else{0},"visible_iframe_owner":true,"surrounding_applicant_ignored":true,"hidden_error_ignored":true,"core_cooldowns_preserved":true,"manual_focus_input_scroll_preserved":true,"no_live_site_requests":true,"post_requests":fixture.posts,"action_dispatch_not_yet_implemented":!activate})
        )?
    );
    Ok(())
}
