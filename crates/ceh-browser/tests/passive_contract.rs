use ceh_browser::passive::{ResponseKind, ResponseQueue};
use serde_json::{Value, json};
fn event(id: &str, url: &str, mime: &str, navigation: bool) -> Value {
    json!({"method":"network.responseCompleted","params":{"context":"owned","timestamp":123456.75,"navigation":if navigation {json!("nav")}else{Value::Null},"request":{"request":id,"url":url},"response":{"url":url,"mimeType":mime,"status":200}}})
}
#[test]
fn only_owned_first_party_html_and_scripts_are_queued_without_query() {
    let mut q = ResponseQueue::new("owned").unwrap();
    q.ingest(&event("html","https://icp.administracionelectronica.gob.es/icpplus/acCitar;jsessionid=synthetic?secret=value","text/html",true));
    q.ingest(&event(
        "script",
        "https://icp.administracionelectronica.gob.es/icpplus/portal.js?v=1",
        "application/javascript",
        false,
    ));
    for url in [
        "http://icp.administracionelectronica.gob.es/icpplus/a.js",
        "https://icp.administracionelectronica.gob.es.evil.invalid/icpplus/a.js",
        "https://other.invalid/icpplus/a.js",
        "https://secret@icp.administracionelectronica.gob.es/icpplus/a.js",
        "https://icp.administracionelectronica.gob.es/outside/a.js",
    ] {
        q.ingest(&event("ignore", url, "application/javascript", false));
    }
    let mut foreign = event(
        "foreign",
        "https://icp.administracionelectronica.gob.es/icpplus/a.js",
        "application/javascript",
        false,
    );
    foreign["params"]["context"] = json!("not-owned");
    q.ingest(&foreign);
    q.ingest(&event(
        "xhr",
        "https://icp.administracionelectronica.gob.es/icpplus/xhr",
        "text/html",
        false,
    ));
    let batch = q.drain();
    assert!(!batch.overflowed);
    assert_eq!(batch.responses.len(), 2);
    assert_eq!(batch.responses[0].kind(), ResponseKind::Document);
    assert_eq!(batch.responses[0].context(), "owned");
    assert_eq!(batch.responses[0].navigation(), Some("nav"));
    assert_eq!(batch.responses[0].timestamp_millis(), Some(123456));
    assert_eq!(batch.responses[1].navigation(), None);

    assert_eq!(
        batch.responses[0].url(),
        "https://icp.administracionelectronica.gob.es/icpplus/acCitar"
    );
    assert_eq!(batch.responses[1].kind(), ResponseKind::Script);
}
#[test]
fn queue_and_recent_duplicate_memory_are_bounded() {
    let mut q = ResponseQueue::new("owned").unwrap();
    let one = event(
        "one",
        "https://icp.administracionelectronica.gob.es/icpplus/a.js",
        "application/javascript",
        false,
    );
    q.ingest(&one);
    q.ingest(&one);
    assert_eq!(q.drain().responses.len(), 1);
    q.ingest(&one);
    assert!(q.drain().responses.is_empty());
    for index in 0..20 {
        q.ingest(&event(
            &format!("r-{index}"),
            "https://icp.administracionelectronica.gob.es/icpplus/a.js",
            "application/javascript",
            false,
        ));
    }
    let batch = q.drain();
    assert_eq!(batch.responses.len(), 8);
    assert!(batch.overflowed);
    assert!(!q.drain().overflowed);
}

#[test]
fn redirects_binary_and_missing_navigation_do_not_hide_final_document() {
    let mut q = ResponseQueue::new("owned").unwrap();
    let mut redirect = event(
        "same",
        "https://icp.administracionelectronica.gob.es/icpplus/old",
        "text/html",
        true,
    );
    redirect["params"]["response"]["status"] = json!(302);
    q.ingest(&redirect);
    q.ingest(&event(
        "binary",
        "https://icp.administracionelectronica.gob.es/icpplus/a",
        "application/octet-stream",
        true,
    ));
    q.ingest(&event(
        "same",
        "https://icp.administracionelectronica.gob.es/icpplus/new",
        "text/html",
        true,
    ));
    let batch = q.drain();
    assert_eq!(batch.responses.len(), 1);
    assert_eq!(batch.responses[0].request(), "same");
    assert!(batch.responses[0].url().ends_with("/new"));
}
#[test]
fn observer_passive_queue_is_opt_in_and_disabling_discards_metadata() {
    let one = event(
        "one",
        "https://icp.administracionelectronica.gob.es/icpplus/a.js",
        "application/javascript",
        false,
    );
    let mut observer = ceh_browser::observer::PageObserver::new("owned").unwrap();
    observer.ingest(&one).unwrap();
    assert!(observer.take_responses().responses.is_empty());
    observer.set_passive_collection(true).unwrap();
    observer.ingest(&one).unwrap();
    assert_eq!(observer.take_responses().responses.len(), 1);
    let two = event(
        "two",
        "https://icp.administracionelectronica.gob.es/icpplus/b.js",
        "application/javascript",
        false,
    );
    observer.ingest(&two).unwrap();
    observer.set_passive_collection(false).unwrap();
    assert!(observer.take_responses().responses.is_empty());
}

