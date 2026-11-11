use ceh_core::appointment_schedule::{attempt_windows, earliest_submission};
use chrono::{DateTime, Duration, Utc};

fn utc(text: &str) -> DateTime<Utc> {
    text.parse().unwrap()
}

#[test]
fn daytime_and_evening_windows_follow_madrid_clock() {
    let morning = utc("2026-11-04T06:00:00Z");
    let windows = attempt_windows(morning, 3).unwrap();
    assert_eq!(
        windows[0],
        (utc("2026-11-04T10:30:00Z"), utc("2026-11-04T11:30:00Z"))
    );
    assert_eq!(windows[2].1, utc("2026-11-04T17:30:00Z"));
    let evening = utc("2026-07-04T17:00:00Z");
    assert_eq!(
        attempt_windows(evening, 1).unwrap(),
        vec![(utc("2026-07-04T21:30:00Z"), utc("2026-07-04T22:00:00Z"))]
    );
    assert!(attempt_windows(evening, 0).unwrap().is_empty());
}

#[test]
fn takes_the_later_of_minimum_interval_and_restriction() {
    let now = utc("2026-11-04T11:00:00Z");
    let restriction = now + Duration::minutes(30);
    assert_eq!(
        earliest_submission(Some(now), Some(restriction), Duration::minutes(15)),
        Some(restriction)
    );
    assert_eq!(
        earliest_submission(Some(now), None, Duration::minutes(15)),
        Some(now + Duration::minutes(15))
    );
}

#[test]
fn cod_oper_adds_no_cooldown_but_the_adaptive_target_still_crosses_madrid_midnight() {
    // Python 0.3.x paused 30/60 minutes after `cod. oper`; that pause is gone by owner decision (2026-11-11).
    // The answer is still recorded, and the next day's estimate still drops by one.
    use ceh_core::appointment_budget::{AppointmentBudget, AppointmentPolicy};
    let policy = AppointmentPolicy::default();
    let mut budget = AppointmentBudget::default();
    let now = utc("2026-11-04T22:40:00Z");
    budget.submitted(now, &policy);
    budget.submitted(now + Duration::minutes(1), &policy);
    budget.cod_oper(now + Duration::minutes(2), &policy);
    let tomorrow = utc("2026-11-04T23:00:00Z");
    // Only the ordinary 15-minute pause (long over by midnight) applies.
    assert_eq!(budget.next_submission(tomorrow, &policy, true), tomorrow);
    // Python 0.3.x never estimates fewer than five attempts for the next day.
    assert_eq!(budget.target(&policy), 5);
    assert_eq!(budget.remaining(&policy), 5);
    let submitted = tomorrow;
    budget.submitted(submitted, &policy);
    assert_eq!(budget.next_submission(submitted, &policy, true), submitted + Duration::minutes(15));
    budget.ordinary_result(submitted);
    assert_eq!(budget.next_submission(submitted, &policy, true), submitted + Duration::minutes(15));
}

#[test]
fn huge_target_returns_bounded_windows_quickly() {
    let started = std::time::Instant::now();
    let windows = attempt_windows(utc("2026-11-04T08:00:00Z"), u32::MAX).unwrap();
    assert!(!windows.is_empty() && windows.len() <= 24 * 60);
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
}
