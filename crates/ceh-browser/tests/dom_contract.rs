use ceh_browser::{bidi::BidiConnection, dom::read_document};
use ceh_core::{model::PageKind, recognition::recognize};
use serde_json::json;
use std::{net::TcpListener, thread, time::Duration};
use tungstenite::Message;
#[test]
fn transient_document_facts_keep_realm_and_recognition_without_persisting_input_values() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let request: serde_json::Value =
            serde_json::from_str(&socket.read().unwrap().into_text().unwrap()).unwrap();
        assert_eq!(request["method"], "script.callFunction");
        let snapshot = json!({"visible":["body","#txtIdCitado"],"frames":[{"text":"No hay citas disponibles"}],"official":true,"endpoint":"acCitar","applicant_editable":true});
        socket.send(Message::Text(json!({"id":request["id"],"type":"success","result":{"type":"success","realm":"document-realm","result":{"type":"string","value":snapshot.to_string()}}}).to_string().into())).unwrap();
    });
    let mut connection = BidiConnection::connect(address, Duration::from_secs(2)).unwrap();
    let document = read_document(&mut connection, "context", "manual").unwrap();
    assert_eq!(document.realm.as_str(), "document-realm");
    assert_eq!(
        recognize(&document.snapshot).unwrap().kind,
        PageKind::ApplicantNoSlots
    );
    server.join().unwrap();
}

#[test]
fn frame_visibility_resolves_exact_context_owner_in_observed_parent_realm() {
    use ceh_browser::dom::frame_visible;
    use ceh_core::model::DocumentId;
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let request: serde_json::Value =
            serde_json::from_str(&socket.read().unwrap().into_text().unwrap()).unwrap();
        assert_eq!(request["method"], "browsingContext.locateNodes");
        assert_eq!(
            request["params"]["locator"],
            json!({"type":"context","value":{"context":"child"}})
        );
        socket.send(Message::Text(json!({"id":request["id"],"type":"success","result":{"nodes":[{"type":"node","sharedId":"owner-node"}]}}).to_string().into())).unwrap();
        let request: serde_json::Value =
            serde_json::from_str(&socket.read().unwrap().into_text().unwrap()).unwrap();
        assert_eq!(request["params"]["target"], json!({"realm":"parent-realm"}));
        assert_eq!(
            request["params"]["arguments"],
            json!([{"sharedId":"owner-node"}])
        );
        socket.send(Message::Text(json!({"id":request["id"],"type":"success","result":{"type":"success","result":{"type":"boolean","value":false}}}).to_string().into())).unwrap();
    });
    let mut connection = BidiConnection::connect(address, Duration::from_secs(2)).unwrap();
    assert!(
        !frame_visible(
            &mut connection,
            "parent",
            &DocumentId::new("parent-realm").unwrap(),
            "child"
        )
        .unwrap()
    );
    server.join().unwrap();
}

#[test]
fn visible_tree_keeps_nested_error_source_and_main_document_identity() {
    use ceh_browser::dom::read_page;
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        for _ in 0..7 {
            let request: serde_json::Value =
                serde_json::from_str(&socket.read().unwrap().into_text().unwrap()).unwrap();
            let params = &request["params"];
            let result = match request["method"].as_str().unwrap() {
                "browsingContext.getTree" => {
                    json!({"contexts":[{"context":"main","children":[{"context":"child","children":[]}]}]})
                }
                "browsingContext.locateNodes" => json!({"nodes":[{"sharedId":"owner"}]}),
                "script.evaluate" => {
                    json!({"type":"success","result":{"type":"boolean","value":true}})
                }
                "script.callFunction"
                    if params
                        .get("arguments")
                        .is_some_and(|a| a[0].get("sharedId").is_some()) =>
                {
                    json!({"type":"success","result":{"type":"boolean","value":true}})
                }
                "script.callFunction" => {
                    let child = params["target"]["context"] == "child";
                    let snapshot = json!({"visible":["body","#txtIdCitado"],"frames":[{"text":if child {"Sesión caducada"}else{"Applicant form"}}],"official":true,"endpoint":"acCitar","applicant_editable":true});
                    json!({"type":"success","realm":if child{"child-realm"}else{"main-realm"},"result":{"type":"string","value":snapshot.to_string()}})
                }
                _ => panic!("unexpected inspection command"),
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
    let page = read_page(&mut connection, "main", "manual").unwrap();
    assert_eq!(page.sources.len(), 2);
    assert_eq!(page.sources[0].realm.as_str(), "main-realm");
    let result = recognize(&page.snapshot).unwrap();
    assert_eq!(result.kind, PageKind::Expired);
    assert_eq!(result.source_frame, 1);
    assert_eq!(page.sources[result.source_frame].context, "child");
    server.join().unwrap();
}

#[test]
fn truncated_frame_tree_is_rejected_instead_of_ignoring_possible_errors() {
    use ceh_browser::{bidi::ProtocolError, dom::read_page};
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        for index in 0..2 {
            let request: serde_json::Value =
                serde_json::from_str(&socket.read().unwrap().into_text().unwrap()).unwrap();
            let result = if index == 0 {
                json!({"contexts":[{"context":"main","children":null}]})
            } else {
                let snapshot =
                    json!({"visible":["body","#txtIdCitado"],"frames":[{"text":"Applicant"}]});
                json!({"type":"success","realm":"realm","result":{"type":"string","value":snapshot.to_string()}})
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
    assert!(matches!(
        read_page(&mut connection, "main", "manual"),
        Err(ProtocolError::InvalidData)
    ));
    server.join().unwrap();
}
