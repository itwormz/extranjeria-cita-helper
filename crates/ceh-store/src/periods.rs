//! UTC interval arithmetic; local calendar boundaries use Spain's civil timezone.
use crate::migrate::StoreError;
use chrono::{DateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Europe::Madrid;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    All,
    Today,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
}
pub fn bounds(period: Period, now: DateTime<Utc>) -> Result<Bounds, StoreError> {
    if period == Period::All {
        return Ok(Bounds {
            start: None,
            end: None,
        });
    }
    let date = now.with_timezone(&Madrid).date_naive();
    let next = date.succ_opt().ok_or(StoreError::InvalidTimestamp)?;
    let midnight = |day: chrono::NaiveDate| {
        Madrid
            .from_local_datetime(&day.and_time(NaiveTime::MIN))
            .single()
            .map(|v| v.with_timezone(&Utc))
            .ok_or(StoreError::InvalidTimestamp)
    };
    Ok(Bounds {
        start: Some(midnight(date)?),
        end: Some(midnight(next)?),
    })
}
fn clip(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    bounds: Bounds,
) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let start = bounds.start.map_or(start, |lower| start.max(lower));
    let end = bounds.end.map_or(end, |upper| end.min(upper));
    (start < end).then_some((start, end))
}
pub fn clipped_seconds(start: DateTime<Utc>, end: DateTime<Utc>, bounds: Bounds) -> f64 {
    clip(start, end, bounds).map_or(0.0, |(start, end)| {
        (end - start)
            .num_microseconds()
            .map_or(0.0, |v| v as f64 / 1_000_000.0)
    })
}
/// Only confirmed intervals contribute. Gaps and backward clock jumps contribute zero.
pub fn union_seconds(intervals: &[(DateTime<Utc>, DateTime<Utc>)], bounds: Bounds) -> f64 {
    let mut intervals = intervals
        .iter()
        .filter_map(|&(start, end)| clip(start, end, bounds))
        .collect::<Vec<_>>();
    intervals.sort_unstable();
    let mut merged: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    for (start, end) in intervals {
        if let Some(last) = merged.last_mut()
            && start <= last.1
        {
            last.1 = last.1.max(end);
        } else {
            merged.push((start, end));
        }
    }
    merged
        .into_iter()
        .map(|(start, end)| {
            clipped_seconds(
                start,
                end,
                Bounds {
                    start: None,
                    end: None,
                },
            )
        })
        .sum()
}
