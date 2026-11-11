//! Real Firefox on a local page, window hidden (macOS): does a pointer action still complete? Run with
//! `-- --ignored --nocapture`. Local 127.0.0.1 only.
use ceh_browser::firefox::{FirefoxInstallation, OwnedFirefox, ProfileLease};
use serde_json::json;
use std::{io::{Read, Write}, net::TcpListener, sync::atomic::AtomicBool, time::Instant};

#[test]
#[ignore = "launches the installed Firefox and hides it"]
fn a_hidden_firefox_still_completes_a_click() {
    let Ok(installation) = FirefoxInstallation::discover() else { return };
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buffer = [0u8; 1024];
            let _ = stream.read(&mut buffer);
            let body = "<html><body><button id=b onclick=\"this.dataset.n=1\" style=\"margin:60px\">go</button></body></html>";
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
    let runtime = tempfile::tempdir().unwrap();
    let lease = ProfileLease::temporary(runtime.path()).unwrap();
    let mut browser = OwnedFirefox::start(&installation, lease, false, &AtomicBool::new(false)).unwrap();
    let context = browser.context().to_owned();
    let pid = browser.process_id().unwrap();
    let connection = browser.connection();
    connection.call("browsingContext.navigate", json!({"context":context,"url":format!("http://127.0.0.1:{port}/"),"wait":"complete"})).unwrap();
    // Hide the browser's own process (Cmd+H equivalent).
    if std::env::var("COVER").is_ok() {
        // Cover the whole screen with a Finder window instead of hiding the browser.
        let cover = "tell application \"Finder\" to make new Finder window\ntell application \"Finder\" to set bounds of front window to {0, 0, 4000, 3000}\ntell application \"Finder\" to activate";
        let _ = std::process::Command::new("osascript").args(["-e", cover]).status();
    } else {
        let hide = format!("tell application \"System Events\" to set visible of (first process whose unix id is {pid}) to false");
        let _ = std::process::Command::new("osascript").args(["-e", &hide]).status();
    }
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let node = connection.call("script.callFunction", json!({"functionDeclaration":"()=>document.getElementById('b')","target":{"context":context},"awaitPromise":false,"arguments":[]})).unwrap();
    let shared = node["result"]["sharedId"].as_str().unwrap().to_owned();
    let started = Instant::now();
    let result = connection.call("input.performActions", json!({"context":context,"actions":[{"type":"pointer","id":"p","parameters":{"pointerType":"mouse"},"actions":[
        {"type":"pointerMove","duration":50,"x":0,"y":0,"origin":{"type":"element","element":{"sharedId":shared}}},{"type":"pointerDown","button":0},{"type":"pointerUp","button":0}]}]}));
    println!("hidden click: {:?} after {} ms", result.is_ok(), started.elapsed().as_millis());
    let state = connection.call("script.evaluate", json!({"expression":"document.visibilityState+':'+document.hasFocus()","target":{"context":context},"awaitPromise":false}));
    println!("state while covered: {:?}", state.map(|v| v["result"]["value"].clone()));
    if std::env::var("COVER").is_ok() {
        let _ = std::process::Command::new("osascript").args(["-e", "tell application \"Finder\" to close front window"]).status();
    }
    let _ = browser.stop();
    assert!(result.is_ok(), "the click did not complete while the window was hidden");
}
