//! Native input protocol experiment. Synthetic data and intercepted requests only.
use super::{Client, Result, portal_fixture::Fixture};
use serde_json::{Value, json};
const ENTRY: &str = "/icpplus/rust-fixture-trusted-input";
const RESULT: &str = "/icpplus/rust-fixture-trusted-result";
const PAGE: &str = "<body style='min-height:2000px'><form method='post' action='/icpplus/rust-fixture-trusted-result'><input id='phone' name='phone' onpaste='window.pastes=(window.pastes||0)+1;event.preventDefault()' oninput='if(!event.isTrusted)this.value=String()'><button id='send' onclick='window.trustedClick=event.isTrusted;if(!event.isTrusted)event.preventDefault()'>Submit</button></form></body>";
fn node(client: &mut Client, context: &str, id: &str) -> Result<Value> {
    let expression = format!("document.getElementById({})", serde_json::to_string(id)?);
    let result = client.call(
        "script.evaluate",
        json!({"expression":expression,"target":{"context":context},"awaitPromise":false}),
    )?;
    let shared = result["result"]["sharedId"]
        .as_str()
        .ok_or("node reference unavailable")?;
    Ok(json!({"sharedId":shared}))
}
fn click(client: &mut Client, context: &str, node: Value) -> Result<()> {
    client.call("input.performActions",json!({"context":context,"actions":[{"type":"pointer","id":"fixture-pointer","parameters":{"pointerType":"mouse"},"actions":[{"type":"pointerMove","x":0,"y":0,"duration":0,"origin":{"type":"element","element":node}},{"type":"pointerDown","button":0},{"type":"pointerUp","button":0}]}]}))?;
    Ok(())
}
pub fn run(client: &mut Client, context: &str) -> Result<()> {
    let mut fixture = Fixture::new(
        client,
        context,
        vec![
            ("GET", ENTRY, PAGE.into()),
            ("POST", RESULT, "<body>Complete</body>".into()),
        ],
    )?;
    fixture.navigate(client, context, ENTRY)?;
    // First prove ordinary JS clicks cannot satisfy this form.
    let rejected=client.call("script.evaluate",json!({"expression":"document.getElementById('send').click();window.trustedClick===false","target":{"context":context},"awaitPromise":false}))?["result"]["value"]==true;
    if !rejected {
        return Err("fixture did not reject untrusted click".into());
    }
    let phone = node(client, context, "phone")?;
    click(client, context, phone)?;
    let mut keys = Vec::new();
    for ch in "600123456".chars() {
        keys.push(json!({"type":"keyDown","value":ch.to_string()}));
        keys.push(json!({"type":"keyUp","value":ch.to_string()}));
    }
    client.call("input.performActions",json!({"context":context,"actions":[{"type":"key","id":"fixture-keyboard","actions":keys}]}))?;
    let typed=client.call("script.evaluate",json!({"expression":"document.getElementById('phone').value==='600123456' && !window.pastes","target":{"context":context},"awaitPromise":false}))?["result"]["value"]==true;
    if !typed {
        return Err("native trusted keyboard not accepted".into());
    }
    let send = node(client, context, "send")?;
    click(client, context, send.clone())?;
    fixture.wait_load(client, context, RESULT)?;
    // A node from the old realm must never turn into coordinates on a new page.
    fixture.navigate(client, context, ENTRY)?;
    if click(client, context, send).is_ok() {
        return Err("stale element pointer was not rejected".into());
    }
    let untouched=client.call("script.evaluate",json!({"expression":"window.trustedClick===undefined && document.getElementById('phone').value===''","target":{"context":context},"awaitPromise":false}))?["result"]["value"]==true;
    if !untouched || fixture.posts != 1 || fixture.entry != 2 {
        return Err("stale pointer mutated new page".into());
    }
    client.call("input.releaseActions", json!({"context":context}))?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"untrusted_dom_click_refused":true,"native_keyboard_no_paste":true,"native_pointer_trusted_post":true,"stale_element_does_not_target_new_document":true,"get_requests":fixture.entry,"post_requests":fixture.posts,"no_live_site_requests":true,"headless_only_not_background_or_lock_acceptance":true})
        )?
    );
    Ok(())
}
