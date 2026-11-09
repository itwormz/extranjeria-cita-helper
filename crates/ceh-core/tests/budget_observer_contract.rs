//! Python 0.3.6 budget observation parity. Scenarios: tests/contracts/budget_observer_reference.py.
#[path = "support/budget_replay.rs"]
#[allow(dead_code)]
mod budget_replay;

use ceh_core::appointment_budget::{AppointmentBudget, AppointmentPolicy};
use ceh_core::budget_observer::{BudgetObserver, ObservedPage};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../tests/contracts/fixtures/budget-observer-0.3.6.json")).unwrap()
}

fn flags(observer: &BudgetObserver) -> Value {
    json!({
        "hot_retry": observer.hot_retry(), "awaiting_result": observer.awaiting_result(),
        "round_saw_banner": observer.round_saw_banner(), "round_saw_office": observer.round_saw_office(),
        "banner_visible": observer.banner_visible(), "used_hot_retry": observer.used_hot_retry(),
    })
}

#[test]
fn observation_scenarios_match_python() {
    for scenario in fixture()["scenarios"].as_array().unwrap() {
        let name = scenario["name"].as_str().unwrap();
        let policy: AppointmentPolicy = serde_json::from_value(scenario["policy"].clone()).unwrap();
        let mut budget = AppointmentBudget::default();
        let mut observer = BudgetObserver::default();
        for (index, step) in scenario["steps"].as_array().unwrap().iter().enumerate() {
            let at: DateTime<Utc> = step["at"].as_str().unwrap().parse().unwrap();
            let label = format!("{name} step {index} ({})", step["op"]);
            match step["op"].as_str().unwrap() {
                "request_sent" => observer.request_sent(&mut budget, at, &policy),
                "observe" => {
                    let page = ObservedPage {
                        kind: serde_json::from_value(step["kind"].clone()).unwrap(),
                        operation_code: step["operation_code"].as_bool().unwrap(),
                    };
                    observer.observe(&page, &mut budget, at, &policy);
                }
                "province" => observer.on_province(),
                "offices_seen" => {
                    let names = step["names"].as_array().unwrap().iter().map(|n| n.as_str().unwrap().to_owned()).collect();
                    observer.offices_seen(&mut budget, at, names);
                }
                other => panic!("unknown op {other}"),
            }
            let summary = serde_json::to_value(budget.summary(at, &policy, false)).unwrap();
            budget_replay::same(&step["summary"], &summary, &format!("{label} summary")).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(flags(&observer), step["flags"], "{label} flags");
        }
    }
}
