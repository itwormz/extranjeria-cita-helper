use ceh_core::{control::ControlMode, model::*, statistics::*};
use serde_json::{Value, json};
fn scope(profile: &str, session: &str) -> EventScope {
    EventScope {
        profile: ProfileId::new(profile).unwrap(),
        session: SessionId::new(session).unwrap(),
    }
}
#[test]
fn counters_match_existing_python_session_and_store_facts() {
    let data: Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/fixtures/counters.json"
    ))
    .unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 6);
    for case in data["cases"].as_array().unwrap() {
        let mut ledger = StatisticsLedger::new();
        for (i, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let context = scope(
                step["profile"].as_str().unwrap_or("fixture"),
                step["session"].as_str().unwrap_or("session"),
            );
            if let Some(page) = step.get("page") {
                ledger.observe_page(&context, serde_json::from_value(page.clone()).unwrap());
                continue;
            }
            let v = &step["observe"];
            let kind: CounterKind = serde_json::from_value(v["kind"].clone()).unwrap();
            let token = v["identity"].as_str().unwrap();
            let identity = if matches!(kind, CounterKind::AutomationAction | CounterKind::Check) {
                EvidenceId::Operation(OperationId::new(token).unwrap())
            } else {
                EvidenceId::Document(DocumentId::new(token).unwrap())
            };
            ledger
                .record(CounterObservation {
                    key: EventKey {
                        scope: context,
                        kind,
                        identity,
                    },
                    at: 1792454400000 + i as i64 * 1000,
                    offices: v["offices"]
                        .as_array()
                        .map(|a| a.iter().map(|s| s.as_str().unwrap().to_owned()).collect())
                        .unwrap_or_default(),
                    mode: ControlMode::Automatic,
                    automated: true,
                })
                .unwrap();
        }
        for (profile, expected) in case["expected"].as_object().unwrap() {
            let id = ProfileId::new(profile).unwrap();
            assert_eq!(
                json!({"totals":ledger.totals(&id),"known_offices":ledger.known_offices(&id)}),
                *expected,
                "{} {profile}",
                case["id"]
            );
        }
    }
}
#[test]
fn booking_enrichment_is_exact_identity_and_never_retimes_or_recounts() {
    let mut ledger = StatisticsLedger::new();
    let key = EventKey {
        scope: scope("fixture", "s"),
        kind: CounterKind::Booking,
        identity: EvidenceId::Document(DocumentId::new("booking").unwrap()),
    };
    let raw = json!({"office":"CNP TEST","offers":[{"date":"2026-10-28","time":"12:30"}],"confirmation":"PRIVATE"});
    assert!(!ledger.enrich_booking(&key.scope, &DocumentId::new("booking").unwrap(), &raw));
    ledger
        .record(CounterObservation {
            key: key.clone(),
            at: 1000,
            offices: vec![],
            mode: ControlMode::Manual,
            automated: false,
        })
        .unwrap();
    let before = ledger.totals(&key.scope.profile);
    assert!(ledger.enrich_booking(&key.scope, &DocumentId::new("booking").unwrap(), &raw));
    assert!(!ledger.enrich_booking(
        &scope("other", "s"),
        &DocumentId::new("booking").unwrap(),
        &raw
    ));
    assert!(!ledger.enrich_booking(
        &scope("fixture", "other"),
        &DocumentId::new("booking").unwrap(),
        &raw
    ));
    assert_eq!(ledger.events().len(), 1);
    assert_eq!(ledger.events()[0].at, 1000);
    assert_eq!(ledger.totals(&key.scope.profile), before);
    assert!(
        !serde_json::to_string(ledger.events())
            .unwrap()
            .contains("PRIVATE")
    );
}
#[test]
fn effort_counters_do_not_count_paused_or_manual_operations() {
    for mode in [ControlMode::Paused, ControlMode::Manual] {
        for kind in [CounterKind::AutomationAction, CounterKind::ProcessedPage] {
            let mut ledger = StatisticsLedger::new();
            let identity = if kind == CounterKind::AutomationAction {
                EvidenceId::Operation(OperationId::new("op").unwrap())
            } else {
                EvidenceId::Document(DocumentId::new("doc").unwrap())
            };
            assert!(
                !ledger
                    .record(CounterObservation {
                        key: EventKey {
                            scope: scope("fixture", "s"),
                            kind,
                            identity
                        },
                        at: 0,
                        offices: vec![],
                        mode,
                        automated: true
                    })
                    .unwrap()
            );
            assert!(ledger.events().is_empty());
        }
    }
}
