//! Isolated cookie banner: trusted click, refusal and manual-page preservation.
use super::{
    Client, Result,
    portal_fixture::{Fixture, seed_observer},
};
use ceh_browser::{BrowserError, DispatchGate, cookies, observer::PageObserver};
use ceh_core::{
    flow::{FlowEffect, FlowEngine, PageFacts},
    model::*,
};
use serde_json::json;
const PATH: &str = "/icpplus/index";
fn banner() -> &'static str {
    "<div id='cookie-law-info-bar'><button id='cookie_action_close_header' onclick='if(event.isTrusted){window.cookieClicks=(window.cookieClicks||0)+1;this.parentElement.hidden=true}'>Aceptar</button></div>"
}
fn province() -> String {
    format!(
        "<body><select id='form'><option>Madrid</option></select><button id='btnAceptar'>Aceptar</button>{}</body>",
        banner()
    )
}
pub fn run(client: &mut Client, context: &str) -> Result<()> {
    let mut fixture = Fixture::new(client, context, vec![("GET", PATH, province())])?;
    fixture.navigate(client, context, PATH)?;
    let mut observer = PageObserver::new(context).map_err(|_| "cookie observer invalid")?;
    let mut cursor = 0;
    seed_observer(client, &mut observer, &mut cursor)?;
    let page = observer
        .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
        .map_err(|_| "cookie page failed")?;
    let available = cookies::control_available(client.bidi.as_mut().ok_or("missing BiDi")?, &page)
        .map_err(|_| "cookie fact failed")?;
    if !available {
        return Err("cookie banner not detected".into());
    }
    let session = SessionId::new("cookie-fixture")?;
    let gate = DispatchGate::new(session.clone());
    let mut core = FlowEngine::new(session, SearchOptions::default());
    let effects = core
        .observe_with_facts(
            page.observation,
            PageFacts {
                cookie_accept_available: available,
                ..Default::default()
            },
            0,
        )
        .map_err(|_| "cookie core failed")?;
    let [FlowEffect::AcceptCookies { guard }] = effects.as_slice() else {
        return Err("cookie did not precede form".into());
    };
    let outcome = cookies::accept(
        client.bidi.as_mut().ok_or("missing BiDi")?,
        &mut observer,
        &gate,
        guard,
    )
    .map_err(|e| format!("cookie click failed: {e:?}"))?;
    if outcome != DispatchOutcome::Dispatched {
        return Err("cookie click not acknowledged".into());
    }
    core.acknowledge_dispatch(guard, ActionKind::Cookie, outcome)
        .map_err(|_| "cookie ack failed")?;
    let page = observer
        .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
        .map_err(|_| "post-cookie read failed")?;
    if cookies::control_available(client.bidi.as_mut().ok_or("missing BiDi")?, &page)
        .map_err(|_| "post-cookie fact failed")?
    {
        return Err("cookie remained visible".into());
    }
    if !matches!(
        core.observe(page.observation, 1)
            .map_err(|_| "form continuation failed")?
            .as_slice(),
        [FlowEffect::Prepare {
            action: ActionKind::Province,
            ..
        }]
    ) {
        return Err("form did not resume".into());
    }
    let result=client.call("script.evaluate",json!({"expression":"window.cookieClicks===1","target":{"context":context},"awaitPromise":false}))?;
    if result["result"]["value"] != true {
        return Err("cookie click repeated or untrusted".into());
    }
    // A real booking shape is never mutated, even though the familiar banner exists.
    fixture.routes = vec![(
        "GET",
        PATH,
        format!(
            "<body><input name='fechaCita'><input id='captcha'>{}</body>",
            banner()
        ),
    )];
    fixture.navigate(client, context, PATH)?;
    seed_observer(client, &mut observer, &mut cursor)?;
    let page = observer
        .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
        .map_err(|_| "manual page failed")?;
    if page.observation.kind != PageKind::Booking {
        return Err("booking not recognized".into());
    }
    if cookies::control_available(client.bidi.as_mut().ok_or("missing BiDi")?, &page)
        .map_err(|_| "manual cookie fact failed")?
    {
        return Err("manual banner exposed as action".into());
    }
    let guard = DocumentGuard {
        session: SessionId::new("manual-cookie")?,
        epoch: 0,
        document: page.observation.document,
        expected_page: PageKind::Booking,
    };
    if cookies::accept(
        client.bidi.as_mut().ok_or("missing BiDi")?,
        &mut observer,
        &gate,
        &guard,
    ) != Err(BrowserError::Unsupported)
    {
        return Err("manual banner mutated".into());
    }
    let result=client.call("script.evaluate",json!({"expression":"!window.cookieClicks","target":{"context":context},"awaitPromise":false}))?;
    if result["result"]["value"] != true {
        return Err("manual cookie click occurred".into());
    }
    if fixture.posts != 0 {
        return Err("cookie action submitted a form".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"trusted_cookie_click_once":true,"core_form_continuation":true,"manual_booking_untouched":true,"no_live_site_requests":true,"post_requests":fixture.posts})
        )?
    );
    Ok(())
}
