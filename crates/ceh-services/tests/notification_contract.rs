use ceh_services::notifications::{
    EventRouter, NotificationError, Pairing, state_event, webhook_request,
};
use serde_json::json;
#[test]
fn router_waits_for_manual_and_preserves_dedup_on_disconnect() {
    let mut router = EventRouter::default();
    let state = json!({"mode":"manual","screen":"Выбор офиса","run_id":"one"});
    assert_eq!(router.observe(&state), Some("office"));
    assert_eq!(router.observe(&state), None);
    assert_eq!(router.observe(&json!({"connection_lost":true})), None);
    assert_eq!(router.observe(&state), None);
    assert_eq!(
        state_event(&json!({"failed":true,"paused":true})),
        Some("error")
    );
}
#[test]
fn webhook_encodes_query_and_escapes_custom_json() {
    let event = json!({"message":"A\"B","event":"booking"});
    let config = json!({"url":"https://example.test/hook?first=1","method":"GET","query":[{"name":"message","value":"{{message}}"}]});
    let request = webhook_request(&config, &event).unwrap();
    assert!(request.url().contains("message=A%22B"));
    assert!(request.body().is_none());
    let config = json!({"url":"https://example.test","body_mode":"custom","body":"{\"text\":\"{{message}}\"}"});
    let request = webhook_request(&config, &event).unwrap();
    let body: serde_json::Value = serde_json::from_slice(request.body().unwrap()).unwrap();
    assert_eq!(body["text"], "A\"B");
}
#[test]
fn invalid_webhook_is_rejected_without_secret_echo() {
    for config in [
        json!({"url":"http://secret.test/TOKEN"}),
        json!({"url":"https://u:p@secret.test/TOKEN"}),
        json!({"url":"https://example.test","headers":[{"name":"Host","value":"TOKEN"}]}),
        json!({"url":"https://example.test","headers":[{"name":"X-A","value":"TOKEN\r\n"}]}),
    ] {
        assert!(matches!(
            webhook_request(&config, &json!({})),
            Err(NotificationError::InvalidConfig)
        ));
    }
}
#[test]
fn pairing_matches_code_type_and_expires() {
    let mut pair = Pairing::with_code("demo_bot", "FIXTURE_CODE", 10_000).unwrap();
    let updates = json!([{"update_id":1,"message":{"text":"/start wrong","chat":{"id":1,"type":"private"}}},{"update_id":2,"message":{"text":"/start FIXTURE_CODE","chat":{"id":2,"type":"private","first_name":"Demo"}}}]);
    assert_eq!(
        pair.match_updates(&updates, false, 20_000).unwrap()["id"],
        "2"
    );
    assert_eq!(pair.offset(), Some(3));
    assert!(pair.match_updates(&updates, false, 130_001).is_none());
}

#[test]
fn notification_payloads_match_python_in_three_languages() {
    use ceh_services::notifications::{make_event_with_metadata, telegram_message};
    let data: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/fixtures/notifications.json"
    ))
    .unwrap();
    for case in data["cases"].as_array().unwrap() {
        let input = &case["input"];
        let op = input["op"].as_str().unwrap();
        let event = make_event_with_metadata(
            input["kind"].as_str().unwrap_or("booking"),
            "Demo",
            "Barcelona",
            input["language"].as_str().unwrap_or("en"),
            "fixture-id",
            "2026-10-30T12:00:00+00:00",
        )
        .unwrap();
        let actual = match op {
            "message" => {
                json!({"text":telegram_message(&event,input["language"].as_str().unwrap_or("en"),&input["details"]),"event":event})
            }
            "router" => {
                let mut router = EventRouter::default();
                json!(
                    input["states"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|state| router.observe(state))
                        .collect::<Vec<_>>()
                )
            }
            "webhook" => match webhook_request(&input["config"], &event) {
                Err(_) => json!({"error":"invalid"}),
                Ok(request) => {
                    json!({"method":request.method(),"url":request.url(),"headers":request.headers().iter().map(|(key,value)|(key.to_ascii_lowercase(),value.clone())).collect::<std::collections::BTreeMap<_,_>>(),"body":request.body().map(|body|serde_json::from_slice::<serde_json::Value>(body).unwrap())})
                }
            },
            _ => panic!("unsupported fixture"),
        };
        assert_eq!(actual, case["expected"], "{}", case["id"]);
    }
}

#[test]
fn channel_planning_keeps_webhook_immediate_and_telegram_details_deferred() {
    use ceh_services::notifications::NotificationPlanner;
    let mut planner = NotificationPlanner::default();
    let config = json!({"telegram":{"enabled":true,"events":["booking"]},"webhook":{"enabled":true,"events":["booking"]}});
    let state = json!({"booking_handoff":true,"run_id":"one","booking_details_pending":true});
    let first = planner
        .observe("profile", "Demo", "Barcelona", &state, "en", &config)
        .unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].channel(), "webhook");
    let token = first[0].cancellation();
    assert!(token.current());
    let second = planner
        .observe(
            "profile",
            "Demo",
            "Barcelona",
            &json!({"booking_handoff":true,"run_id":"one","booking_details":{"office":"Office A"}}),
            "en",
            &config,
        )
        .unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].channel(), "telegram");
    assert!(second[0].message().contains("Office A"));
    planner
        .observe("other", "Other", "Barcelona", &state, "en", &config)
        .unwrap();
    assert!(!token.current());
}
