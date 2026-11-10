use ceh_browser::icp_journal::IcpNetworkJournal;
use serde_json::{Value, json};

fn request(step: &Value) -> Value {
    let (destination, initiator) = match step["resource_type"].as_str().unwrap() {
        "document" => ("document", "other"),
        "script" => ("script", "parser"),
        "stylesheet" => ("style", "parser"),
        "image" => ("image", "parser"),
        "font" => ("font", "parser"),
        "xhr" => ("", "xmlhttprequest"),
        "fetch" => ("", "fetch"),
        _ => ("", "other"),
    };
    json!({"request":"r1","url":step["url"],"method":step["method"],"destination":destination,"initiatorType":initiator})
}
fn event(step: &Value) -> Value {
    match step["op"].as_str().unwrap() {
        "request" => json!({"type":"event","method":"network.beforeRequestSent","params":{"context":"c","redirectCount":if step["redirected"] == true {1} else {0},"request":request(step)}}),
        "response" => json!({"type":"event","method":"network.responseCompleted","params":{"context":"c","redirectCount":0,"request":request(step),"response":{"url":step["url"],"status":step["status"]}}}),
        _ => json!({"type":"event","method":"network.fetchError","params":{"context":"c","redirectCount":0,"request":request(step),"errorText":"NS_ERROR_NET_RESET"}}),
    }
}

#[test]
fn network_journal_rows_match_python() {
    let data: Value = serde_json::from_str(include_str!("../../../tests/contracts/fixtures/network-journal-0.3.6.json")).unwrap();
    let wall = data["wall"].as_str().unwrap();
    for scenario in data["scenarios"].as_array().unwrap() {
        let mut journal = IcpNetworkJournal::new();
        let mut rows = Vec::new();
        for step in scenario["steps"].as_array().unwrap() {
            if let Some(row) = journal.record(&event(step), step["at"].as_f64().unwrap(), wall) {
                rows.push(row);
            }
        }
        assert_eq!(Value::Array(rows), scenario["rows"], "{}", scenario["name"]);
    }
}

#[test]
fn rows_never_carry_query_strings_headers_or_bodies() {
    let mut journal = IcpNetworkJournal::new();
    let event = json!({"type":"event","method":"network.beforeRequestSent","params":{"context":"c","redirectCount":0,"request":{"request":"r1","url":"https://icp.administracionelectronica.gob.es/icpplus/acCitar?nie=Y1234567X&name=PRIVATE","method":"POST","destination":"document","initiatorType":"other","headers":[{"name":"cookie","value":{"type":"string","value":"SECRET"}}],"body":"PRIVATE"}}});
    let row = journal.record(&event, 0.0, "2026-11-10T10:00:00+01:00").unwrap();
    let text = row.to_string();
    assert!(!text.contains("Y1234567X") && !text.contains("PRIVATE") && !text.contains("SECRET"), "{text}");
    assert_eq!(row["endpoint"], "acCitar");
}
