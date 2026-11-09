use ceh_core::model::PageKind;
use ceh_services::notifications::{NotificationStateInput, notification_state, state_event};
use serde_json::json;

fn input(page: PageKind) -> NotificationStateInput {
    NotificationStateInput {
        run_id: "run".into(),
        document: Some("doc".into()),
        page,
        ..Default::default()
    }
}

#[test]
fn notification_state_maps_page_kinds_to_python_screens() {
    for (page, screen) in [
        (PageKind::Office, "Выбор офиса"),
        (PageKind::Booking, "Выбор ситы"),
        (PageKind::Captcha, "Капча"),
        (PageKind::AuthWait, "Авторизация"),
        (PageKind::Auth, "Авторизация"),
    ] {
        let state = notification_state(&NotificationStateInput { attention: true, ..input(page) });
        assert_eq!((state["screen"].as_str(), state["mode"].as_str()), (Some(screen), Some("manual")), "{page:?}");
    }
    let quiet = notification_state(&input(PageKind::Province));
    assert_eq!((quiet["screen"].as_str(), quiet["mode"].as_str(), quiet["stage"].as_str()), (Some(""), Some("working"), Some("")));
}

#[test]
fn office_attention_becomes_an_office_event_and_quiet_office_inspection_does_not() {
    let attention = notification_state(&NotificationStateInput { attention: true, offices: vec!["A".into(), "B".into()], ..input(PageKind::Office) });
    assert_eq!(state_event(&attention), Some("office"));
    assert_eq!(attention["available_offices"], json!(["A", "B"]));
    assert_eq!(attention["document"], "doc");
    let quiet = notification_state(&NotificationStateInput { offices: vec!["A".into()], ..input(PageKind::Office) });
    assert_eq!(state_event(&quiet), None);
    assert_eq!((quiet["screen"].as_str(), quiet["stage"].as_str()), (Some("Проверяем страницу"), Some("office")));
}

#[test]
fn offices_are_exposed_only_for_the_office_page() {
    let state = notification_state(&NotificationStateInput { attention: true, offices: vec!["A".into()], ..input(PageKind::Booking) });
    assert_eq!(state["available_offices"], json!([]));
}

#[test]
fn booking_handoff_failure_and_details_pass_through() {
    let handoff = notification_state(&NotificationStateInput {
        booking_handoff: true,
        booking_details: Some(json!({"office":"X","offers":[]})),
        booking_details_pending: true,
        browser: Some("Firefox".into()),
        ..input(PageKind::Booking)
    });
    assert_eq!(state_event(&handoff), Some("booking"));
    assert_eq!(handoff["booking_details_pending"], true);
    assert_eq!(handoff["booking_details"]["office"], "X");
    assert_eq!(handoff["browser"], "Firefox");
    let failed = notification_state(&NotificationStateInput { failed: true, ..input(PageKind::Unknown) });
    assert_eq!(state_event(&failed), Some("error"));
}