#[test]
fn mime_parameters_cannot_turn_html_into_javascript() {
    let mut q = ResponseQueue::new("owned").unwrap();
    q.ingest(&event(
        "html",
        "https://icp.administracionelectronica.gob.es/icpplus/a",
        "text/html; charset=javascript",
        true,
    ));
    q.ingest(&event(
        "fake",
        "https://icp.administracionelectronica.gob.es/icpplus/b",
        "application/notjavascript",
        false,
    ));
    let batch = q.drain();
    assert_eq!(batch.responses.len(), 1);
    assert_eq!(batch.responses[0].kind(), ResponseKind::Document);
}

#[test]
fn response_body_decodes_bounded_string_or_base64_without_debug_data() {
    use ceh_browser::passive::{MAX_RESPONSE_BYTES, ResponseBody};
    assert_eq!(
        ResponseBody::decode(&serde_json::json!({"type":"string","value":"fixture"}))
            .unwrap()
            .as_bytes(),
        b"fixture"
    );
    assert_eq!(
        ResponseBody::decode(&serde_json::json!({"type":"base64","value":"Zml4dHVyZQ=="}))
            .unwrap()
            .as_bytes(),
        b"fixture"
    );
    for raw in [
        serde_json::json!({"type":"string","value":"x".repeat(MAX_RESPONSE_BYTES+1)}),
        serde_json::json!({"type":"base64","value":"%%%"}),
        serde_json::json!({"type":"string","value":"fixture","cookies":"PRIVATE"}),
        serde_json::json!({"type":"unknown","value":"PRIVATE"}),
    ] {
        assert!(ResponseBody::decode(&raw).is_err());
    }
}

#[test]
fn body_release_is_requested_even_when_response_decoding_fails() {
    use ceh_browser::{bidi::BidiConnection, passive::read_response};
    use std::{net::TcpListener, thread, time::Duration};
    use tungstenite::Message;
    let mut queue = ResponseQueue::new("owned").unwrap();
    queue.ingest(&event(
        "body",
        "https://icp.administracionelectronica.gob.es/icpplus/a.js",
        "application/javascript",
        false,
    ));
    let candidate = queue.drain().responses.remove(0);
    assert_eq!(candidate.status(), 200);
    assert_eq!(candidate.mime(), "application/javascript");
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let request: Value =
            serde_json::from_str(&socket.read().unwrap().into_text().unwrap()).unwrap();
        assert_eq!(request["method"], "network.getData");
        assert_eq!(
            request["params"],
            json!({"request":"body","dataType":"response","collector":"collector","disown":true})
        );
        socket.send(Message::Text(json!({"type":"success","id":request["id"],"result":{"bytes":{"type":"base64","value":"INVALID%%"}}}).to_string().into())).unwrap();
    });
    let mut connection = BidiConnection::connect(address, Duration::from_secs(2)).unwrap();
    assert!(read_response(&mut connection, "collector", &candidate).is_err());
    drop(connection);
    server.join().unwrap();
}

#[test]
fn rejected_body_is_released_without_retrying_the_get() {
    use ceh_browser::{
        bidi::{BidiConnection, ProtocolError},
        passive::read_response,
    };
    use std::{net::TcpListener, thread, time::Duration};
    use tungstenite::Message;
    let mut queue = ResponseQueue::new("owned").unwrap();
    queue.ingest(&event(
        "unavailable",
        "https://icp.administracionelectronica.gob.es/icpplus/a.js",
        "application/javascript",
        false,
    ));
    let candidate = queue.drain().responses.remove(0);
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let request: Value =
            serde_json::from_str(&socket.read().unwrap().into_text().unwrap()).unwrap();
        assert_eq!(request["method"], "network.getData");
        socket.send(Message::Text(json!({"type":"error","id":request["id"],"error":"unavailable network data","message":"PRIVATE"}).to_string().into())).unwrap();
        let request: Value =
            serde_json::from_str(&socket.read().unwrap().into_text().unwrap()).unwrap();
        assert_eq!(request["method"], "network.disownData");
        assert_eq!(
            request["params"],
            json!({"request":"unavailable","dataType":"response","collector":"collector"})
        );
        socket
            .send(Message::Text(
                json!({"type":"success","id":request["id"],"result":{}})
                    .to_string()
                    .into(),
            ))
            .unwrap();
    });
    let mut connection = BidiConnection::connect(address, Duration::from_secs(2)).unwrap();
    assert!(matches!(
        read_response(&mut connection, "collector", &candidate),
        Err(ProtocolError::Rejected)
    ));
    drop(connection);
    server.join().unwrap();
}
