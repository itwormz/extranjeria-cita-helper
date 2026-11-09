use ceh_browser::navigation::{NavigationFacts, ResponseFacts};
use ceh_core::model::RequestMethod;
use serde_json::json;
fn start(id: &str) -> serde_json::Value {
    json!({"method":"browsingContext.navigationStarted","params":{"context":"tab","navigation":id,"url":"https://example.invalid/same"}})
}
fn request(id: &str, request: &str, method: &str) -> serde_json::Value {
    json!({"method":"network.beforeRequestSent","params":{"context":"tab","navigation":id,"request":{"request":request,"method":method,"url":"https://example.invalid/same?secret=never-store"}}})
}
fn response(id: &str, request: &str, status: u16) -> serde_json::Value {
    json!({"method":"network.responseCompleted","params":{"context":"tab","navigation":id,"request":{"request":request},"response":{"status":status}}})
}
#[test]
fn same_url_new_navigation_cannot_inherit_old_post_or_429() {
    let mut facts = NavigationFacts::default();
    facts.ingest(&start("old")).unwrap();
    facts
        .ingest(&request("old", "request-old", "POST"))
        .unwrap();
    facts.ingest(&response("old", "request-old", 429)).unwrap();
    facts
        .ingest(
            &json!({"method":"browsingContext.load","params":{"context":"tab","navigation":"old"}}),
        )
        .unwrap();
    assert_eq!(
        facts.get("tab", "old"),
        ResponseFacts {
            method: RequestMethod::Post,
            status: Some(429)
        }
    );
    facts.ingest(&start("new")).unwrap();
    facts.ingest(&response("old", "request-old", 500)).unwrap();
    assert_eq!(facts.get("tab", "new"), ResponseFacts::default());
    assert_eq!(facts.get("tab", "old"), ResponseFacts::default());
    facts.ingest(&request("new", "request-new", "GET")).unwrap();
    facts.ingest(&response("new", "request-new", 200)).unwrap();
    facts
        .ingest(
            &json!({"method":"browsingContext.load","params":{"context":"tab","navigation":"new"}}),
        )
        .unwrap();
    assert_eq!(
        facts.get("tab", "new"),
        ResponseFacts {
            method: RequestMethod::Get,
            status: Some(200)
        }
    );
}
#[test]
fn resources_and_late_redirect_responses_cannot_override_document() {
    let mut facts = NavigationFacts::default();
    facts.ingest(&start("nav")).unwrap();
    facts.ingest(&request("nav", "redirect", "POST")).unwrap();
    facts.ingest(&request("nav", "final", "GET")).unwrap();
    facts.ingest(&response("nav", "redirect", 500)).unwrap();
    facts.ingest(&json!({"method":"network.beforeRequestSent","params":{"context":"tab","navigation":null,"request":{"request":"script","method":"GET"}}})).unwrap();
    facts.ingest(&response("nav", "final", 200)).unwrap();
    facts
        .ingest(
            &json!({"method":"browsingContext.load","params":{"context":"tab","navigation":"nav"}}),
        )
        .unwrap();
    assert_eq!(
        facts.get("tab", "nav"),
        ResponseFacts {
            method: RequestMethod::Get,
            status: Some(200)
        }
    );
}

#[test]
fn pending_navigation_cannot_attach_new_get_facts_to_old_post_document() {
    let mut facts = NavigationFacts::default();
    facts.ingest(&start("nav")).unwrap();
    facts.ingest(&request("nav", "new-get", "GET")).unwrap();
    facts.ingest(&response("nav", "new-get", 200)).unwrap();
    assert_eq!(facts.get("tab", "nav"), ResponseFacts::default());
    facts
        .ingest(
            &json!({"method":"browsingContext.load","params":{"context":"tab","navigation":"nav"}}),
        )
        .unwrap();
    assert_eq!(
        facts.get("tab", "nav"),
        ResponseFacts {
            method: RequestMethod::Get,
            status: Some(200)
        }
    );
}

#[test]
fn worker_requests_without_a_document_context_are_ignored() {
    let mut facts = NavigationFacts::default();
    assert_eq!(facts.ingest(&json!({"method":"network.beforeRequestSent","params":{"context":null,"navigation":null,"request":{"request":"worker","method":"GET"}}})),Ok(()));
    assert_eq!(facts.ingest(&json!({"method":"network.responseCompleted","params":{"context":null,"navigation":null,"request":{"request":"worker"},"response":{"status":200}}})),Ok(()));
}
