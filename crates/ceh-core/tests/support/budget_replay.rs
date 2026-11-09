//! Replays scenarios recorded from Python 0.3.6 (tests/contracts/budget_reference.py).
use ceh_core::appointment_budget::{AppointmentBudget, AppointmentPolicy};
use chrono::{DateTime, Utc};
use serde_json::Value;

pub const FIXTURE: &str = include_str!("../../../../tests/contracts/fixtures/budget-0.3.6.json");

pub fn scenarios_in_group(group: &str) -> Vec<Value> {
    let data: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(data["schema"], 1);
    data["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|scenario| scenario["group"] == group)
        .cloned()
        .collect()
}

fn time(value: &Value) -> DateTime<Utc> {
    value.as_str().unwrap().parse().unwrap()
}

fn policy(scenario: &Value, step: &Value) -> AppointmentPolicy {
    let mut merged = scenario["policy"].as_object().cloned().unwrap_or_default();
    if let Some(extra) = step.get("policy").and_then(Value::as_object) {
        merged.extend(extra.clone());
    }
    serde_json::from_value(Value::Object(merged)).unwrap()
}

/// Datetimes compare within one millisecond (Python rounds timedelta division to microseconds).
fn same(expected: &Value, actual: &Value, path: &str) -> Result<(), String> {
    if let (Some(a), Some(b)) = (expected.as_str(), actual.as_str())
        && let (Ok(a), Ok(b)) = (a.parse::<DateTime<Utc>>(), b.parse::<DateTime<Utc>>())
    {
        return if (a - b).num_milliseconds().abs() <= 1 {
            Ok(())
        } else {
            Err(format!("{path}: expected {a}, got {b}"))
        };
    }
    match (expected, actual) {
        (Value::Object(a), Value::Object(b)) => {
            let mut keys: Vec<_> = a.keys().chain(b.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                same(
                    a.get(key).unwrap_or(&Value::Null),
                    b.get(key).unwrap_or(&Value::Null),
                    &format!("{path}.{key}"),
                )?;
                if a.contains_key(key) != b.contains_key(key) {
                    return Err(format!("{path}.{key}: key set differs"));
                }
            }
            Ok(())
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Err(format!("{path}: expected {} items, got {}", a.len(), b.len()));
            }
            a.iter().zip(b).enumerate().try_for_each(|(i, (a, b))| same(a, b, &format!("{path}[{i}]")))
        }
        _ if expected == actual => Ok(()),
        _ => Err(format!("{path}: expected {expected}, got {actual}")),
    }
}

pub fn run_scenario(scenario: &Value) -> Result<(), String> {
    let name = scenario["name"].as_str().unwrap();
    let mut budget: AppointmentBudget = match scenario.get("initial").filter(|v| !v.is_null()) {
        Some(initial) => serde_json::from_value(initial.clone()).map_err(|e| format!("{name}: initial: {e}"))?,
        None => AppointmentBudget::default(),
    };
    for (index, step) in scenario["steps"].as_array().unwrap().iter().enumerate() {
        let op = step["op"].as_str().unwrap();
        let at = time(&step["at"]);
        let policy = policy(scenario, step);
        let label = format!("{name} step {index} ({op})");
        let observed: Option<Value> = match op {
            "submitted" => {
                budget.submitted(at, &policy);
                None
            }
            "cod_oper" => {
                budget.cod_oper(at, &policy);
                None
            }
            "ordinary_result" => {
                budget.ordinary_result(at);
                None
            }
            "office_during_restriction" => {
                let names = step["names"].as_array().unwrap().iter().map(|n| n.as_str().unwrap().to_owned()).collect();
                budget.office_during_restriction(at, names);
                None
            }
            "set_last_office_result" => {
                budget.episode_offices.last_mut().unwrap().result = Some(step["result"].as_str().unwrap().to_owned());
                None
            }
            "next_submission" => {
                let hot = step["hot"].as_bool().unwrap();
                Some(serde_json::to_value(budget.next_submission(at, &policy, hot)).unwrap())
            }
            other => return Err(format!("{label}: unsupported op: {other}")),
        };
        if let (Some(expected), Some(actual)) = (step.get("expect"), observed) {
            same(expected, &actual, &label)?;
        }
    }
    Ok(())
}
