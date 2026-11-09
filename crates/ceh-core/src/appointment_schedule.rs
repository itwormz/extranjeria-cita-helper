//! Civil-time scheduling for the appointment submission policy.
use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe::Madrid;

const WINDOW_RADIUS: Duration = Duration::minutes(30);
/// Only near-term windows are needed (the plan is rebuilt after each submission or expiry);
/// bounding the list keeps memory and time independent of a very large daily target.
const MAX_WINDOWS: u32 = 24 * 60;

/// Calculate the remaining windows for the Madrid day. The caller refreshes
/// these on submission or when a window expires, rather than on every poll.
pub fn attempt_windows(
    now: DateTime<Utc>,
    remaining: u32,
) -> Option<Vec<(DateTime<Utc>, DateTime<Utc>)>> {
    if remaining == 0 {
        return Some(Vec::new());
    }
    let local = now.with_timezone(&Madrid).naive_local();
    let date = local.date();
    let morning = date.and_hms_opt(9, 0, 0)?;
    let evening = date.and_hms_opt(18, 0, 0)?;
    let midnight = date.succ_opt()?.and_hms_opt(0, 0, 0)?;
    let start = local.max(morning);
    let end = if local < evening { evening } else { midnight };
    let span = (end - start).num_microseconds()?;
    let to_utc = |local: NaiveDateTime| {
        Madrid
            .from_local_datetime(&local)
            .single()
            .map(|time| time.to_utc())
    };
    (1..=remaining.min(MAX_WINDOWS))
        .map(|index| {
            let center = start
                + Duration::microseconds(
                    (i128::from(span) * i128::from(index) / i128::from(remaining)) as i64,
                );
            Some((
                to_utc(morning.max(center - WINDOW_RADIUS))?,
                to_utc(midnight.min(center + WINDOW_RADIUS))?,
            ))
        })
        .collect()
}

pub fn earliest_submission(
    last_submission: Option<DateTime<Utc>>,
    restriction_until: Option<DateTime<Utc>>,
    minimum_pause: Duration,
) -> Option<DateTime<Utc>> {
    match last_submission {
        Some(last) => {
            let ordinary = last.checked_add_signed(minimum_pause)?;
            Some(restriction_until.map_or(ordinary, |restriction| ordinary.max(restriction)))
        }
        None => restriction_until,
    }
}
