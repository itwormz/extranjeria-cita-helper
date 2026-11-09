//! Capability experiment: retrieve already loaded response bodies, never fetch again.
use super::{
    Client, Result,
    portal_fixture::{Fixture, ORIGIN},
};
use serde_json::json;
const ENTRY: &str = "/icpplus/rust-fixture-passive";
const SCRIPT: &str = "/icpplus/rust-fixture-passive.js";
const HTML: &str = "<body style='min-height:2000px'><input id='manualField' value='synthetic'><script src='/icpplus/rust-fixture-passive.js'></script></body>";
const JS: &str = "window.syntheticPassiveScript=true;";
pub fn run(client: &mut Client, context: &str) -> Result<()> {
    let collector=match client.call("network.addDataCollector",json!({"dataTypes":["response"],"maxEncodedDataSize":131072,"collectorType":"blob","contexts":[context]})){
  Ok(result)=>result["collector"].as_str().ok_or("missing collector ID")?.to_owned(),
  Err(_)=>{println!("{}",json!({"response_collector_supported":false,"reason":"unsupported_or_rejected","no_live_site_requests":true}));return Ok(());}
 };
    let mut fixture = Fixture::new(
        client,
        context,
        vec![("GET", ENTRY, HTML.into()), ("GET", SCRIPT, JS.into())],
    )?;
    fixture.navigate(client, context, ENTRY)?;
    let ready=client.call("script.evaluate",json!({"expression":"document.getElementById('manualField').value='manual edit';document.getElementById('manualField').focus({preventScroll:true});window.scrollTo(0,200);window.syntheticPassiveScript===true","target":{"context":context},"awaitPromise":false}))?["result"]["value"]==true;
    if !ready {
        return Err("passive fixture script did not load".into());
    }
    // From here only response data is read. No DOM snapshot, screenshot, reload,
    // fetch, focus or scroll command participates in collecting those bodies.
    let mut queue =
        ceh_browser::passive::ResponseQueue::new(context).map_err(|_| "passive queue invalid")?;
    for event in &client.events {
        queue.ingest(event);
    }
    let batch = queue.drain();
    if batch.overflowed {
        return Err("fixture overflowed capture queue".into());
    }
    let archive_root = tempfile::Builder::new()
        .prefix("passive-archive-")
        .tempdir_in(".local/builds/work/browser-spikes")?;
    let archive = ceh_services::diagnostics::ScriptArchive::new(archive_root.path());
    let session = archive
        .prepare_session("native-passive-fixture", 7, 1_793_534_400.0, false)
        .map_err(|_| "archive session failed")?;
    let mut count = 0;
    let mut archived_scripts = 0;
    for candidate in batch.responses {
        let expected = if candidate.url() == format!("{ORIGIN}{ENTRY}") {
            HTML
        } else if candidate.url() == format!("{ORIGIN}{SCRIPT}") {
            JS
        } else {
            continue;
        };
        let body = ceh_browser::passive::read_response(
            client.bidi.as_mut().ok_or("missing BiDi")?,
            &collector,
            &candidate,
        )
        .map_err(|_| "bounded passive body read failed")?;
        if body.as_bytes() != expected.as_bytes() {
            return Err("collected response differs from loaded bytes".into());
        }
        // Only public static JS goes through the existing sensitive-value filter
        // and content-addressed archive. HTML persistence needs its own policy.
        if candidate.kind() == ceh_browser::passive::ResponseKind::Script
            && archive
                .save_script(
                    &session,
                    candidate.url(),
                    body.as_bytes(),
                    candidate.status(),
                    candidate.mime(),
                    1_793_534_400.0,
                )
                .map_err(|_| "script archive failed")?
                .is_some()
        {
            archived_scripts += 1;
        }
        count += 1;
    }
    if archived_scripts != 1
        || std::fs::read_dir(archive_root.path().join("resources"))?.count() != 1
    {
        return Err("script archive count mismatch".into());
    }
    client.call(
        "network.removeDataCollector",
        json!({"collector":collector}),
    )?;
    if count != 2 || fixture.entry != 2 || fixture.posts != 0 {
        return Err("passive response count wrong or requests repeated".into());
    }
    // Verification only: this is not the runtime's manual-page capture implementation.
    let kept=client.call("script.evaluate",json!({"expression":"document.getElementById('manualField').value==='manual edit' && document.activeElement.id==='manualField' && window.scrollY===200","target":{"context":context},"awaitPromise":false}))?["result"]["value"]==true;
    if !kept {
        return Err("passive retrieval changed manual state".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"response_collector_supported":true,"html_and_javascript_exact":true,"response_count":count,"get_requests":fixture.entry,"post_requests":fixture.posts,"no_live_site_requests":true,"no_refetch_or_dom_capture_for_retrieval":true,"manual_input_focus_scroll_preserved":true,"bounded_collector_disowned_and_removed":true,"production_metadata_queue_used":true,"bounded_body_api_used":true,"static_js_archived":archived_scripts,"manual_html_not_persisted":true})
        )?
    );
    Ok(())
}
