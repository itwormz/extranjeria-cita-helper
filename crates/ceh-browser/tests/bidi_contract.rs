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
