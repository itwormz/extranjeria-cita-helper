use ceh_browser::bidi::{BidiConnection, ProtocolError};
use serde_json::json;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener},
    thread,
    time::Duration,
};
use tungstenite::Message;

#[test]
fn correlation_preserves_events_and_requires_one_pending_command() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut ws = tungstenite::accept(stream).unwrap();
        let command = ws.read().unwrap().into_text().unwrap();
        let command: serde_json::Value = serde_json::from_str(&command).unwrap();
        ws.send(Message::Text(
            json!({"type":"event","method":"network.beforeRequestSent","params":{"fixture":true}})
                .to_string()
                .into(),
        ))
        .unwrap();
        ws.send(Message::Text(
            json!({"type":"success","id":command["id"],"result":{"fixture":"ok"}})
                .to_string()
                .into(),
        ))
        .unwrap();
    });
    let mut connection = BidiConnection::connect(address, Duration::from_secs(2)).unwrap();
    let ticket = connection.send("script.evaluate", json!({})).unwrap();
    assert_eq!(
        connection.send("script.evaluate", json!({})),
        Err(ProtocolError::PendingCommand)
    );
    assert_eq!(connection.receive(ticket).unwrap(), json!({"fixture":"ok"}));
    assert_eq!(connection.take_events().len(), 1);
    server.join().unwrap();
}
#[test]
fn timeout_poisoning_never_reuses_an_ambiguous_connection() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut ws = tungstenite::accept(stream).unwrap();
        ws.read().unwrap();
        thread::sleep(Duration::from_millis(250));
    });
    let mut connection = BidiConnection::connect(address, Duration::from_millis(50)).unwrap();
    let ticket = connection.send("script.callFunction", json!({})).unwrap();
    assert_eq!(connection.receive(ticket), Err(ProtocolError::Unavailable));
    assert_eq!(
        connection.send("script.callFunction", json!({})),
        Err(ProtocolError::Unavailable)
    );
    server.join().unwrap();
}
#[test]
fn remote_endpoint_is_rejected_before_connecting() {
    assert!(matches!(
        BidiConnection::connect(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), 9222),
            Duration::from_secs(1)
        ),
        Err(ProtocolError::InvalidEndpoint)
    ));
}

#[test]
fn event_overflow_fails_closed_instead_of_losing_navigation_facts() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut ws = tungstenite::accept(stream).unwrap();
        ws.read().unwrap();
        for _ in 0..257 {
            if ws
                .send(Message::Text(
                    json!({"type":"event","method":"network.beforeRequestSent","params":{}})
                        .to_string()
                        .into(),
                ))
                .is_err()
            {
                break;
            }
        }
    });
    let mut connection = BidiConnection::connect(address, Duration::from_secs(2)).unwrap();
    let ticket = connection.send("script.evaluate", json!({})).unwrap();
    assert_eq!(
        connection.receive(ticket),
        Err(ProtocolError::EventOverflow)
    );
    assert_eq!(connection.take_events().len(), 256);
    assert_eq!(
        connection.send("script.evaluate", json!({})),
        Err(ProtocolError::Unavailable)
    );
    server.join().unwrap();
}
#[test]
fn rejected_command_keeps_connection_but_wrong_correlation_poisons_it() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut ws = tungstenite::accept(stream).unwrap();
        let command: serde_json::Value =
            serde_json::from_str(&ws.read().unwrap().into_text().unwrap()).unwrap();
        ws.send(Message::Text(json!({"type":"error","id":command["id"],"error":"no such frame","message":"sensitive fixture must stay private"}).to_string().into())).unwrap();
        ws.read().unwrap();
        ws.send(Message::Text(
            json!({"type":"success","id":999,"result":{}})
                .to_string()
                .into(),
        ))
        .unwrap();
    });
    let mut connection = BidiConnection::connect(address, Duration::from_secs(2)).unwrap();
    assert_eq!(
        connection.call("script.evaluate", json!({})),
        Err(ProtocolError::Rejected)
    );
    assert_eq!(
        connection.call("script.evaluate", json!({})),
        Err(ProtocolError::InvalidData)
    );
    assert_eq!(
        connection.send("script.evaluate", json!({})),
        Err(ProtocolError::Unavailable)
    );
    server.join().unwrap();
}

use ceh_browser::bidi::sanitize_command;

#[test]
fn key_values_never_enter_the_command_log_only_counts_and_pauses() {
    let keys = json!({"context":"c","actions":[{"type":"key","id":"k","actions":[
        {"type":"keyDown","value":"Y"},{"type":"keyUp","value":"Y"},{"type":"pause","duration":70},{"type":"keyDown","value":"1"},{"type":"keyUp","value":"1"},{"type":"pause","duration":120}]}]});
    let summary = sanitize_command("input.performActions", &keys);
    let text = summary.to_string();
    assert!(!text.contains("\"Y\"") && !text.contains("\"1\""), "{text}");
    assert_eq!(summary["key_down"], 2);
    assert_eq!(summary["key_up"], 2);
    assert_eq!(summary["pause_ms"], 190);
    let pointer = json!({"context":"c","actions":[{"type":"pointer","id":"m","parameters":{"pointerType":"mouse"},"actions":[
        {"type":"pointerMove","duration":120,"x":0,"y":0,"origin":{"type":"element","element":{"sharedId":"SECRET-NODE"}}},
        {"type":"pointerDown","button":0},{"type":"pause","duration":60},{"type":"pointerUp","button":0}]}]});
    let summary = sanitize_command("input.performActions", &pointer);
    assert_eq!((summary["pointer_move"].as_u64(), summary["pointer_down"].as_u64(), summary["pointer_up"].as_u64()), (Some(1), Some(1), Some(1)));
    assert_eq!(summary["move_ms"], 120);
    assert_eq!(summary["pause_ms"], 60);
    assert!(!summary.to_string().contains("SECRET-NODE"));
}

