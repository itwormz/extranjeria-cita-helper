//! Actual GET/POST cardinality on intercepted synthetic pages only.
use super::{
    Client, Result,
    portal_fixture::{Fixture, seed_observer},
};
use ceh_browser::{
    DispatchGate,
    observer::PageObserver,
    refresh::{RefreshAction, execute_refresh, plan_refresh},
};
use ceh_core::{
    flow::{FlowEffect, FlowEngine, PageFacts},
    model::*,
    recovery::RecoveryEndpoint,
};
use serde_json::json;
const BOOT: &str = "/icpplus/rust-fixture-refresh-boot";
const APPLICANT: &str = "<body><p>No hay citas disponibles</p><input id='txtIdCitado'><button id='btnEnviar'>Aceptar</button></body>";
const READY: &str = "<body><input id='txtIdCitado'><button id='btnEnviar'>Aceptar</button></body>";
const EMPTY: &str = "<body>No hay citas disponibles</body>";
pub fn run(client: &mut Client, context: &str) -> Result<()> {
    client.call(
        "session.subscribe",
        json!({"events":["browsingContext.userPromptOpened"]}),
    )?;
    let mut fixture = Fixture::new(client, context, vec![])?;
    for (index, (path, method, requested)) in [
        (
            "/icpplus/acEntrada",
            RequestMethod::Get,
            RefreshAction::ReloadApplicant,
        ),
        (
            "/icpplus/acEntrada",
            RequestMethod::Post,
            RefreshAction::FetchApplicant,
        ),
        (
            "/icpplus/acCitar",
            RequestMethod::Post,
            RefreshAction::ReloadAvailability,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let body = if index == 2 { EMPTY } else { APPLICANT };
        fixture.routes = vec![
            ("GET", path, body.into()),
            ("POST", path, body.into()),
            (
                "GET",
                BOOT,
                format!(
                    "<body><form method='post' action='{path}'><input type='hidden' name='synthetic' value='not-user-data'><button id='go'>Go</button></form></body>"
                ),
            ),
        ];
        if method == RequestMethod::Post {
            fixture.navigate(client, context, BOOT)?;
            client.call("script.evaluate",json!({"expression":"document.getElementById('go').click();true","target":{"context":context},"awaitPromise":false}))?;
            fixture.wait_load(client, context, path)?;
        } else {
            fixture.navigate(client, context, path)?;
        }
        let mut observer = PageObserver::new(context).map_err(|_| "refresh observer invalid")?;
        let mut cursor = 0;
        seed_observer(client, &mut observer, &mut cursor)?;
        if index == 2 {
            observer.set_actions(Some(ActionKind::Request), None);
        }
        let page = observer
            .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
            .map_err(|_| "refresh observation failed")?;
        if page.observation.method != method
            || plan_refresh(&page.observation, &page.page.snapshot)
                .map_err(|_| "refresh plan refused")?
                != requested
        {
            return Err("refresh actual method/plan mismatch".into());
        }
        let session = SessionId::new(format!("refresh-fixture-{index}"))?;
        let gate = DispatchGate::new(session.clone());
        let mut flow = FlowEngine::new(
            session,
            SearchOptions {
                no_slots_mode: NoSlotsMode::Reload,
                ..SearchOptions::default()
            },
        );
        let mut facts = PageFacts::default();
        facts.recovery.official_site = true;
        facts.recovery.endpoint = if index == 2 {
            RecoveryEndpoint::Availability
        } else {
            RecoveryEndpoint::ApplicantEntry
        };
        if index == 2 {
            facts.recovery.last_action = Some(ActionKind::Request);
        }
        let current = page.observation.clone();
        let effects = flow
            .observe_with_facts(page.observation, facts, 0)
            .map_err(|_| "refresh core observe failed")?;
        let deadline = effects
            .iter()
            .find_map(|e| {
                if let FlowEffect::WaitUntil(value) = e {
                    Some(*value)
                } else {
                    None
                }
            })
            .ok_or("refresh core skipped cooldown")?;
        let effects = flow
            .observe_with_facts(current, facts, deadline)
            .map_err(|_| "refresh core cooldown failed")?;
        let guard = match effects.first() {
            Some(FlowEffect::Reload { guard }) if requested != RefreshAction::FetchApplicant => {
                guard
            }
            Some(FlowEffect::FetchApplicant { guard })
                if requested == RefreshAction::FetchApplicant =>
            {
                guard
            }
            _ => return Err("core did not issue matching refresh effect".into()),
        };
        fixture.routes = vec![("GET", path, READY.into()), ("POST", path, READY.into())];
        let before_get = fixture.entry;
        let before_post = fixture.posts;
        let attempt = execute_refresh(
            client.bidi.as_mut().ok_or("missing BiDi")?,
            &mut observer,
            &gate,
            guard,
            requested,
        );
        if index == 2 {
            if !matches!(attempt, Err(ceh_browser::BrowserError::Unsupported))
                || fixture.entry != before_get
                || fixture.posts != before_post
            {
                return Err("unverified availability POST refresh was published".into());
            }
            continue;
        }
        let sent = attempt.map_err(|e| format!("refresh dispatch failed: {e:?}"))?;
        flow.acknowledge_dispatch(guard, ActionKind::ApplicantReload, sent)
            .map_err(|_| "refresh dispatch ack failed")?;
        if sent == DispatchOutcome::NotDispatched {
            return Err("refresh not dispatched".into());
        }
        fixture.wait_load(client,context,path).map_err(|_|{
   let prompts:Vec<&str>=client.events.iter().filter(|e|e["method"]=="browsingContext.userPromptOpened").filter_map(|e|e["params"]["type"].as_str()).collect();
   format!("refresh case {index} did not finish; prompt types={prompts:?}; new GET={}, new POST={}",fixture.entry-before_get,fixture.posts-before_post)
  })?;
        seed_observer(client, &mut observer, &mut cursor)?;
        let result = observer
            .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
            .map_err(|_| "refreshed page observation failed")?;
        let expected = if index == 2 {
            RequestMethod::Post
        } else {
            RequestMethod::Get
        };
        if result.observation.method != expected || result.observation.kind != PageKind::Applicant {
            return Err("refreshed state/method mismatch".into());
        }
        if fixture.entry - before_get != usize::from(index != 2)
            || fixture.posts - before_post != usize::from(index == 2)
        {
            return Err("refresh repeated or used wrong method".into());
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"no_live_site_requests":true,"core_cooldown_preserved":true,"applicant_get_reloaded":true,"certificate_post_replaced_with_get":true,"availability_post_refresh_blocked_pending_backend":true,"get_requests":fixture.entry,"post_requests":fixture.posts,"clock_advanced_fixture_only":true})
        )?
    );
    Ok(())
}
