use ceh_browser::{bidi::BidiConnection, observer::PageObserver};
use ceh_core::model::{PageKind, RequestMethod};
use serde_json::json;
use std::{net::TcpListener, thread, time::Duration};
use tungstenite::Message;
#[test]
fn observer_holds_pending_navigation_and_uses_actual_method_after_load() {
    for (loaded, booking, contact, blank_child) in [
        (false, false, false, false),
        (true, false, false, false),
        (false, true, false, false),
        (true, false, true, false),
        (true, false, false, true),
    ] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            for index in 0..if blank_child { 8 } else { 4 } {
                let request: serde_json::Value =
                    serde_json::from_str(&socket.read().unwrap().into_text().unwrap()).unwrap();
                if index == 0 {
                    let mut events = vec![
                        json!({"method":"browsingContext.navigationStarted","params":{"context":"main","navigation":"nav"}}),
                        json!({"method":"network.beforeRequestSent","params":{"context":"main","navigation":"nav","request":{"request":"request","method":"GET"}}}),
                        json!({"method":"network.responseCompleted","params":{"context":"main","navigation":"nav","request":{"request":"request"},"response":{"status":200}}}),
                    ];
                    if loaded {
                        events.push(json!({"method":"browsingContext.load","params":{"context":"main","navigation":"nav"}}));
                    }
                    for mut event in events {
                        event["type"] = json!("event");
                        socket
                            .send(Message::Text(event.to_string().into()))
                            .unwrap();
                    }
                }
                let result = match request["method"].as_str().unwrap() {
                    "browsingContext.getTree" => {
                        json!({"contexts":[{"context":"main","children":if blank_child {vec![json!({"context":"blank","children":[]})]}else{vec![]}}]})
                    }
                    "script.evaluate" => {
                        json!({"type":"success","result":{"type":"boolean","value":true}})
                    }
                    "browsingContext.locateNodes" => json!({"nodes":[{"sharedId":"owner"}]}),
                    "script.callFunction"
                        if request["params"]["arguments"][0].get("sharedId").is_some() =>
                    {
                        json!({"type":"success","result":{"type":"boolean","value":true}})
                    }
                    "script.callFunction" => {
                        let child = request["params"]["target"]["context"] == "blank";
                        let snapshot = if child {
                            json!({"visible":["body"],"frames":[{"text":""}],"official":false})
                        } else if contact {
                            json!({"visible":["body"],"frames":[{"text":"No hay citas disponibles"}],"official":true})
                        } else if booking {
                            json!({"visible":["body","input[name*='fecha']"],"frames":[{"text":"Choose date"}],"official":true})
                        } else {
                            json!({"visible":["body","#txtIdCitado"],"frames":[{"text":"No hay citas disponibles"}],"official":true,"applicant_editable":true})
                        };
                        json!({"type":"success","realm":if child{"blank-realm"}else{"main-realm"},"result":{"type":"string","value":snapshot.to_string()}})
                    }
                    _ => panic!("unexpected observer command"),
                };
                socket
                    .send(Message::Text(
                        json!({"id":request["id"],"type":"success","result":result})
                            .to_string()
                            .into(),
                    ))
                    .unwrap();
            }
        });
        let mut connection = BidiConnection::connect(address, Duration::from_secs(2)).unwrap();
        let mut observer = PageObserver::new("main").unwrap();
        if contact {
            observer.set_actions(None, Some(ceh_core::model::ActionKind::Contact));
        }
        let page = observer.observe(&mut connection, "manual").unwrap();
        assert_eq!(
            page.observation.kind,
            if contact {
                PageKind::PostContactNoSlots
            } else if booking {
                PageKind::Booking
            } else if loaded {
                PageKind::ApplicantNoSlots
            } else {
                PageKind::Loading
            }
        );
        assert_eq!(
            page.observation.method,
            if loaded {
                RequestMethod::Get
            } else {
                RequestMethod::Unknown
            }
        );
        assert_eq!(page.observation.document.as_str(), "main-realm");
        server.join().unwrap();
    }
}

#[test]
fn network_quiet_follows_requests_of_this_context_only() {
    use ceh_browser::observer::PageObserver;
    use serde_json::json;
    let mut observer = PageObserver::new("ctx").unwrap();
    let event = |method: &str, context: &str| json!({"type":"event","method":method,"params":{"context":context,"navigation":"n1","request":{"request":"r1","url":"https://icp.administracionelectronica.gob.es/icpplus/index.html","method":"GET"},"response":{"status":200,"url":"https://icp.administracionelectronica.gob.es/icpplus/index.html"},"timestamp":1}});
    assert!(observer.network_quiet());
    let _ = observer.ingest(&event("network.beforeRequestSent", "other"));
    assert!(observer.network_quiet(), "another tab's request does not count");
    let _ = observer.ingest(&event("network.beforeRequestSent", "ctx"));
    let _ = observer.ingest(&event("network.beforeRequestSent", "ctx"));
    assert!(!observer.network_quiet());
    let _ = observer.ingest(&event("network.responseCompleted", "ctx"));
    assert!(!observer.network_quiet());
    observer.ingest(&event("network.fetchError", "ctx")).unwrap();
    assert!(observer.network_quiet(), "a failed request is finished too");
    let _ = observer.ingest(&event("network.responseCompleted", "ctx"));
    assert!(observer.network_quiet(), "never negative");
    let _ = observer.ingest(&event("network.beforeRequestSent", "ctx"));
    let _ = observer.ingest(&event("browsingContext.navigationStarted", "ctx"));
    assert!(observer.network_quiet(), "a new navigation drops the old requests");
}
