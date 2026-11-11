//! Synthetic trusted-input/noPaste contacts, offline at the official origin.
use super::{
    Client, Result,
    portal_fixture::{Fixture, seed_observer},
};
use ceh_browser::{
    BrowserError, DispatchGate,
    contact::{ContactInput, activate_contact, prepare_contact},
    observer::PageObserver,
};
use ceh_core::{
    control::Authority,
    flow::{FlowEffect, FlowEngine, PreparationOutcome},
    model::*,
    profiles::ProfileDraft,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
const ENTRY: &str = "/icpplus/rust-fixture-contact-entry";
const RESULT: &str = "/icpplus/rust-fixture-contact-result";
const PAGE: &str = "<body><input type='hidden' id='hidden-phone'><form method='post' action='/icpplus/rust-fixture-contact-result'><input id='txtTelefonoCitado' type='tel' name='phone'><input id='emailUNO' type='email' name='email1'><input id='emailDOS' type='email' name='email2'><button id='btnSiguiente' onclick='if(!event.isTrusted)event.preventDefault()'>Siguiente</button></form><script>for(const el of document.querySelectorAll('form input')){el.onpaste=event=>{window.pastes=(window.pastes||0)+1;event.preventDefault()};el.oninput=event=>{if(!event.isTrusted)el.value='';else window.trustedInputs=(window.trustedInputs||0)+1};}</script></body>";
fn profile() -> Result<ProfileDraft> {
    let cases: Value = serde_json::from_str(include_str!(
        "../../../../contracts/fixtures/profile-validation.json"
    ))?;
    let mut p: ProfileDraft =
        serde_json::from_value(cases["cases"][0]["expected"]["normalized"].clone())?;
    p.phone = "600123456".into();
    p.email = "alex@example.invalid".into();
    Ok(p)
}
pub fn run(client: &mut Client, context: &str) -> Result<()> {
    let mut fixture = Fixture::new(
        client,
        context,
        vec![
            ("GET", ENTRY, PAGE.into()),
            (
                "POST",
                RESULT,
                "<body>No hay citas disponibles</body>".into(),
            ),
        ],
    )?;
    fixture.navigate(client, context, ENTRY)?;
    let input = ContactInput::from_profile(&profile()?).map_err(|_| "contact profile invalid")?;
    let session = SessionId::new("contact-fixture")?;
    let gate = DispatchGate::new(session.clone());
    let mut flow = FlowEngine::new(session, SearchOptions::default());
    let mut observer = PageObserver::new(context).map_err(|_| "contact observer invalid")?;
    let mut cursor = 0;
    seed_observer(client, &mut observer, &mut cursor)?;
    let page = observer
        .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
        .map_err(|_| "contact observation failed")?;
    let effects = flow
        .observe(page.observation, 0)
        .map_err(|_| "contact core failed")?;
    let Some(FlowEffect::Prepare { guard, action: _ }) = effects.first() else {
        return Err("core did not prepare contacts".into());
    };
    let mut authority = Authority::new(guard.session.clone());
    gate.apply_control(&authority.pause().map_err(|_| "contact pause failed")?)
        .map_err(|_| "pause apply failed")?;
    if !matches!(
        prepare_contact(
            client.bidi.as_mut().ok_or("missing BiDi")?,
            &mut observer,
            &gate,
            guard,
            &input,
            Duration::ZERO,
            Instant::now() + Duration::from_secs(3)
        ),
        Err(BrowserError::Paused)
    ) {
        return Err("paused contact typing was not blocked".into());
    }
    gate.apply_control(&authority.resume().map_err(|_| "contact resume failed")?)
        .map_err(|_| "resume apply failed")?;
    // The production core supplies the new epoch after resume. A fresh local core is
    // unnecessary here: this separate probe only proves no key was published.
    let session = SessionId::new("contact-active-fixture")?;
    let gate = DispatchGate::new(session.clone());
    let mut flow = FlowEngine::new(session, SearchOptions::default());
    let page = observer
        .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
        .map_err(|_| "active contact observation failed")?;
    let effects = flow
        .observe(page.observation, 0)
        .map_err(|_| "active contact core failed")?;
    let Some(FlowEffect::Prepare { guard, action }) = effects.first() else {
        return Err("core did not prepare active contacts".into());
    };
    let prepared = prepare_contact(
        client.bidi.as_mut().ok_or("missing BiDi")?,
        &mut observer,
        &gate,
        guard,
        &input,
        Duration::from_millis(1),
        Instant::now() + Duration::from_secs(15),
    )
    .map_err(|e| format!("contact preparation failed: {e:?}"))?;
    if prepared != PreparationOutcome::Completed {
        return Err("contacts not prepared".into());
    }
    let typed=client.call("script.evaluate",json!({"expression":"!window.pastes && window.trustedInputs===49 && document.getElementById('txtTelefonoCitado').value==='600123456' && document.getElementById('emailUNO').value==='alex@example.invalid' && document.getElementById('emailDOS').value==='alex@example.invalid' && document.getElementById('hidden-phone').value===''","target":{"context":context},"awaitPromise":false}))?["result"]["value"]==true;
    if !typed {
        return Err("contacts used paste, untrusted events or wrong fields".into());
    }
    let again = prepare_contact(
        client.bidi.as_mut().ok_or("missing BiDi")?,
        &mut observer,
        &gate,
        guard,
        &input,
        Duration::ZERO,
        Instant::now() + Duration::from_secs(3),
    )
    .map_err(|_| "contact recheck failed")?;
    if again != PreparationOutcome::Completed {
        return Err("contact recheck incomplete".into());
    }
    let untouched=client.call("script.evaluate",json!({"expression":"window.trustedInputs===49","target":{"context":context},"awaitPromise":false}))?["result"]["value"]==true;
    if !untouched {
        return Err("prepared contacts typed again".into());
    }
    flow.complete_preparation(guard, *action, prepared)
        .map_err(|_| "contact core prepare ack failed")?;
    let page = observer
        .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
        .map_err(|_| "prepared contact observation failed")?;
    let effects = flow
        .observe(page.observation, 1)
        .map_err(|_| "prepared contact core failed")?;
    let Some(FlowEffect::Activate { guard, action }) = effects.first() else {
        return Err("core did not activate contacts".into());
    };
    observer.set_actions(None, Some(ActionKind::Contact));
    let sent = activate_contact(
        client.bidi.as_mut().ok_or("missing BiDi")?,
        &mut observer,
        &gate,
        guard,
        &input,
    )
    .map_err(|e| format!("contact activation failed: {e:?}"))?;
    if sent != DispatchOutcome::Dispatched {
        return Err("contacts not submitted".into());
    }
    flow.acknowledge_dispatch(guard, *action, sent)
        .map_err(|_| "contact dispatch ack failed")?;
    fixture.wait_load(client, context, RESULT)?;
    seed_observer(client, &mut observer, &mut cursor)?;
    let result = observer
        .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
        .map_err(|_| "contact result read failed")?;
    if result.observation.kind != PageKind::PostContactNoSlots || fixture.posts != 1 {
        return Err("contact result not contextualized or repeated".into());
    }
    // Refuse a short field without truncating, typing elsewhere or submitting.
    fixture.routes = vec![
        (
            "GET",
            ENTRY,
            PAGE.replace(
                "id='txtTelefonoCitado'",
                "id='txtTelefonoCitado' maxlength='3'",
            ),
        ),
        (
            "POST",
            RESULT,
            "<body>No hay citas disponibles</body>".into(),
        ),
    ];
    fixture.navigate(client, context, ENTRY)?;
    observer.set_actions(None, None);
    seed_observer(client, &mut observer, &mut cursor)?;
    let page = observer
        .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
        .map_err(|_| "short contact observation failed")?;
    let session = SessionId::new("short-contact-fixture")?;
    let gate = DispatchGate::new(session.clone());
    let guard = DocumentGuard {
        session,
        epoch: 0,
        document: page.observation.document,
        expected_page: PageKind::Contact,
    };
    let prepared = prepare_contact(
        client.bidi.as_mut().ok_or("missing BiDi")?,
        &mut observer,
        &gate,
        &guard,
        &input,
        Duration::ZERO,
        Instant::now() + Duration::from_secs(2),
    )
    .map_err(|_| "short contact probe failed")?;
    if prepared != PreparationOutcome::Failed {
        return Err("short contact field accepted".into());
    }
    let empty=client.call("script.evaluate",json!({"expression":"document.getElementById('txtTelefonoCitado').value==='' && document.getElementById('emailUNO').value==='' && document.getElementById('emailDOS').value==='' && !window.trustedInputs","target":{"context":context},"awaitPromise":false}))?["result"]["value"]==true;
    if !empty {
        return Err("short contact value truncated or typed".into());
    }
    if activate_contact(
        client.bidi.as_mut().ok_or("missing BiDi")?,
        &mut observer,
        &gate,
        &guard,
        &input,
    )
    .map_err(|_| "short contact activation failed")?
        != DispatchOutcome::NotDispatched
        || fixture.posts != 1
    {
        return Err("incomplete contacts submitted".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"no_live_site_requests":true,"trusted_inputs":49,"no_paste":true,"hidden_leftovers_ignored":true,"already_prepared_not_retyped":true,"paused_typing_refused":true,"post_requests":fixture.posts,"post_contact_no_slots_recognized":true,"native_background_lock_not_claimed":true,"maxlength_not_truncated_or_submitted":true})
        )?
    );
    Ok(())
}
