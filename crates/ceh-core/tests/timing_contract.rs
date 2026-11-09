use ceh_core::timings::{RetryKind, deadline_millis, parse_timings};
use serde_json::Value;
fn floats(value: Value) -> Value {
    match value {
        Value::Number(n) => serde_json::json!(n.as_f64().unwrap()),
        Value::Array(values) => Value::Array(values.into_iter().map(floats).collect()),
        Value::Object(values) => {
            Value::Object(values.into_iter().map(|(k, v)| (k, floats(v))).collect())
        }
        other => other,
    }
}
#[test]
fn timing_validation_matches_python_defaults_bounds_and_legacy_upgrade() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/fixtures/timings.json"
    ))
    .unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let result = parse_timings(&case["input"]);
        if case["expected"]["valid"] == true {
            assert_eq!(
                floats(serde_json::to_value(result.unwrap()).unwrap()),
                floats(case["expected"]["value"].clone()),
                "case {}",
                case["id"]
            );
        } else {
            assert!(result.is_err(), "case {}", case["id"]);
        }
    }
}
#[test]
fn retry_schedules_reuse_last_delay_and_validate_sampled_jitter() {
    let timing = parse_timings(&serde_json::json!({"recovery_jitter_seconds":[1,3]})).unwrap();
    assert_eq!(
        timing.recovery_delay(RetryKind::Blocked, 0, 1.).unwrap(),
        31.
    );
    assert_eq!(
        timing
            .recovery_delay(RetryKind::Blocked, usize::MAX, 3.)
            .unwrap(),
        603.
    );
    assert_eq!(
        timing.recovery_delay(RetryKind::RateLimit, 0, 2.).unwrap(),
        182.
    );
    assert!(timing.recovery_delay(RetryKind::Blocked, 0, 0.).is_err());
    assert!(
        timing
            .recovery_delay(RetryKind::Blocked, 0, f64::NAN)
            .is_err()
    );
}
#[test]
fn virtual_deadlines_never_fire_early_and_reject_overflow() {
    assert_eq!(deadline_millis(100, 30.).unwrap(), 30100);
    assert_eq!(deadline_millis(100, 0.0001).unwrap(), 101);
    assert!(deadline_millis(100, -1.).is_err());
    assert!(deadline_millis(100, f64::INFINITY).is_err());
    assert!(deadline_millis(u64::MAX, 1.).is_err());
}
