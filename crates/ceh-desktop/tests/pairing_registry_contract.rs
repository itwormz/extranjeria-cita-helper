use ceh_desktop::notification_pairing::{PairError, PairRegistry};
use ceh_services::notifications::{NotificationError, Pairing};
use serde_json::json;
#[test]
fn pairing_is_exclusive_per_bot_expired_and_cancelled_sessions_are_not_polled() {
    let mut registry = PairRegistry::default();
    let mut calls = 0;
    let ticket = registry
        .start("123456:FIXTURE", false, 1000, |_, now| {
            calls += 1;
            Pairing::with_code("fixture_bot", "FIXTURE_CODE", now)
        })
        .unwrap();
    assert_eq!(ticket.link, "https://t.me/fixture_bot?start=FIXTURE_CODE");
    assert!(matches!(
        registry.start("123456:FIXTURE", true, 1001, |_, _| panic!(
            "duplicate bot must not reach transport"
        )),
        Err(PairError::Busy)
    ));
    assert_eq!(calls, 1);
    registry.cancel(&ticket.id);
    assert!(matches!(
        registry.poll(&ticket.id, 1002, |_, _, _, _| panic!(
            "cancelled must not reach transport"
        )),
        Err(PairError::Missing)
    ));
    let ticket = registry
        .start("123456:FIXTURE", true, 2000, |_, now| {
            Pairing::with_code("fixture_bot", "GROUP_CODE", now)
        })
        .unwrap();
    assert_eq!(
        ticket.link,
        "https://t.me/fixture_bot?startgroup=GROUP_CODE"
    );
    assert!(matches!(
        registry.poll(&ticket.id, 122001, |_, _, _, _| panic!(
            "expired must not reach transport"
        )),
        Err(PairError::Expired)
    ));
}
#[test]
fn failed_begin_releases_bot_and_successful_recipient_ends_polling() {
    let mut registry = PairRegistry::default();
    assert!(
        registry
            .start("123456:FIXTURE", false, 0, |_, _| Err(
                NotificationError::ExistingWebhook
            ))
            .is_err()
    );
    let ticket = registry
        .start("123456:FIXTURE", false, 0, |_, now| {
            Pairing::with_code("fixture_bot", "CODE", now)
        })
        .unwrap();
    let recipient = registry
        .poll(&ticket.id, 100, |token, _, group, _| {
            assert_eq!(token, "123456:FIXTURE");
            assert!(!group);
            Ok(Some(
                json!({"id":"123456","label":"Fixture","type":"private","username":""}),
            ))
        })
        .unwrap()
        .unwrap();
    assert_eq!(recipient.id, "123456");
    assert!(matches!(
        registry.poll(&ticket.id, 101, |_, _, _, _| panic!("already complete")),
        Err(PairError::Missing)
    ));
}

#[test]
fn malformed_pairing_response_is_not_exposed_or_saved() {
    let mut registry = PairRegistry::default();
    assert!(matches!(
        registry.start("123456:FIXTURE", false, 0, |_, now| Pairing::with_code(
            &"a".repeat(5000),
            "CODE",
            now
        )),
        Err(PairError::Invalid)
    ));
    let ticket = registry
        .start("123456:FIXTURE", false, 0, |_, now| {
            Pairing::with_code("fixture_bot", "CODE", now)
        })
        .unwrap();
    assert!(matches!(
        registry.poll(&ticket.id, 1, |_, _, _, _| Ok(Some(
            json!({"id":"-","label":"Fixture","type":"private","username":""})
        ))),
        Err(PairError::Invalid)
    ));
}
