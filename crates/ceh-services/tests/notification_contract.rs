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
    assert!(pair.match_updates(&updates, false, 310_001).is_none());
}

#[test]
fn notification_payloads_match_python_in_three_languages() {
    use ceh_services::notifications::{make_event_with_metadata, telegram_message};
    let data: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/fixtures/notifications.json"
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
                json!({"text":telegram_message(&event,input["language"].as_str().unwrap_or("en"),&input["details"],&json!(null)),"event":event})
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

#[test]
fn pairing_expires_at_exactly_300_seconds_and_not_before() {
    let pairing = Pairing::with_code("fixture_bot", "FIXTURE_CODE", 1_000).unwrap();
    assert!(!pairing.expired(1_000 + 299_999));
    assert!(pairing.expired(1_000 + 300_000));
    assert_eq!(ceh_services::notifications::PAIRING_LIFETIME_SECONDS, 300);
}

#[test]
fn event_text_uses_the_selected_browser_name_in_all_languages() {
    use ceh_services::notifications::{make_event_with_browser, make_event_with_metadata};
    for language in ["ru", "en", "es"] {
        for kind in ["auth", "office", "booking"] {
            let python = make_event_with_metadata(kind, "Demo", "Madrid", language, "id", "ts").unwrap();
            assert!(python["message"].as_str().unwrap().contains("Camoufox"), "{kind} {language}");
            let event = make_event_with_browser(kind, "Demo", "Madrid", language, "Firefox").unwrap();
            let message = event["message"].as_str().unwrap();
            assert!(message.contains("Firefox") && !message.contains("Camoufox"), "{kind} {language}: {message}");
        }
    }
}

#[test]
fn planner_uses_the_browser_name_from_state_for_telegram_and_webhook() {
    use ceh_services::notifications::NotificationPlanner;
    let mut planner = NotificationPlanner::default();
    let config = json!({"telegram":{"enabled":true,"events":["office"],"recipients":[{"id":"1"}],"secret_ref":"a"},
                        "webhook":{"enabled":true,"events":["office"],"secret_ref":"b"}});
    let state = json!({"run_id":"r","document":"d","mode":"manual","screen":"Выбор офиса","stage":"office",
                       "available_offices":["Oficina A"],"browser":"Firefox"});
    let intents = planner.observe("profile", "Demo", "Madrid", &state, "en", &config).unwrap();
    assert_eq!(intents.len(), 2);
    for intent in &intents {
        assert!(intent.message().contains("Firefox") && !intent.message().contains("Camoufox"), "{}: {}", intent.channel(), intent.message());
    }
    let hostile = json!({"run_id":"r2","document":"d","mode":"manual","screen":"Выбор офиса","stage":"office","browser":"Fire\nfox"});
    let intents = planner.observe("profile", "Demo", "Madrid", &hostile, "en", &config).unwrap();
    assert!(intents.iter().all(|intent| intent.message().contains("Camoufox")), "an invalid browser name is ignored");
}

mod prepared {
    use ceh_services::notifications::{
        NotificationError, NotificationPlanner, deliver_intent_prepared, prepare_notifications, HttpDelivery,
    };
    use ceh_services::secrets::{Backend, SecretData, Vault, VaultError};
    use serde_json::{Value, json};
    use std::{cell::{Cell, RefCell}, collections::{BTreeMap, BTreeSet}};

    #[derive(Default)]
    struct Memory {
        values: RefCell<BTreeMap<String, String>>,
        read_keys: RefCell<BTreeSet<String>>,
        locked: Cell<bool>,
    }
    impl Backend for Memory {
        fn get(&self, _: &str, key: &str) -> Result<Option<String>, VaultError> {
            self.read_keys.borrow_mut().insert(key.into());
            if self.locked.get() {
                return Err(VaultError::Locked);
            }
            Ok(self.values.borrow().get(key).cloned())
        }
        fn set(&self, _: &str, key: &str, value: &str) -> Result<(), VaultError> {
            self.values.borrow_mut().insert(key.into(), value.into());
            Ok(())
        }
        fn delete(&self, _: &str, key: &str) -> Result<(), VaultError> {
            self.values.borrow_mut().remove(key);
            Ok(())
        }
    }
    fn stored(vault: &Vault<Memory>, secret: Value) -> String {
        vault.put(&SecretData::new(secret).unwrap()).unwrap()
    }
    fn config(telegram_ref: &str, webhook_ref: &str, webhook_enabled: bool, telegram_events: Value) -> Value {
        json!({"telegram":{"enabled":true,"events":telegram_events,"recipients":[{"id":"1"}],"secret_ref":telegram_ref},
               "webhook":{"enabled":webhook_enabled,"events":["error"],"secret_ref":webhook_ref}})
    }

    #[test]
    fn prepare_reads_only_enabled_channels_with_events() {
        let vault = Vault::new(Memory::default());
        let telegram = stored(&vault, json!({"token":"TELEGRAM-TOKEN"}));
        let webhook = stored(&vault, json!({"url":"https://example.test/hook"}));
        vault.backend().read_keys.borrow_mut().clear();
        let prepared = prepare_notifications("p", &config(&telegram, &webhook, false, json!(["office"])), &vault, None).unwrap();
        let read = vault.backend().read_keys.borrow().clone();
        assert!(read.iter().all(|key| key.starts_with(&telegram)), "only the telegram secret is read: {read:?}");
        assert!(prepared.has_secret(&telegram) && !prepared.has_secret(&webhook));
        vault.backend().read_keys.borrow_mut().clear();
        prepare_notifications("p", &config(&telegram, &webhook, true, json!([])), &vault, None).unwrap();
        let read = vault.backend().read_keys.borrow().clone();
        assert!(read.iter().all(|key| key.starts_with(&webhook)), "a channel without events is not read: {read:?}");
    }

    #[test]
    fn prepare_reports_the_failing_channel_without_secret_text() {
        let vault = Vault::new(Memory::default());
        let telegram = stored(&vault, json!({"token":"TELEGRAM-TOKEN"}));
        let webhook = stored(&vault, json!({"url":"https://example.test/hook"}));
        vault.backend().locked.set(true);
        let failure = prepare_notifications("p", &config(&telegram, &webhook, true, json!(["office"])), &vault, None).err().unwrap();
        assert_eq!(failure.channel, "telegram");
        assert_eq!(failure.error, NotificationError::SecretStorage);
        assert!(!format!("{failure:?}").contains("TELEGRAM-TOKEN"));
    }

    #[test]
    fn prepare_reuses_previous_secret_for_the_same_reference() {
        let vault = Vault::new(Memory::default());
        let telegram = stored(&vault, json!({"token":"TELEGRAM-TOKEN"}));
        let first = prepare_notifications("p", &config(&telegram, "", false, json!(["office"])), &vault, None).unwrap();
        vault.backend().locked.set(true);
        vault.backend().read_keys.borrow_mut().clear();
        let second = prepare_notifications("p", &config(&telegram, "", false, json!(["office"])), &vault, Some(&first)).unwrap();
        assert!(second.has_secret(&telegram));
        assert!(vault.backend().read_keys.borrow().is_empty(), "a retained secret does not touch the vault again");
        let other_profile = prepare_notifications("q", &config(&telegram, "", false, json!(["office"])), &vault, Some(&first));
        assert!(other_profile.is_err(), "secrets are not reused across profiles");
    }

    #[test]
    fn delivery_without_a_prepared_secret_fails_closed_and_skips_the_vault() {
        let vault = Vault::new(Memory::default());
        let telegram = stored(&vault, json!({"token":"TELEGRAM-TOKEN"}));
        let cfg = config(&telegram, "", false, json!(["office"]));
        let state = json!({"run_id":"r","document":"d","mode":"manual","screen":"Выбор офиса","stage":"office","available_offices":["A"]});
        let mut planner = NotificationPlanner::default();
        let intents = planner.observe("p", "Demo", "Madrid", &state, "en", &cfg).unwrap();
        assert_eq!(intents.len(), 1);
        vault.backend().locked.set(true);
        let empty = prepare_notifications("p", &json!({}), &vault, None).unwrap();
        vault.backend().read_keys.borrow_mut().clear();
        let http = HttpDelivery::new().unwrap();
        assert_eq!(deliver_intent_prepared(&intents[0], &empty, &http).err(), Some(NotificationError::SecretStorage));
        assert!(vault.backend().read_keys.borrow().is_empty());
    }
}
