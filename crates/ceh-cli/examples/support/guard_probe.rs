//! Local fixture acceptance for realm-bound commands, never a live-site action map.
use super::{Client, Result, Server};
use ceh_browser::{BrowserError, DispatchGate, DispatchOutcome};
use ceh_core::{control::Authority, model::*};
use serde_json::json;
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

fn realm(client: &mut Client, context: &str) -> Result<String> {
    client.call(
        "script.evaluate",
        json!({"expression":"document.title", "target":{"context":context},"awaitPromise":false}),
    )?["realm"]
        .as_str()
        .map(str::to_owned)
        .ok_or("missing realm".into())
}
fn params(realm: &str) -> serde_json::Value {
    json!({"target":{"realm":realm}, "awaitPromise":false,
        "functionDeclaration":"() => { const form=document.querySelector('form'); const button=document.getElementById('submit'); if(!form || !button || form.method!=='post' || new URL(form.action).pathname!=='/submit') return false; button.click(); return true; }"})
}
pub fn run(client: &mut Client, context: &str, url: &str, server: &Server) -> Result<()> {
    client.call(
        "browsingContext.navigate",
        json!({"context":context,"url":format!("{url}applicant"),"wait":"complete"}),
    )?;
    client.call("script.evaluate",json!({"expression":"document.getElementById('txtIdCitado').value='local-fixture-private'; document.body.style.minHeight='2000px'; window.scrollTo(0,200); document.getElementById('txtIdCitado').focus({preventScroll:true}); true", "target":{"context":context},"awaitPromise":false}))?;
    let mut document = ceh_browser::dom::read_document(
        client.bidi.as_mut().ok_or("missing owned BiDi")?,
        context,
        "manual",
    )
    .map_err(|_| "DOM inspection failed")?;
    let recognized =
        ceh_core::recognition::recognize(&document.snapshot).map_err(|_| "invalid DOM facts")?;
    let unchanged=client.call("script.evaluate",json!({"expression":"document.getElementById('txtIdCitado').value==='local-fixture-private' && document.activeElement.id==='txtIdCitado' && window.scrollY===200", "target":{"context":context},"awaitPromise":false}))?["result"]["value"]==true;
    let dom_readonly = recognized.kind == PageKind::ApplicantNoSlots
        && unchanged
        && !document.snapshot.frames[0]
            .text
            .contains("local-fixture-private");
    if !dom_readonly {
        return Err("read-only DOM contract failed".into());
    }
    let tree = client.call("browsingContext.getTree", json!({"root":context}))?;
    let child = tree["contexts"][0]["children"][0]["context"]
        .as_str()
        .ok_or("missing child frame")?
        .to_owned();
    let visible = ceh_browser::dom::frame_visible(
        client.bidi.as_mut().ok_or("missing BiDi")?,
        context,
        &document.realm,
        &child,
    )
    .map_err(|_| "visible frame probe failed")?;
    client.call("script.evaluate",json!({"expression":"document.querySelector('iframe').style.display='none'", "target":{"realm":document.realm.as_str()},"awaitPromise":false}))?;
    let hidden = !ceh_browser::dom::frame_visible(
        client.bidi.as_mut().ok_or("missing BiDi")?,
        context,
        &document.realm,
        &child,
    )
    .map_err(|_| "hidden frame probe failed")?;
    client.call("script.evaluate",json!({"expression":"document.querySelector('iframe').style.display=''", "target":{"realm":document.realm.as_str()},"awaitPromise":false}))?;
    client.call(
        "browsingContext.navigate",
        json!({"context":child,"url":format!("{url}expired"),"wait":"complete"}),
    )?;
    let charset=client.call("script.evaluate",json!({"expression":"document.characterSet", "target":{"context":child},"awaitPromise":false}))?;
    if charset["result"]["value"] != "UTF-8" {
        return Err("fixture response did not declare UTF-8".into());
    }
    let nested = ceh_browser::dom::read_document(
        client.bidi.as_mut().ok_or("missing BiDi")?,
        &child,
        "manual",
    )
    .map_err(|_| "child DOM inspection failed")?;
    document.snapshot.frames.extend(nested.snapshot.frames);
    let mut observer = ceh_browser::observer::PageObserver::new(context)
        .map_err(|_| "invalid observer context")?;
    for event in &client.events {
        observer
            .ingest(event)
            .map_err(|_| "invalid observer event")?;
    }
    let complete = observer
        .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
        .map_err(|_| "native observer failed")?;
    let complete_kind = &complete.recognition;
    if complete.page.sources.len() != 2
        || complete.observation.kind != PageKind::Expired
        || complete.observation.method != RequestMethod::Get
        || complete.page.sources[complete_kind.source_frame].context != child
    {
        return Err("native observer frame/source/method contract failed".into());
    }
    client.call("script.evaluate",json!({"expression":"document.querySelector('iframe').style.display='none'", "target":{"realm":document.realm.as_str()},"awaitPromise":false}))?;
    let hidden_page = observer
        .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
        .map_err(|_| "hidden iframe observer failed")?;
    if hidden_page.page.sources.len() != 1
        || hidden_page.observation.kind != PageKind::ApplicantNoSlots
    {
        return Err("hidden iframe incorrectly overrode applicant".into());
    }
    let nested_kind = ceh_core::recognition::recognize(&document.snapshot)
        .map_err(|_| "invalid nested facts")?
        .kind;
    let iframe_priority = visible && hidden && nested_kind == PageKind::Expired;
    if !iframe_priority {
        return Err(format!("visible iframe precedence failed: visible={visible}, hidden={hidden}, kind={nested_kind:?}").into());
    }
    client.call(
        "browsingContext.navigate",
        json!({"context":context,"url":url,"wait":"complete"}),
    )?;
    let old = realm(client, context)?;
    client.call(
        "browsingContext.navigate",
        json!({"context":context,"url":url,"wait":"complete"}),
    )?;
    let current = realm(client, context)?;
    if old == current {
        return Err("navigation reused document realm".into());
    }
    // Old-realm commands must fail without acting on the replacement document.
    let stale_rejected = client.call("script.callFunction", params(&old)).is_err();
    if !stale_rejected || server.posts.load(Ordering::SeqCst) != 0 {
        return Err("stale realm reached replacement document".into());
    }
    client.call("script.evaluate",json!({"expression":"document.getElementById('submit').remove()", "target":{"realm":current},"awaitPromise":false}))?;
    let changed = client.call("script.callFunction", params(&current))?;
    let stage_rejected = changed["result"]["value"] == false;
    if !stage_rejected || server.posts.load(Ordering::SeqCst) != 0 {
        return Err("changed stage submitted".into());
    }
    client.call(
        "browsingContext.navigate",
        json!({"context":context,"url":url,"wait":"complete"}),
    )?;
    let current = realm(client, context)?;
    let session = SessionId::new("native-guard-fixture")?;
    let gate = DispatchGate::new(session.clone());
    let controller = gate.clone();
    let mut authority = Authority::new(session.clone());
    let mut guard = DocumentGuard {
        session,
        epoch: 0,
        document: DocumentId::new(&current)?,
        expected_page: PageKind::Applicant,
    };
    let observation = Observation {
        document: guard.document.clone(),
        kind: PageKind::Applicant,
        method: RequestMethod::Get,
        operation_code: false,
        variant: String::new(),
    };
    gate.apply_control(&authority.pause().map_err(|_| "pause failed")?)
        .map_err(|_| "control rejected")?;
    let paused = gate.dispatch(&guard, &observation, ActionKind::Applicant, || {
        panic!("paused command reached socket")
    });
    if paused != Err(BrowserError::Paused) {
        return Err("pause failed to revoke".into());
    }
    guard.epoch = authority.resume().map_err(|_| "resume failed")?.epoch();
    gate.apply_control(&authority.snapshot())
        .map_err(|_| "control rejected")?;
    // Only write under the gate. The browser's potentially slow reply is deliberately not read.
    let sent = gate
        .dispatch(&guard, &observation, ActionKind::Applicant, || {
            let _ = client.send("script.callFunction", params(&current));
            DispatchOutcome::Unknown
        })
        .map_err(|_| "dispatch rejected")?;
    let start = Instant::now();
    controller
        .apply_control(&authority.pause().map_err(|_| "second pause failed")?)
        .map_err(|_| "control rejected")?;
    let control_millis = start.elapsed().as_millis();
    gate.apply_control(&authority.resume().map_err(|_| "second resume failed")?)
        .map_err(|_| "control rejected")?;
    guard.epoch = authority.snapshot().epoch();
    let duplicate = gate.dispatch(&guard, &observation, ActionKind::Applicant, || {
        panic!("ambiguous command retried")
    });
    let _ = client.receive();
    let deadline = Instant::now() + Duration::from_secs(4);
    while server.posts.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(400));
    client.call("browsingContext.getTree", json!({}))?;
    let mut facts = ceh_browser::navigation::NavigationFacts::default();
    let mut navigation = None;
    for event in &client.events {
        facts
            .ingest(event)
            .map_err(|_| "invalid navigation facts")?;
        if event["method"] == "browsingContext.navigationStarted"
            && event["params"]["context"] == context
        {
            navigation = event["params"]["navigation"].as_str();
        }
    }
    let actual = facts.get(context, navigation.ok_or("missing actual navigation")?);
    let http_facts = actual.method == RequestMethod::Post && actual.status == Some(200);
    if !http_facts {
        return Err("actual POST/HTTP facts not observed".into());
    }
    if sent != DispatchOutcome::Unknown
        || duplicate != Err(BrowserError::DuplicateCommand)
        || server.posts.load(Ordering::SeqCst) != 1
        || control_millis > 100
    {
        return Err("dispatch ownership contract failed".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"protocol":"bidi","localhost_only":true,"stale_realm_rejected":stale_rejected,"same_document_stage_rejected":stage_rejected,"pause_before_send_revoked":true,"pause_applied_before_reply":true,"control_millis":control_millis,"lost_ack_duplicate_rejected":true,"post_requests_received":server.posts.load(Ordering::SeqCst),"actual_post_and_200_navigation":http_facts,"dom_recognition_and_input_focus_scroll_preserved":dom_readonly,"iframe_visibility_and_expired_priority":iframe_priority,"not_verified":["production action map","native authorization","manual background lock behavior"]})
        )?
    );
    Ok(())
}
