//! Passive diagnostics of the traffic to the appointment site (Python `app/icp_network_journal.py`):
//! one row per request, response and failure, plus request counts per minute window on HTTP 429.
//! A row holds only the method, endpoint name, resource type and status: no query, header or body.
use serde_json::{Map, Value, json};
use std::collections::VecDeque;

const HOST: &str = "icp.administracionelectronica.gob.es";
const WINDOW_SECONDS: f64 = 300.0;

#[derive(Default)]
pub struct IcpNetworkJournal {
    sent: VecDeque<f64>,
}
/// The last path segment without a trailing slash or a `;jsessionid` suffix (Python `_flow_endpoint`).
fn endpoint(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    let segment = trimmed.rsplit('/').next().unwrap_or("");
    segment.split(';').next().unwrap_or("").to_owned()
}
/// Playwright's resource type names for the closest WebDriver BiDi request attributes.
fn resource_type(request: &Value) -> &'static str {
    match request["destination"].as_str().unwrap_or("") {
        "document" | "iframe" | "frame" => "document",
        "script" => "script",
        "style" => "stylesheet",
        "image" => "image",
        "font" => "font",
        "" => match request["initiatorType"].as_str().unwrap_or("") {
            "xmlhttprequest" => "xhr",
            "fetch" => "fetch",
            _ => "other",
        },
        _ => "other",
    }
}
fn names(list: &Value, key: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in list.as_array().into_iter().flatten() {
        if let Some(name) = item[key].as_str() {
            let name = name.to_owned();
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}
impl IcpNetworkJournal {
    /// Names only, for the document and script-initiated traffic of the site: which headers and cookies a
    /// request carried and which cookies a response set. Values, queries and bodies never enter the row.
    pub fn meta(&self, event: &Value, wall: impl FnOnce(Option<i64>) -> String) -> Option<Value> {
        let kind = match event["method"].as_str()? {
            "network.beforeRequestSent" => "request",
            "network.responseCompleted" => "response",
            _ => return None,
        };
        let request = &event["params"]["request"];
        let url = url::Url::parse(request["url"].as_str()?).ok()?;
        if url.host_str() != Some(HOST) || !matches!(resource_type(request), "document" | "xhr" | "fetch") {
            return None;
        }
        let lower = |list: Vec<String>| {
            let mut out: Vec<String> = Vec::new();
            for name in list.into_iter().map(|n| n.to_ascii_lowercase()) {
                if !out.contains(&name) {
                    out.push(name);
                }
            }
            out
        };
        let mut row = Map::new();
        row.insert("time".into(), json!(wall(event["params"]["timestamp"].as_i64())));
        row.insert("event".into(), json!(kind));
        row.insert("method".into(), json!(request["method"].as_str().unwrap_or("")));
        row.insert("endpoint".into(), json!(endpoint(url.path())));
        row.insert("resource_type".into(), json!(resource_type(request)));
        row.insert("request_header_names".into(), json!(lower(names(&request["headers"], "name"))));
        row.insert("request_cookie_names".into(), json!(names(&request["cookies"], "name")));
        if kind == "response" {
            let response = &event["params"]["response"];
            row.insert("status".into(), json!(response["status"].as_u64().unwrap_or(0)));
            row.insert("response_header_names".into(), json!(lower(names(&response["headers"], "name"))));
            let set_cookie: Vec<String> = response["headers"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|h| h["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case("set-cookie")))
                .filter_map(|h| h["value"]["value"].as_str())
                .filter_map(|v| v.split(['=', ';']).next())
                .map(|n| n.trim().to_owned())
                .collect();
            let mut unique: Vec<String> = Vec::new();
            for name in set_cookie {
                if !unique.contains(&name) {
                    unique.push(name);
                }
            }
            row.insert("set_cookie_names".into(), json!(unique));
        }
        Some(Value::Object(row))
    }
    pub fn new() -> Self {
        Self::default()
    }
    fn prune(&mut self, now: f64) {
        while self.sent.front().is_some_and(|sent| *sent <= now - WINDOW_SECONDS) {
            self.sent.pop_front();
        }
    }
    /// The row for a network event of the appointment site, or `None` for anything else.
    /// `now` is monotonic seconds, `wall` the local ISO time of the event.
    pub fn record(&mut self, event: &Value, now: f64, wall: &str) -> Option<Value> {
        self.record_with(event, now, |_| wall.to_owned())
    }
    /// Like `record`, but the wall-clock text is only produced for a row that is actually written, from the
    /// event's own instant (BiDi `params.timestamp`, ms since the epoch) when the browser reports one.
    pub fn record_with(&mut self, event: &Value, now: f64, wall: impl FnOnce(Option<i64>) -> String) -> Option<Value> {
        let kind = match event["method"].as_str()? {
            "network.beforeRequestSent" => "request",
            "network.responseCompleted" => "response",
            "network.fetchError" => "failed",
            _ => return None,
        };
        let request = &event["params"]["request"];
        let url = url::Url::parse(request["url"].as_str()?).ok()?;
        if url.host_str() != Some(HOST) {
            return None;
        }
        let mut row = Map::new();
        row.insert("time".into(), json!(wall(event["params"]["timestamp"].as_i64())));
        row.insert("event".into(), json!(kind));
        row.insert("method".into(), json!(request["method"].as_str().unwrap_or("")));
        row.insert("endpoint".into(), json!(endpoint(url.path())));
        row.insert("resource_type".into(), json!(resource_type(request)));
        match kind {
            "request" => {
                self.prune(now);
                self.sent.push_back(now);
                row.insert("redirected".into(), json!(event["params"]["redirectCount"].as_u64().unwrap_or(0) > 0));
            }
            "response" => {
                let status = event["params"]["response"]["status"].as_u64()?;
                row.insert("status".into(), json!(status));
                if status == 429 {
                    self.prune(now);
                    let counts: Map<String, Value> = (1..=5)
                        .map(|minute| {
                            let within = self.sent.iter().filter(|sent| now - **sent < f64::from(minute) * 60.0).count();
                            (minute.to_string(), json!(within))
                        })
                        .collect();
                    row.insert("requests_by_minutes".into(), Value::Object(counts));
                }
            }
            _ => {}
        }
        Some(Value::Object(row))
    }
}
