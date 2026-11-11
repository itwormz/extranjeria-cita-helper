//! Real Chrome on a local page (no site): run with `-- --ignored`. Checks the CDP wire answers the BiDi
//! commands the driver uses.
use ceh_browser::chromium::ChromiumInstallation;
use ceh_browser::firefox::{OwnedFirefox, ProfileKind, ProfileLease};
use serde_json::{json, Value};
use std::{io::{Read, Write}, net::TcpListener, sync::atomic::AtomicBool};

fn serve() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buffer = [0u8; 2048];
            let n = stream.read(&mut buffer).unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..n]).to_string();
            let body = if request.starts_with("GET /inner") {
                "<html><body><p id=inner>inner</p><button id=ib style=\"margin:40px\" onclick=\"this.dataset.n=1\">in</button></body></html>"
            } else {
                "<html><body><button id=b onclick=\"this.dataset.n=(+(this.dataset.n||0)+1);this.dataset.trusted=event.isTrusted\">go</button><input id=i oninput=\"this.dataset.v=this.value\"><iframe id=f src=\"/inner\"></iframe></body></html>"
            };
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
    port
}
fn string(result: &Value) -> String {
    assert_eq!(result["type"], "success", "{result}");
    result["result"]["value"].as_str().map(str::to_owned).unwrap_or_else(|| result["result"]["value"].to_string())
}

