use ceh_browser::{
    BrowserError, DispatchGate, bidi::BidiConnection, observer::PageObserver,
    request::activate_request,
};
use ceh_core::{control::Authority, model::*};
use serde_json::{Value, json};
use std::{net::TcpListener, thread, time::Duration};
use tungstenite::Message;
#[test]
fn pause_after_target_preparation_does_not_reserve_unsent_form() {
    let session = SessionId::new("trusted-pause").unwrap();
    let gate = DispatchGate::new(session.clone());
    let worker_gate = gate.clone();
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let worker_session = session.clone();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let mut authority = Authority::new(worker_session);
        let mut seeded = false;
        while let Ok(message) = socket.read() {
            let Ok(text) = message.into_text() else {
                break;
            };
            let request: Value = serde_json::from_str(&text).unwrap();
            if !seeded {
                seeded = true;
                for event in [
                    json!({"method":"browsingContext.navigationStarted","params":{"context":"main","navigation":"nav"}}),
                    json!({"method":"network.beforeRequestSent","params":{"context":"main","navigation":"nav","request":{"request":"req","method":"GET"}}}),
                    json!({"method":"browsingContext.load","params":{"context":"main","navigation":"nav"}}),
                ] {
                    let mut event = event;
                    event["type"] = json!("event");
                    socket
                        .send(Message::Text(event.to_string().into()))
                        .unwrap();
                }
            }
            let result = match request["method"].as_str().unwrap() {
                "browsingContext.getTree" => json!({"contexts":[{"context":"main","children":[]}]}),
                "script.evaluate" => {
                    json!({"type":"success","result":{"type":"boolean","value":true}})
                }
                "script.callFunction" => {
                    let target = request["params"]["arguments"][0]["value"]
                        .as_str()
                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                        .is_some_and(|v| v.get("issuedAt").is_some());
                    if target {
                        worker_gate
                            .apply_control(&authority.pause().unwrap())
                            .unwrap();
                        json!({"type":"success","result":{"type":"node","sharedId":"target-button"}})
                    } else {
                        let snapshot = json!({"visible":["body","#btnEnviar"],"frames":[{"text":"Solicitar cita"}],"official":true,"endpoint":"acValidarEntrada","button_label":"Solicitar cita"});
                        json!({"type":"success","realm":"realm-one","result":{"type":"string","value":snapshot.to_string()}})
                    }
                }
                "input.performActions" => panic!("paused input was published"),
                _ => panic!("unexpected protocol command"),
            };
            if socket
                .send(Message::Text(
                    json!({"type":"success","id":request["id"],"result":result})
                        .to_string()
                        .into(),
                ))
                .is_err()
            {
                break;
            }
        }
        authority
    });
    let mut connection = BidiConnection::connect(address, Duration::from_secs(2)).unwrap();
    let mut observer = PageObserver::new("main").unwrap();
    let guard = DocumentGuard {
        session,
        epoch: 0,
        document: DocumentId::new("realm-one").unwrap(),
        expected_page: PageKind::RequestMenu,
    };
    assert_eq!(
        activate_request(&mut connection, &mut observer, &gate, &guard),
        Err(BrowserError::Paused)
    );
    drop(connection);
    let mut authority = server.join().unwrap();
    gate.apply_control(&authority.resume().unwrap()).unwrap();
    let mut fresh = guard;
    fresh.epoch = authority.snapshot().epoch();
    let page = Observation {
        document: fresh.document.clone(),
        kind: PageKind::RequestMenu,
        method: RequestMethod::Get,
        operation_code: false,
        variant: String::new(),
    };
    assert!(
        gate.begin(&fresh, &page, ActionKind::Request).is_ok(),
        "preparatory scroll reserved an unsent form"
    );
}
