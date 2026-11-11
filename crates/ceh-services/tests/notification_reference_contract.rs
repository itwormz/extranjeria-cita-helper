//! Python 0.3.6 notification parity. Scenarios: tests/contracts/notifications_reference.py.
use ceh_services::notifications::{
    EventRouter, NotificationPlanner, make_event_with_metadata, state_event_with, telegram_message,
};
use serde_json::{Value, json};

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../contracts/fixtures/notifications-0.3.6.json")).unwrap()
}

fn expected(value: &Value) -> Option<&str> {
    value.as_str()
}

#[test]
fn state_events_match_python() {
    for (index, case) in fixture()["state_events"].as_array().unwrap().iter().enumerate() {
        let actual = state_event_with(&case["state"], case["include_transient"].as_bool().unwrap());
        assert_eq!(actual, expected(&case["expect"]), "case {index}: {}", case["state"]);
    }
}

#[test]
fn routers_match_python() {
    for scenario in fixture()["routers"].as_array().unwrap() {
        let mut router = EventRouter::default();
        for (index, step) in scenario["steps"].as_array().unwrap().iter().enumerate() {
            assert_eq!(
                router.observe(&step["state"]),
                expected(&step["expect"]),
                "{} step {index}",
                scenario["name"]
            );
        }
    }
}

#[test]
fn telegram_messages_match_python() {
    for case in fixture()["telegram"].as_array().unwrap() {
        let language = case["language"].as_str().unwrap();
        let event = make_event_with_metadata(case["event"].as_str().unwrap(), "Demo", "Madrid", language, "id", "ts").unwrap();
        assert_eq!(event["message"], case["plain"], "{} plain message ({language})", case["name"]);
        let details = if case["details"].is_null() { json!(null) } else { case["details"].clone() };
        let text = telegram_message(&event, language, &details, &case["offices"]);
        assert_eq!(text, case["expect"].as_str().unwrap(), "{} ({language})", case["name"]);
    }
}

#[test]
fn pairing_lifetime_in_python_is_300_seconds() {
    assert_eq!(fixture()["pairing_lifetime_seconds"], 300);
}

fn enabled_telegram() -> Value {
    json!({"telegram":{"enabled":true,"events":["office","booking"],"recipients":[{"id":"1"}],"secret_ref":"ref"},"webhook":{"enabled":false}})
}
fn office_state(document: &str, mode: &str, screen: &str) -> Value {
    json!({"run_id":"run","document":document,"mode":mode,"screen":screen,"stage":"office","available_offices":["Oficina A","Oficina B"]})
}

#[test]
fn planner_does_not_repeat_office_alert_for_the_same_document() {
    let mut planner = NotificationPlanner::default();
    let config = enabled_telegram();
    let mut sent = 0;
    for state in [
        office_state("d1", "manual", "Выбор офиса"),
        office_state("d1", "working", "Проверяем страницу"),
        office_state("d1", "manual", "Выбор офиса"),
        office_state("d1", "working", "Проверяем страницу"),
        office_state("d1", "manual", "Выбор офиса"),
    ] {
        let intents = planner.observe("profile", "Demo", "Madrid", &state, "en", &config).unwrap();
        for intent in &intents {
            assert_eq!(intent.channel(), "telegram");
            assert!(intent.message().contains("Oficina A") && intent.message().contains("Oficina B"), "{}", intent.message());
        }
        sent += intents.len();
    }
    assert_eq!(sent, 1);
}

#[test]
fn planner_alerts_again_for_a_new_office_document() {
    let mut planner = NotificationPlanner::default();
    let config = enabled_telegram();
    let first = planner.observe("profile", "Demo", "Madrid", &office_state("d1", "manual", "Выбор офиса"), "en", &config).unwrap();
    let second = planner.observe("profile", "Demo", "Madrid", &office_state("d2", "manual", "Выбор офиса"), "en", &config).unwrap();
    assert_eq!((first.len(), second.len()), (1, 1));
    assert!(!first[0].cancellation().current(), "a new office visit cancels pending deliveries of the old one");
    assert!(second[0].cancellation().current());
}
