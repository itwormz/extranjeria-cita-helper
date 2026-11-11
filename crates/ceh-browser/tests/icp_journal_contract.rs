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
    let data: Value = serde_json::from_str(include_str!("../../../contracts/fixtures/network-journal-0.3.6.json")).unwrap();
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

fn site_event(method: &str, timestamp: Option<i64>, destination: &str, headers: serde_json::Value, extra: serde_json::Value) -> serde_json::Value {
    let mut params = serde_json::json!({"context":"ctx","redirectCount":0,"request":{
        "request":"r","method":"POST","url":"https://icp.administracionelectronica.gob.es/icpplustieb/acInfo?secret=value","destination":destination,"initiatorType":null,
        "headers":headers,"cookies":[{"name":"JSESSIONID","value":{"type":"string","value":"SECRET-COOKIE"}},{"name":"TS01abc","value":{"type":"string","value":"SECRET-TS"}}]}});
    if let Some(timestamp) = timestamp { params["timestamp"] = serde_json::json!(timestamp); }
    if let (Some(target), Some(extra)) = (params.as_object_mut(), extra.as_object()) { for (k, v) in extra { target.insert(k.clone(), v.clone()); } }
    serde_json::json!({"type":"event","method":method,"params":params})
}
fn header(name: &str, value: &str) -> serde_json::Value { serde_json::json!({"name":name,"value":{"type":"string","value":value}}) }

#[test]
fn a_row_carries_the_time_of_the_event_not_of_the_drain() {
    let mut journal = ceh_browser::icp_journal::IcpNetworkJournal::new();
    let seen = std::cell::RefCell::new(Vec::new());
    for timestamp in [1_000_i64, 2_500] {
        let event = site_event("network.beforeRequestSent", Some(timestamp), "document", serde_json::json!([]), serde_json::json!({}));
        let row = journal.record_with(&event, 0.0, |ms| { seen.borrow_mut().push(ms); format!("t{ms:?}") }).unwrap();
        assert_eq!(row["time"], format!("tSome({timestamp})"));
    }
    // Without a timestamp the caller's own clock is the only source.
    let event = site_event("network.beforeRequestSent", None, "document", serde_json::json!([]), serde_json::json!({}));
    assert_eq!(journal.record_with(&event, 0.0, |ms| format!("t{ms:?}")).unwrap()["time"], "tNone");
}

#[test]
fn metadata_rows_hold_header_and_cookie_names_never_values_or_queries() {
    let journal = ceh_browser::icp_journal::IcpNetworkJournal::new();
    let request = site_event("network.beforeRequestSent", Some(5), "document", serde_json::json!([header("User-Agent", "SECRET-UA"), header("Referer", "https://x/?q=SECRET")]), serde_json::json!({}));
    let row = journal.meta(&request, |ms| format!("t{ms:?}")).unwrap();
    assert_eq!(row["event"], "request");
    assert_eq!(row["endpoint"], "acInfo");
    assert_eq!(row["request_header_names"], serde_json::json!(["user-agent", "referer"]));
    assert_eq!(row["request_cookie_names"], serde_json::json!(["JSESSIONID", "TS01abc"]));
    let response = site_event("network.responseCompleted", Some(9), "document", serde_json::json!([header("User-Agent", "SECRET-UA")]),
        serde_json::json!({"response":{"status":200,"headers":[header("Content-Type", "text/html"), header("Set-Cookie", "TSPD_101=SECRET-TOKEN; Path=/; Secure"), header("set-cookie", "JSESSIONID=SECRET; HttpOnly")]}}));
    let row = journal.meta(&response, |ms| format!("t{ms:?}")).unwrap();
    assert_eq!(row["status"], 200);
    assert_eq!(row["response_header_names"], serde_json::json!(["content-type", "set-cookie"]));
    assert_eq!(row["set_cookie_names"], serde_json::json!(["TSPD_101", "JSESSIONID"]));
    let text = row.to_string();
    for secret in ["SECRET", "secret=", "?"] {
        assert!(!text.contains(secret), "{secret} leaked: {text}");
    }
    // Static resources are not part of the metadata stream.
    let script = site_event("network.beforeRequestSent", Some(5), "script", serde_json::json!([]), serde_json::json!({}));
    assert!(journal.meta(&script, |_| String::new()).is_none());
}

#[test]
fn odd_set_cookie_values_never_become_names() {
    let journal = ceh_browser::icp_journal::IcpNetworkJournal::new();
    let response = |headers: serde_json::Value| site_event("network.responseCompleted", Some(9), "document", serde_json::json!([]), serde_json::json!({"response":{"status":200,"headers":headers}}));
    let odd = serde_json::json!([
        header("Set-Cookie", "abc123def456; Path=/"),
        {"name":"Set-Cookie","value":{"type":"base64","value":"U0VDUkVULVRPS0VO"}},
        header("Set-Cookie", "bad name=SECRET"),
        header("Set-Cookie", &format!("{}=SECRET", "x".repeat(200))),
        header("Set-Cookie", "good_1=SECRET; Secure"),
    ]);
    let row = journal.meta(&response(odd), |ms| format!("t{ms:?}")).unwrap();
    assert_eq!(row["set_cookie_names"], serde_json::json!(["good_1"]));
    assert!(!row.to_string().contains("SECRET") && !row.to_string().contains("U0VDUkVU") && !row.to_string().contains("abc123"));
}
