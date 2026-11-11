//! The CDP wire against a scripted fake browser (no Chrome): a command that outlives its timeout, because a
//! native dialog (certificate, keychain) holds the browser, must not poison the connection.
use ceh_browser::bidi::BidiConnection;
use serde_json::{json, Value};
use std::{net::TcpListener, time::Duration};

fn fake_browser(slow_first_evaluate: Duration) -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let mut slow = true;
        while let Ok(message) = socket.read() {
            let Ok(text) = message.into_text() else { break };
            let request: Value = serde_json::from_str(&text).unwrap();
            let result = match request["method"].as_str().unwrap() {
                "Target.getTargets" => json!({"targetInfos":[{"targetId":"T","type":"page"}]}),
                "Target.attachToTarget" => json!({"sessionId":"S"}),
                "Page.createIsolatedWorld" => json!({"executionContextId":1}),
                "Runtime.evaluate" => {
                    if std::mem::take(&mut slow) {
                        std::thread::sleep(slow_first_evaluate);
                    }
                    json!({"result":{"type":"number","value":2}})
                }
                _ => json!({}),
            };
            socket.send(tungstenite::Message::Text(json!({"id":request["id"],"result":result}).to_string().into())).unwrap();
        }
    });
    port
}

#[test]
fn a_command_held_past_its_timeout_fails_alone_and_the_connection_goes_on() {
    let port = fake_browser(Duration::from_millis(450));
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut connection = BidiConnection::connect_cdp(address, "/devtools/browser/fake", Duration::from_millis(300)).unwrap();
    let params = json!({"expression":"1+1","target":{"context":"T"},"awaitPromise":false});
    assert!(connection.call("script.evaluate", params.clone()).is_err(), "the held command times out");
    assert!(connection.available(), "a slow browser is not a broken connection");
    // The late reply of the first command arrives while the next one waits: it is not mistaken for its answer.
    let again = connection.call("script.evaluate", params).unwrap();
    assert_eq!(again["result"], json!({"type":"number","value":2}));
}
