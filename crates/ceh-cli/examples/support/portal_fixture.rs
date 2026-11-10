//! Synthetic portal responses at the exact official origin. Every tab request is
//! intercepted before networking; the isolated browser also has a dead external proxy.
use super::{Client, Result};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};
pub(super) const ORIGIN: &str = "https://icp.administracionelectronica.gob.es";
fn fixture_url(path: &str) -> String {
    if path.starts_with("https://") {
        path.to_owned()
    } else {
        format!("{ORIGIN}{path}")
    }
}
const ENTRY: &str = "/icpplus/rust-fixture-province";
const NEXT: &str = "/icpplus/rust-fixture-procedure";
const FINISH: &str = "/icpplus/rust-fixture-contact";
const OFFICE: &str = "<body style='min-height:2000px'><input id='manualField' value='synthetic'><form method='post' action='/icpplus/rust-fixture-contact'><select id='idSede' name='idSede' onchange='window.selectionChanges=(window.selectionChanges||0)+1;window.selectionTrusted=event.isTrusted'><option value='0'>Seleccionar</option><option disabled value='disabled'>Unavailable office</option><optgroup disabled><option value='disabled-group'>Unavailable group</option></optgroup><option value='one'>CNP COMISARIA TEST, CALLE EJEMPLO 1</option></select><button id='btnSiguiente' onclick='if(!event.isTrusted)event.preventDefault()'>Siguiente</button></form></body>";
const COMPLETE: &str = "/icpplus/rust-fixture-complete";
const PROVINCE: &str = "<body style='min-height:2000px'><input id='manualField' value='synthetic'><form method='post' action='/icpplus/rust-fixture-procedure'><select id='form' name='p' onchange='window.selectionChanges=(window.selectionChanges||0)+1;window.selectionTrusted=event.isTrusted'><option value=''>Seleccionar</option><option value='other'>Other province</option><option disabled value='unavailable'>Unavailable province</option><optgroup disabled><option value='group-disabled'>Unavailable group province</option></optgroup><option value='08'>Barcelona</option></select><button id='btnAceptar' onclick='if(!event.isTrusted)event.preventDefault()'>Aceptar</button></form></body>";
const PROCEDURE: &str = "<body style='min-height:2000px'><input id='manualField' value='synthetic'><form method='post' action='/icpplus/rust-fixture-complete'><select id='extra'><option value='keep'>Ignore</option></select><select id='tramiteGrupo[0]'><option value='other'>Different procedure family</option></select><select id='tramiteGrupo[3]' onchange='window.selectionChanges=(window.selectionChanges||0)+1;window.selectionTrusted=event.isTrusted'><option>Seleccionar</option></select><script>const delayed=document.getElementById('tramiteGrupo[3]');const originalOptions=delayed.options;Object.defineProperty(delayed,'options',{configurable:true,get(){delete delayed.options;window.fixtureDelayedOption=true;setTimeout(()=>delayed.add(new Option('AUTORIZACIÓN DE REGRESO','regreso')),400);return originalOptions;}});</script><button id='btnAceptar' onclick='if(!event.isTrusted)event.preventDefault()'>Aceptar</button></form></body>";
const CONTACT: &str = "<body><input id='txtTelefonoCitado'><input id='emailUNO'><input id='emailDOS'><button id='btnSiguiente' onclick='if(!event.isTrusted)event.preventDefault()'>Siguiente</button></body>";
pub(super) struct Fixture {
    cursor: usize,
    responded: HashSet<String>,
    pub(super) posts: usize,
    pub(super) entry: usize,
    pub(super) routes: Vec<(&'static str, &'static str, String)>,
}
impl Fixture {
    pub(super) fn new(
        client: &mut Client,
        context: &str,
        routes: Vec<(&'static str, &'static str, String)>,
    ) -> Result<Self> {
        client.call(
            "network.addIntercept",
            json!({"phases":["beforeRequestSent"],"contexts":[context]}),
        )?;
        Ok(Self {
            cursor: client.events.len(),
            responded: HashSet::new(),
            posts: 0,
            entry: 0,
            routes,
        })
    }
    pub(super) fn navigate(
        &mut self,
        client: &mut Client,
        context: &str,
        path: &str,
    ) -> Result<()> {
        client.call("script.evaluate",json!({"expression":format!("setTimeout(()=>location.href={},0);true",serde_json::to_string(&fixture_url(path))?),"target":{"context":context},"awaitPromise":false}))?;
        self.wait_load(client, context, path)
    }
    pub(super) fn pump(&mut self, client: &mut Client) -> Result<()> {
        client.call("session.status", json!({}))?;
        let events: Vec<Value> = client.events[self.cursor..].to_vec();
        self.cursor = client.events.len();
        for event in events {
            if event["method"] != "network.beforeRequestSent"
                || event["params"]["isBlocked"] != true
            {
                continue;
            }
            let params = &event["params"];
            let request = params["request"]["request"]
                .as_str()
                .ok_or("missing blocked request")?;
            if !self.responded.insert(request.to_owned()) {
                continue;
            }
            let url = params["request"]["url"]
                .as_str()
                .ok_or("missing fixture URL")?;
            let method = params["request"]["method"]
                .as_str()
                .ok_or("missing request method")?;
            let (body, status) = if let Some((_, _, body)) = self
                .routes
                .iter()
                .find(|(expected, path, _)| method == *expected && url == fixture_url(path))
            {
                if method == "POST" {
                    self.posts += 1;
                } else if method == "GET" {
                    self.entry += 1;
                }
                (body.as_str(), 200)
            } else {
                ("<body>Offline fixture: request rejected</body>", 404)
            };
            let mime = if url.ends_with(".js") {
                "application/javascript; charset=utf-8"
            } else {
                "text/html; charset=utf-8"
            };
            client.call("network.provideResponse",json!({"request":request,"statusCode":status,"body":{"type":"string","value":body},"headers":[{"name":"Content-Type","value":{"type":"string","value":mime}},{"name":"Cache-Control","value":{"type":"string","value":"no-store"}}]}))?;
        }
        Ok(())
    }
    pub(super) fn wait_load(
        &mut self,
        client: &mut Client,
        context: &str,
        path: &str,
    ) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(8);
        let start = client.events.len();
        while Instant::now() < deadline {
            self.pump(client)?;
            if client.events[start..].iter().any(|event| {
                event["method"] == "browsingContext.load"
                    && event["params"]["context"] == context
                    && event["params"]["url"] == fixture_url(path)
            }) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Err("intercepted fixture did not load".into())
    }
}
pub(super) fn seed_observer(
    client: &Client,
    observer: &mut ceh_browser::observer::PageObserver,
    cursor: &mut usize,
) -> Result<()> {
    for event in &client.events[*cursor..] {
        observer
            .ingest(event)
            .map_err(|_| "invalid fixture navigation event")?;
    }
    *cursor = client.events.len();
    Ok(())
}
fn focus_fixture(client: &mut Client, context: &str) -> Result<()> {
    client.call("script.evaluate",json!({"expression":"document.getElementById('manualField').focus({preventScroll:true});window.scrollTo(0,200);true","target":{"context":context},"awaitPromise":false}))?;
    Ok(())
}
fn refused_browser_command(client: &mut Client, context: &str, stale: bool) -> Result<()> {
    use ceh_browser::dom::{INSPECT_DOCUMENT, read_page};
    let page = read_page(
        client.bidi.as_mut().ok_or("missing BiDi")?,
        context,
        "manual",
    )
    .map_err(|_| "guard proof read failed")?;
    let issued = if stale {
        0
    } else {
        u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis(),
        )?
    };
    let payload = json!({"submit":true,"province":true,"labels":["Barcelona"],"expectedText":page.snapshot.frames[0].text,"expectedVisible":page.snapshot.visible,"expectedEndpoint":page.snapshot.endpoint,"issuedAt":issued});
    if !stale {
        client.call("script.evaluate",json!({"expression":"const probe=document.createElement('input');probe.id='captcha';document.body.append(probe);true","target":{"realm":page.sources[0].realm.as_str()},"awaitPromise":false}))?;
    }
    let script = include_str!("../../../ceh-browser/src/selection-step.js");
    let validate = include_str!("../../../ceh-browser/src/mutation-guard.js");
    let declaration = format!(
        "raw => {{const inspect={INSPECT_DOCUMENT};const validate={validate};const step={script};return validate(raw,inspect,step);}}"
    );
    let result=client.call("script.callFunction",json!({"functionDeclaration":declaration,"target":{"realm":page.sources[0].realm.as_str()},"awaitPromise":false,"arguments":[{"type":"string","value":payload.to_string()}]}))?;
    if result["result"]["value"] != "refused" {
        return Err("expired/changed browser command was not refused".into());
    }
    if !stale {
        client.call("script.evaluate",json!({"expression":"document.getElementById('captcha').remove();true","target":{"realm":page.sources[0].realm.as_str()},"awaitPromise":false}))?;
    }
    Ok(())
}
pub fn run(client: &mut Client, context: &str) -> Result<()> {
    use ceh_browser::{
        DispatchGate,
        observer::PageObserver,
        selection::{Selection, activate_selection, prepare_selection},
    };
    use ceh_core::{
        flow::{FlowEffect, FlowEngine, PageFacts, PreparationOutcome},
        model::{DispatchOutcome, PageKind, SearchOptions, SessionId},
    };
    let mut fixture = Fixture::new(
        client,
        context,
        vec![
            ("GET", ENTRY, PROVINCE.to_owned()),
            ("POST", NEXT, PROCEDURE.to_owned()),
            ("POST", COMPLETE, OFFICE.to_owned()),
            ("POST", FINISH, CONTACT.to_owned()),
        ],
    )?;
    fixture.navigate(client, context, ENTRY)?;
    let session = SessionId::new("native-selection-fixture")?;
    let gate = DispatchGate::new(session.clone());
    let mut flow = FlowEngine::new(session, SearchOptions::default());
    let mut observer = PageObserver::new(context).map_err(|_| "invalid observer")?;
    let mut cursor = 0;
    for (index, (selection, next)) in [
        (
            Selection::province("Barcelona").map_err(|_| "invalid province")?,
            NEXT,
        ),
        (
            Selection::procedure("regreso").map_err(|_| "invalid service")?,
            COMPLETE,
        ),
        (Selection::single_office(), FINISH),
    ]
    .into_iter()
    .enumerate()
    {
        focus_fixture(client, context)?;
        seed_observer(client, &mut observer, &mut cursor)?;
        let page = observer
            .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
            .map_err(|_| "native observation failed")?;
        if !page.page.snapshot.official {
            return Err("fixture lost production origin guard".into());
        }
        let effects = flow
            .observe_with_facts(
                page.observation,
                PageFacts {
                    office_count: usize::from(index == 2),
                    ..PageFacts::default()
                },
                index as u64 * 2,
            )
            .map_err(|_| "core observation failed")?;
        let Some(FlowEffect::Prepare { guard, action }) = effects.first() else {
            return Err("core did not request preparation".into());
        };
        if index == 0 {
            use ceh_core::{control::Authority, model::DocumentGuard};
            let probe_session = SessionId::new("selection-guard-probe")?;
            let probe_gate = DispatchGate::new(probe_session.clone());
            let mut authority = Authority::new(probe_session.clone());
            let probe_guard = DocumentGuard {
                session: probe_session,
                epoch: 0,
                document: guard.document.clone(),
                expected_page: guard.expected_page,
            };
            probe_gate
                .apply_control(&authority.pause().map_err(|_| "probe pause failed")?)
                .map_err(|_| "probe control failed")?;
            if !matches!(
                prepare_selection(
                    client.bidi.as_mut().ok_or("missing BiDi")?,
                    &mut observer,
                    &probe_gate,
                    &probe_guard,
                    &selection,
                    Instant::now() + Duration::from_secs(3)
                ),
                Err(ceh_browser::BrowserError::Paused)
            ) {
                return Err("paused preparation was not blocked".into());
            }
            probe_gate
                .apply_control(&authority.handoff().map_err(|_| "probe handoff failed")?)
                .map_err(|_| "probe manual failed")?;
            if !matches!(
                activate_selection(
                    client.bidi.as_mut().ok_or("missing BiDi")?,
                    &mut observer,
                    &probe_gate,
                    &probe_guard,
                    &selection
                ),
                Err(ceh_browser::BrowserError::ManualOwnership)
            ) {
                return Err("manual activation was not blocked".into());
            }
            let missing = Selection::province("Missing fixture province")
                .map_err(|_| "missing choice invalid")?;
            if prepare_selection(
                client.bidi.as_mut().ok_or("missing BiDi")?,
                &mut observer,
                &gate,
                guard,
                &missing,
                Instant::now() + Duration::from_millis(100),
            )
            .map_err(|_| "missing choice execution failed")?
                != PreparationOutcome::Failed
            {
                return Err("missing choice was not refused".into());
            }
        }
        if index == 2 {
            client.call("script.evaluate",json!({"expression":"document.getElementById('idSede').add(new Option('SECOND OFFICE','two'));true","target":{"context":context},"awaitPromise":false}))?;
            let denied = prepare_selection(
                client.bidi.as_mut().ok_or("missing BiDi")?,
                &mut observer,
                &gate,
                guard,
                &selection,
                Instant::now() + Duration::from_millis(100),
            )
            .map_err(|_| "multiple-office probe failed")?;
            if denied != PreparationOutcome::Failed {
                return Err("multiple offices were auto-selected".into());
            }
            let untouched=client.call("script.evaluate",json!({"expression":"document.getElementById('idSede').value==='0'","target":{"context":context},"awaitPromise":false}))?["result"]["value"]==true;
            if !untouched {
                return Err("multiple-office probe mutated selection".into());
            }
            client.call("script.evaluate",json!({"expression":"document.querySelector('#idSede option[value=two]').remove();true","target":{"context":context},"awaitPromise":false}))?;
        }
        client.call("script.evaluate",json!({"expression":"window.fixtureKeys=[]; document.addEventListener('keydown',e=>window.fixtureKeys.push([e.key,e.target.id,e.isTrusted]),true); true","target":{"context":context},"awaitPromise":false}))?;
        let prepared = prepare_selection(
            client.bidi.as_mut().ok_or("missing BiDi")?,
            &mut observer,
            &gate,
            guard,
            &selection,
            Instant::now() + Duration::from_secs(3),
        );
        let prepared=match prepared{Ok(value)=>value,Err(error)=>{
            let probe=client.call("script.evaluate",json!({"expression":"JSON.stringify({focus:document.activeElement.id,index:document.querySelector('select').selectedIndex,open:document.querySelector('select').matches(':open'),keys:window.fixtureKeys,changes:window.selectionChanges||0,trusted:window.selectionTrusted===true})","target":{"context":context},"awaitPromise":false}))?;
            return Err(format!("native preparation failed at {index}: {error:?}, synthetic fixture state: {}",probe["result"]["value"]).into());
        }};
        if prepared != PreparationOutcome::Completed {
            let probe=client.call("script.evaluate",json!({"expression":"JSON.stringify({focus:document.activeElement.id,index:document.querySelector('select').selectedIndex,keys:window.fixtureKeys,changes:window.selectionChanges||0,trusted:window.selectionTrusted===true})","target":{"context":context},"awaitPromise":false}))?;
            return Err(format!("native preparation not completed at {index}: {prepared:?}, synthetic fixture state: {}",probe["result"]["value"]).into());
        }
        let repeated = prepare_selection(
            client.bidi.as_mut().ok_or("missing BiDi")?,
            &mut observer,
            &gate,
            guard,
            &selection,
            Instant::now() + Duration::from_secs(3),
        )
        .map_err(|_| "selection recheck failed")?;
        if repeated != PreparationOutcome::Completed {
            return Err("prepared selection not reusable".into());
        }
        let once=client.call("script.evaluate",json!({"expression":"window.selectionChanges===1","target":{"context":context},"awaitPromise":false}))?["result"]["value"]==true;
        if !once {
            return Err("prepared selection fired change again".into());
        }
        // Python parity: the select is chosen by one script-dispatched input + change (not trusted, no keys).
        let untrusted=client.call("script.evaluate",json!({"expression":"window.selectionTrusted===false && window.fixtureKeys.length===0","target":{"context":context},"awaitPromise":false}))?["result"]["value"]==true;
        if !untrusted{return Err("selection change was not a single script-dispatched event without key input".into());}
        client.call("session.status",json!({}))?;
        let posts=client.events.iter().filter(|event|event["method"]=="network.beforeRequestSent"&&event["params"]["request"]["method"]=="POST").count();
        if posts!=index{return Err("selection preparation unexpectedly submitted the form".into());}

        flow.complete_preparation(guard, *action, prepared)
            .map_err(|_| "core preparation acknowledgement failed")?;
        let kept=client.call("script.evaluate",json!({"expression":"document.activeElement.id==='manualField' && document.getElementById('manualField').value==='synthetic' && window.scrollY===200","target":{"context":context},"awaitPromise":false}))?["result"]["value"]==true;
        if !kept {
            return Err("selection moved focus/input/scroll".into());
        }
        if index == 1 {
            let ignored=client.call("script.evaluate",json!({"expression":"document.getElementById('extra').value==='keep' && document.getElementById('tramiteGrupo[0]').value==='other' && document.getElementById('tramiteGrupo[3]').value==='regreso' && window.fixtureDelayedOption===true","target":{"context":context},"awaitPromise":false}))?["result"]["value"]==true;
            if !ignored {
                return Err("unrelated province select was modified".into());
            }
        }
        if index == 0 {
            refused_browser_command(client, context, true)?;
            refused_browser_command(client, context, false)?;
        }
        let page = observer
            .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
            .map_err(|_| "prepared observation failed")?;
        let effects = flow
            .observe_with_facts(
                page.observation,
                PageFacts {
                    office_count: usize::from(index == 2),
                    ..PageFacts::default()
                },
                index as u64 * 2 + 1,
            )
            .map_err(|_| "prepared core observation failed")?;
        let Some(FlowEffect::Activate { guard, action }) = effects.first() else {
            return Err("core did not request activation".into());
        };
        let sent = activate_selection(
            client.bidi.as_mut().ok_or("missing BiDi")?,
            &mut observer,
            &gate,
            guard,
            &selection,
        )
        .map_err(|error| format!("native activation failed: {error:?}"))?;
        if sent != DispatchOutcome::Dispatched {
            return Err("native activation not acknowledged".into());
        }
        flow.acknowledge_dispatch(guard, *action, sent)
            .map_err(|_| "core dispatch acknowledgement failed")?;
        fixture.wait_load(client, context, next)?;
    }
    seed_observer(client, &mut observer, &mut cursor)?;
    let final_page = observer
        .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
        .map_err(|_| "final observation failed")?;
    if final_page.observation.kind != PageKind::Contact || fixture.entry != 1 || fixture.posts != 3
    {
        return Err("fixture final state/cardinality failed".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"official_origin_fixture":true,"requests_fulfilled_by_bidi":true,"external_proxy_blocked":true,"get_requests":fixture.entry,"post_requests":fixture.posts,"no_live_site_requests":true,"core_prepare_activate_native":true,"single_office_selected_multiple_refused":true,"selected_option_not_changed_again":true,"extra_province_fields_ignored":true,"input_focus_scroll_preserved":true,"final_contact_page_recognized":true,"delayed_option_readiness":true,"paused_manual_missing_choice_blocked":true,"expired_command_and_changed_dom_refused":true})
        )?
    );
    Ok(())
}