#[test]
#[ignore = "launches the installed Chrome"]
fn the_cdp_wire_serves_navigation_scripts_nodes_input_frames_and_events() {
    let Ok(installation) = ChromiumInstallation::discover(ProfileKind::Chrome) else { return };
    let runtime = tempfile::tempdir().unwrap();
    let lease = ProfileLease::temporary_kind(runtime.path(), ProfileKind::Chrome).unwrap();
    let mut browser = OwnedFirefox::start_chromium(&installation, lease, &AtomicBool::new(false)).unwrap();
    assert!(browser.page_open());
    let context = browser.context().to_owned();
    let port = serve();
    let url = format!("http://127.0.0.1:{port}/");
    let connection = browser.connection();
    let collector = connection.call("network.addDataCollector", json!({"dataTypes":["response"],"maxEncodedDataSize":131072,"collectorType":"blob","contexts":[context]})).unwrap();
    let collector = collector["collector"].as_str().unwrap().to_owned();
    connection.call("browsingContext.navigate", json!({"context":context,"url":url,"wait":"complete"})).unwrap();
    // Events: a navigation, its document request and response, and the load.
    let events = connection.take_events();
    let methods: Vec<&str> = events.iter().filter_map(|e| e["method"].as_str()).collect();
    for wanted in ["browsingContext.navigationStarted", "network.beforeRequestSent", "network.responseCompleted", "browsingContext.load"] {
        assert!(methods.contains(&wanted), "{wanted} in {methods:?}");
    }
    let document = events.iter().find(|e| e["method"] == "network.beforeRequestSent" && !e["params"]["navigation"].is_null()).unwrap();
    assert_eq!(document["params"]["request"]["destination"], "document");
    // The collector keeps the document's response body for a later read (the F5 script archive reads these).
    let request = document["params"]["request"]["request"].as_str().unwrap().to_owned();
    let body = connection.call("network.getData", json!({"request":request,"dataType":"response","collector":collector,"disown":true})).unwrap();
    assert_eq!(body["bytes"]["type"], "string");
    assert!(body["bytes"]["value"].as_str().unwrap().contains("<button id=b"));
    assert!(connection.call("network.getData", json!({"request":"unknown","dataType":"response","collector":collector,"disown":true})).is_err());
    connection.call("network.disownData", json!({"request":request,"dataType":"response","collector":collector})).unwrap();
    connection.call("network.removeDataCollector", json!({"collector":collector})).unwrap();
    // Scripts: a string, a boolean, and the realm that identifies the document.
    let title = connection.call("script.evaluate", json!({"expression":"document.readyState","target":{"context":context},"awaitPromise":false})).unwrap();
    assert_eq!(string(&title), "complete");
    let realm = title["realm"].as_str().unwrap().to_owned();
    let again = connection.call("script.callFunction", json!({"functionDeclaration":"()=>true","target":{"realm":realm},"awaitPromise":false,"arguments":[]})).unwrap();
    assert_eq!(again["result"], json!({"type":"boolean","value":true}));
    // A node result and a click on it with the browser's own input: a trusted event.
    let node = connection.call("script.callFunction", json!({"functionDeclaration":"()=>document.getElementById('b')","target":{"realm":realm},"awaitPromise":false,"arguments":[]})).unwrap();
    assert_eq!(node["result"]["type"], "node");
    let shared = node["result"]["sharedId"].as_str().unwrap().to_owned();
    let origin = json!({"type":"element","element":{"sharedId":shared}});
    connection.call("input.performActions", json!({"context":context,"actions":[{"type":"pointer","id":"p","parameters":{"pointerType":"mouse"},"actions":[
        {"type":"pointerMove","duration":20,"x":60,"y":30,"origin":origin},{"type":"pointerMove","duration":20,"x":0,"y":0,"origin":origin},
        {"type":"pointerDown","button":0},{"type":"pause","duration":30},{"type":"pointerUp","button":0}]}]})).unwrap();
    let clicked = connection.call("script.evaluate", json!({"expression":"document.getElementById('b').dataset.n+':'+document.getElementById('b').dataset.trusted","target":{"context":context},"awaitPromise":false})).unwrap();
    assert_eq!(string(&clicked), "1:true");
    // Typing: keys reach the focused input as trusted key events.
    connection.call("script.evaluate", json!({"expression":"document.getElementById('i').focus()","target":{"context":context},"awaitPromise":false})).unwrap();
    connection.call("input.performActions", json!({"context":context,"actions":[{"type":"key","id":"k","actions":[
        {"type":"keyDown","value":"h"},{"type":"keyUp","value":"h"},{"type":"keyDown","value":"i"},{"type":"keyUp","value":"i"}]}]})).unwrap();
    let typed = connection.call("script.evaluate", json!({"expression":"document.getElementById('i').value","target":{"context":context},"awaitPromise":false})).unwrap();
    assert_eq!(string(&typed), "hi");
    // Select all and delete: the modifier shortcut and Backspace.
    connection.call("input.performActions", json!({"context":context,"actions":[{"type":"key","id":"k","actions":[
        {"type":"keyDown","value":"\u{E03D}"},{"type":"keyDown","value":"a"},{"type":"keyUp","value":"a"},{"type":"keyUp","value":"\u{E03D}"},
        {"type":"keyDown","value":"\u{E003}"},{"type":"keyUp","value":"\u{E003}"}]}]})).unwrap();
    let cleared = connection.call("script.evaluate", json!({"expression":"document.getElementById('i').value","target":{"context":context},"awaitPromise":false})).unwrap();
    assert_eq!(string(&cleared), "");
    // Frames: the tree lists the iframe, and its owner element is found from the parent.
    let tree = connection.call("browsingContext.getTree", json!({})).unwrap();
    let child = tree["contexts"][0]["children"][0]["context"].as_str().unwrap().to_owned();
    let located = connection.call("browsingContext.locateNodes", json!({"context":context,"locator":{"type":"context","value":{"context":child}},"maxNodeCount":2})).unwrap();
    let owner = located["nodes"][0]["sharedId"].as_str().unwrap().to_owned();
    let visible = connection.call("script.callFunction", json!({"functionDeclaration":"el=>el.isConnected","target":{"realm":realm},"awaitPromise":false,"arguments":[{"sharedId":owner}]})).unwrap();
    assert_eq!(visible["result"], json!({"type":"boolean","value":true}));
    let inner = connection.call("script.evaluate", json!({"expression":"document.getElementById('inner').textContent","target":{"context":child},"awaitPromise":false})).unwrap();
    assert_eq!(string(&inner), "inner");
    // A click on a button inside the iframe lands on it (main-viewport coordinates, not frame-relative ones).
    let inner_node = connection.call("script.callFunction", json!({"functionDeclaration":"()=>document.getElementById('ib')","target":{"context":child},"awaitPromise":false,"arguments":[]})).unwrap();
    let inner_realm = inner_node["realm"].as_str().unwrap().to_owned();
    let inner_shared = inner_node["result"]["sharedId"].as_str().unwrap().to_owned();
    let inner_origin = json!({"type":"element","element":{"sharedId":inner_shared}});
    connection.call("input.performActions", json!({"context":child,"actions":[{"type":"pointer","id":"p","parameters":{"pointerType":"mouse"},"actions":[
        {"type":"pointerMove","duration":10,"x":0,"y":0,"origin":inner_origin},{"type":"pointerDown","button":0},{"type":"pointerUp","button":0}]}]})).unwrap();
    let inner_clicked = connection.call("script.callFunction", json!({"functionDeclaration":"()=>document.getElementById('ib').dataset.n","target":{"realm":inner_realm},"awaitPromise":false,"arguments":[]})).unwrap();
    assert_eq!(string(&inner_clicked), "1");
    // Punctuation and a space type as a real keyboard would.
    connection.call("script.evaluate", json!({"expression":"document.getElementById('i').focus()","target":{"context":context},"awaitPromise":false})).unwrap();
    let keys: Vec<Value> = "a b@.-".chars().flat_map(|c| [json!({"type":"keyDown","value":c.to_string()}), json!({"type":"keyUp","value":c.to_string()})]).collect();
    connection.call("input.performActions", json!({"context":context,"actions":[{"type":"key","id":"k","actions":keys}]})).unwrap();
    let punctuated = connection.call("script.evaluate", json!({"expression":"document.getElementById('i').value","target":{"context":context},"awaitPromise":false})).unwrap();
    assert_eq!(string(&punctuated), "a b@.-");
    // A JavaScript dialog is dismissed instead of freezing the page.
    let _ = connection.call("script.evaluate", json!({"expression":"setTimeout(()=>alert('x'),0)","target":{"context":context},"awaitPromise":false}));
    std::thread::sleep(std::time::Duration::from_millis(300));
    let alive = connection.call("script.evaluate", json!({"expression":"1+1","target":{"context":context},"awaitPromise":false})).unwrap();
    assert_eq!(string(&alive), "2");
    // A navigation makes the old realm stale (fail closed) and a context call gets the new document.
    connection.call("browsingContext.navigate", json!({"context":context,"url":format!("{url}?second"),"wait":"complete"})).unwrap();
    assert!(connection.call("script.callFunction", json!({"functionDeclaration":"()=>1","target":{"realm":realm},"awaitPromise":false,"arguments":[]})).is_err());
    let fresh = connection.call("script.evaluate", json!({"expression":"1","target":{"context":context},"awaitPromise":false})).unwrap();
    assert_ne!(fresh["realm"], json!(realm));
    // The page the person closes ends the page, not the app.
    connection.call("browsingContext.close", json!({"context":context})).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(!browser.page_open());
    let _ = browser.stop();
}
