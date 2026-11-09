use ceh_core::{journal::*, model::*};
use serde_json::{Value, json};
fn stage(value: &Value) -> PageKind {
    serde_json::from_value(value.clone()).unwrap()
}
fn document(value: &Value) -> DocumentId {
    DocumentId::new(value.as_str().unwrap()).unwrap()
}
#[test]
fn journal_matches_existing_python_stable_facts() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/fixtures/journal.json"
    ))
    .unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 18);
    for case in fixture["cases"].as_array().unwrap() {
        let mut journal = PageJournal::new();
        for (i, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let at = 1792454400000 + i as i64 * 1000;
            if let Some(v) = step.get("observe") {
                journal
                    .observe(
                        document(&v["document"]),
                        stage(&v["stage"]),
                        v["applicant_continue"] == true,
                        at,
                    )
                    .unwrap();
            } else if let Some(v) = step.get("event") {
                journal
                    .event(
                        serde_json::from_value(v["event"].clone()).unwrap(),
                        v.get("document").map(document),
                        v.get("stage").map(stage),
                        v["user_requested"] == true,
                        at,
                    )
                    .unwrap();
            } else if let Some(v) = step.get("phase") {
                journal
                    .phase(
                        serde_json::from_value(v["mode"].clone()).unwrap(),
                        v["remaining"].as_f64(),
                        at,
                    )
                    .unwrap();
            } else if let Some(v) = step.get("offices") {
                journal.offices(
                    v["options"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|o| o["text"].as_str().unwrap().to_owned())
                        .collect(),
                    v["selected"].as_str().map(str::to_owned),
                );
            } else if let Some(v) = step.get("booking") {
                journal.booking(&document(&v["document"]), &v["details"]);
            } else {
                panic!("unknown journal fixture operation");
            }
        }
        let actual:Vec<Value>=journal.rows().iter().map(|r|json!({"id":r.id,"time":r.time,"ended_at":r.ended_at,"event":r.event,"stage":r.stage.map(|s|serde_json::to_value(s).unwrap()).unwrap_or(json!("")),"document":r.document.as_ref().map(DocumentId::as_str).unwrap_or(""),"status":r.status,"destination":r.destination.map(|s|serde_json::to_value(s).unwrap()).unwrap_or(json!("")),"submitted":r.submitted,"checked":r.checked,"offices":r.offices,"selected_office":r.selected_office,"applicant_continue":r.applicant_continue,"booking_details":r.booking_details})).collect();
        assert_eq!(json!(actual), case["expected"], "{}", case["id"]);
    }
}
#[test]
fn repeated_observations_and_enrichment_preserve_booking_identity_and_time() {
    let mut journal = PageJournal::new();
    let d = DocumentId::new("booking").unwrap();
    journal
        .observe(d.clone(), PageKind::Booking, false, 1000)
        .unwrap();
    for at in 1001..1005 {
        journal
            .observe(d.clone(), PageKind::Booking, false, at)
            .unwrap();
    }
    assert!(journal.booking(&d,&json!({"office":"CNP TEST","offers":[{"date":"2026-10-28","time":"12:30"}],"token":"NOT_PUBLIC"})));
    assert_eq!(journal.rows().len(), 1);
    assert_eq!(journal.rows()[0].id, 1);
    assert_eq!(journal.rows()[0].time, 1000);
    assert!(
        !serde_json::to_string(&journal.rows()[0].booking_details)
            .unwrap()
            .contains("NOT_PUBLIC")
    );
    assert!(!journal.booking(
        &DocumentId::new("foreign").unwrap(),
        &json!({"office":"CNP OTHER"})
    ));
}

#[test]
fn failed_later_details_do_not_erase_known_booking() {
    let mut journal = PageJournal::new();
    let d = DocumentId::new("booking").unwrap();
    journal
        .observe(d.clone(), PageKind::Booking, false, 1000)
        .unwrap();
    journal.booking(&d, &json!({"office":"CNP TEST"}));
    assert!(!journal.booking(&d, &json!({})));
    assert_eq!(
        journal.rows()[0].booking_details.office.as_deref(),
        Some("CNP TEST")
    );
}
#[test]
fn bounded_control_history_preserves_active_visit() {
    let mut journal = PageJournal::new();
    let d = DocumentId::new("doc").unwrap();
    journal
        .observe(d.clone(), PageKind::Applicant, false, 0)
        .unwrap();
    for at in 1..=300 {
        journal
            .event(JournalEvent::BrowserShown, None, None, false, at)
            .unwrap();
    }
    assert_eq!(journal.rows().len(), 200);
    assert_eq!(journal.rows()[0].id, 1);
    journal
        .event(
            JournalEvent::ActionCompleted,
            Some(d),
            Some(PageKind::Applicant),
            false,
            301,
        )
        .unwrap();
    assert!(journal.rows()[0].submitted);
    assert!(serde_json::from_value::<JournalEvent>(json!("capture")).is_err());
}

#[test]
fn wall_clock_correction_does_not_disable_journal_updates() {
    let mut journal = PageJournal::new();
    let d = DocumentId::new("doc").unwrap();
    journal
        .observe(d.clone(), PageKind::Province, false, 1000)
        .unwrap();
    journal
        .event(
            JournalEvent::ActionCompleted,
            Some(d),
            Some(PageKind::Province),
            false,
            900,
        )
        .unwrap();
    journal
        .observe(
            DocumentId::new("new").unwrap(),
            PageKind::Procedure,
            false,
            800,
        )
        .unwrap();
    assert!(journal.rows()[0].submitted);
    assert_eq!(journal.rows()[0].ended_at, Some(800));
    assert_eq!(journal.rows()[1].time, 800);
}