#[test]
fn scripts_are_identified_by_a_stable_hash_and_length_without_arguments() {
    let call = |arg: &str| json!({"functionDeclaration":"(a) => a.value","arguments":[{"type":"string","value":arg}],"target":{"realm":"r"},"awaitPromise":false});
    let first = sanitize_command("script.callFunction", &call("SECRET-NIE"));
    let second = sanitize_command("script.callFunction", &call("OTHER"));
    assert_eq!(first, second, "arguments do not matter");
    assert_eq!(first["length"], "(a) => a.value".len());
    assert!(first["script_hash"].as_str().unwrap().len() == 16);
    assert!(!first.to_string().contains("SECRET"));
    let evaluate = sanitize_command("script.evaluate", &json!({"expression":"document.title","target":{"context":"c"}}));
    assert_eq!(evaluate["length"], "document.title".len());
    assert!(!evaluate.to_string().contains("document.title"));
}

#[test]
fn navigation_drops_the_query_and_other_commands_give_the_method_only() {
    let navigate = sanitize_command("browsingContext.navigate", &json!({"context":"c","url":"https://icp.administracionelectronica.gob.es/icpplus/index.html?token=SECRET#frag","wait":"none"}));
    assert_eq!(navigate["url"], "https://icp.administracionelectronica.gob.es/icpplus/index.html");
    assert_eq!(navigate["wait"], "none");
    assert!(!navigate.to_string().contains("SECRET"));
    assert_eq!(sanitize_command("network.addDataCollector", &json!({"dataTypes":["response"]})), json!({}));
    assert_eq!(sanitize_command("browsingContext.captureScreenshot", &json!({"context":"c"})), json!({}));
}

#[test]
fn every_command_is_logged_once_with_its_outcome_and_drained_once() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut ws = tungstenite::accept(stream).unwrap();
        for ok in [true, false] {
            let command: serde_json::Value = serde_json::from_str(&ws.read().unwrap().into_text().unwrap()).unwrap();
            let reply = if ok { json!({"type":"success","id":command["id"],"result":{}}) } else { json!({"type":"error","id":command["id"],"error":"no such node"}) };
            ws.send(Message::Text(reply.to_string().into())).unwrap();
        }
    });
    let mut connection = BidiConnection::connect(address, Duration::from_secs(2)).unwrap();
    assert!(connection.call("browsingContext.reload", json!({"context":"c","wait":"none"})).is_ok());
    assert!(connection.call("script.evaluate", json!({"expression":"1","target":{"context":"c"}})).is_err());
    let log = connection.take_command_log();
    assert_eq!(log.len(), 2);
    assert_eq!((log[0]["method"].as_str(), log[0]["ok"].as_bool()), (Some("browsingContext.reload"), Some(true)));
    assert_eq!((log[1]["method"].as_str(), log[1]["ok"].as_bool()), (Some("script.evaluate"), Some(false)));
    assert!(log[0]["t_ms"].as_i64().unwrap() > 1_700_000_000_000);
    assert!(log[0]["duration_ms"].is_u64());
    assert!(connection.take_command_log().is_empty());
    server.join().unwrap();
}

#[test]
fn navigation_urls_lose_path_parameters_and_credentials_too() {
    let navigate = |url: &str| sanitize_command("browsingContext.navigate", &json!({"context":"c","url":url,"wait":"none"}))["url"].clone();
    assert_eq!(navigate("https://icp.administracionelectronica.gob.es/icpplustieb/acEntrada;jsessionid=SECRET123?x=1"), "https://icp.administracionelectronica.gob.es/icpplustieb/acEntrada");
    assert_eq!(navigate("https://user:SECRET@icp.administracionelectronica.gob.es:8443/a;b=c/d"), "https://icp.administracionelectronica.gob.es:8443/a/d");
    assert_eq!(navigate("not a url SECRET"), "invalid");
}

#[test]
fn unknown_action_shapes_cannot_smuggle_text_into_the_summary() {
    let odd = json!({"context":"c","actions":[{"type":"key","id":"SECRET-ID","actions":[{"type":"keyDown","value":"SECRET-KEY","text":"SECRET-TEXT"},{"type":"mystery","value":"SECRET-OTHER","duration":"SECRET-DURATION"}],"extra":"SECRET-EXTRA"}],"origin":"SECRET-ORIGIN"});
    let text = sanitize_command("input.performActions", &odd).to_string();
    assert!(!text.contains("SECRET"), "{text}");
}
