use ceh_browser::{BrowserError, auth::AuthFacts};
use serde_json::{Value, json};
#[test]
fn authentication_facts_are_boolean_only_and_reject_extra_private_fields() {
    let empty = json!({"portal_entry":false,"provider":false,"certificate":false,"movil":false,"permanente":false,"requires_attention":false,"input_started":false});
    assert_eq!(
        AuthFacts::decode(&empty.to_string()).unwrap(),
        AuthFacts::default()
    );
    let mut extra = empty.clone();
    extra["password"] = json!("never-export");
    assert_eq!(
        AuthFacts::decode(&extra.to_string()),
        Err(BrowserError::NoObservation)
    );
    let mut wrong: Value = empty;
    wrong["input_started"] = json!("private");
    assert_eq!(
        AuthFacts::decode(&wrong.to_string()),
        Err(BrowserError::NoObservation)
    );
    assert_eq!(
        AuthFacts::decode(&"x".repeat(2049)),
        Err(BrowserError::NoObservation)
    );
}

// A native certificate prompt may withhold the input acknowledgement. Its
// reminder must be armed after publication, without waiting for that response.
#[test]
fn certificate_publication_is_reported_before_the_input_acknowledgement() {
    use ceh_browser::{
        DispatchGate,
        auth::{AuthenticationChoice, select_method},
        bidi::BidiConnection,
        observer::PageObserver,
    };
    use ceh_core::model::*;
    use std::{net::TcpListener, sync::mpsc, thread, time::Duration};
    use tungstenite::Message;
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let (published, notified) = mpsc::channel();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let mut seeded = false;
        let mut inputs = 0;
        while let Ok(message) = socket.read() {
            let request: Value = serde_json::from_str(&message.into_text().unwrap()).unwrap();
            if !seeded {
                seeded = true;
                for event in [
                    json!({"type":"event","method":"browsingContext.navigationStarted","params":{"context":"main","navigation":"nav"}}),
                    json!({"type":"event","method":"network.beforeRequestSent","params":{"context":"main","navigation":"nav","request":{"request":"req","method":"GET"}}}),
                    json!({"type":"event","method":"browsingContext.load","params":{"context":"main","navigation":"nav"}}),
                ] {
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
                    let declaration = request["params"]["functionDeclaration"].as_str().unwrap();
                    if declaration.contains("const facts={portal_entry") {
                        let facts = json!({"portal_entry":false,"provider":true,"certificate":true,"movil":false,"permanente":false,"requires_attention":false,"input_started":false});
                        json!({"type":"success","result":{"type":"string","value":facts.to_string()}})
                    } else if declaration.contains("scrollIntoView") {
                        json!({"type":"success","result":{"type":"node","sharedId":"certificate-button"}})
                    } else {
                        let snapshot = json!({"visible":["body"],"frames":[{"text":"Certificado electrónico"}],"official":false,"endpoint":"ServiceProvider","button_label":""});
                        json!({"type":"success","realm":"auth-realm","result":{"type":"string","value":snapshot.to_string()}})
                    }
                }
                "input.performActions" => {
                    inputs += 1;
                    notified
                        .recv_timeout(Duration::from_secs(2))
                        .expect("publication hook waited for input acknowledgement");
                    json!({})
                }
                _ => panic!("unexpected command"),
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
        inputs
    });
    let mut connection = BidiConnection::connect(address, Duration::from_secs(3)).unwrap();
    let session = SessionId::new("certificate-publication").unwrap();
    let gate = DispatchGate::new(session.clone());
    let guard = DocumentGuard {
        session,
        epoch: 0,
        document: DocumentId::new("auth-realm").unwrap(),
        expected_page: PageKind::AuthWait,
    };
    let mut observer = PageObserver::new("main").unwrap();
    let mut calls = 0;
    let result = select_method(
        &mut connection,
        &mut observer,
        &gate,
        &guard,
        AuthenticationChoice::Certificate,
        || {
            calls += 1;
            published.send(()).unwrap();
        },
    );
    assert_eq!(result, Ok(DispatchOutcome::Dispatched));
    assert_eq!(calls, 1);
    drop(connection);
    assert_eq!(server.join().unwrap(), 1);
}
