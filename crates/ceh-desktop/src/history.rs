//! Profile-scoped read-only history with explicit civil-day selection.
use ceh_store::{
    periods::{Bounds, Period},
    statistics::HistorySnapshot,
};
use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, TimeZone, Utc};
use std::path::Path;
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(tag = "period", rename_all = "snake_case", deny_unknown_fields)]
pub enum HistoryRange {
    All {},
    Today {},
    Range { start: String, end: String },
}
pub fn selection_bounds(
    selection: &HistoryRange,
    now: DateTime<Utc>,
) -> Result<Bounds, &'static str> {
    match selection {
        HistoryRange::All {} => {
            ceh_store::periods::bounds(Period::All, now).map_err(|_| "invalid_period")
        }
        HistoryRange::Today {} => {
            ceh_store::periods::bounds(Period::Today, now).map_err(|_| "invalid_period")
        }
        HistoryRange::Range { start, end } => {
            let parse = |text: &str| {
                NaiveDate::parse_from_str(text, "%Y-%m-%d")
                    .ok()
                    .filter(|date| {
                        date.year() > 0
                            && date.year() < 9999
                            && date.format("%Y-%m-%d").to_string() == text
                    })
                    .ok_or("invalid_period")
            };
            let start = parse(start)?;
            let end = parse(end)?;
            if start > end {
                return Err("invalid_period");
            }
            let midnight = |date: NaiveDate| {
                chrono_tz::Europe::Madrid
                    .from_local_datetime(&date.and_time(NaiveTime::MIN))
                    .single()
                    .map(|date| date.to_utc())
                    .ok_or("invalid_period")
            };
            Ok(Bounds {
                start: Some(midnight(start)?),
                end: Some(midnight(end.succ_opt().ok_or("invalid_period")?)?),
            })
        }
    }
}
pub fn profile_history(
    root: &Path,
    id: &str,
    selection: &HistoryRange,
) -> Result<Option<HistorySnapshot>, &'static str> {
    let profiles = crate::bootstrap(root, "ru")?;
    if !profiles.profiles.iter().any(|profile| profile.id == id) {
        return Err("profile_not_found");
    }
    let now = crate::current_time()?;
    let bounds = selection_bounds(selection, now)?;
    ceh_store::statistics::history_file(
        &root.join("data/profile-statistics.sqlite3"),
        id,
        now,
        bounds.start,
        bounds.end,
    )
    .map_err(|_| "statistics_unavailable")
}

pub fn profile_selection(
    root: &Path,
    id: &str,
    selection: &HistoryRange,
    ids: &[String],
) -> Result<Option<serde_json::Value>, &'static str> {
    if ids.len() > 512 || ids.iter().any(|key| key.len() > 256) {
        return Err("invalid_selection");
    }
    let profiles = crate::bootstrap(root, "ru")?;
    if !profiles.profiles.iter().any(|profile| profile.id == id) {
        return Err("profile_not_found");
    }
    let now = crate::current_time()?;
    let bounds = selection_bounds(selection, now)?;
    ceh_store::statistics::selection_file(
        &root.join("data/profile-statistics.sqlite3"),
        id,
        now,
        bounds.start,
        bounds.end,
        ids,
    )
    .map_err(|_| "statistics_unavailable")
}
