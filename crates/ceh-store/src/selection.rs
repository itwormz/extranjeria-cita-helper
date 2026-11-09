//! Read-only statistics over unions of selected timeline intervals and markers.
use crate::{
    migrate::{StoreError, parse_time},
    periods::{Bounds, union_seconds},
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
type Time = DateTime<Utc>;
fn text<'a>(row: &'a Value, key: &str) -> &'a str {
    row[key].as_str().unwrap_or("")
}
fn event(row: &Value) -> bool {
    row["event"].as_bool().unwrap_or(false)
}
fn start(row: &Value) -> Result<Time, StoreError> {
    parse_time(text(row, "start"))
}
fn merge(mut spans: Vec<(Time, Time)>) -> Vec<(Time, Time)> {
    spans.retain(|(a, b)| a < b);
    spans.sort_unstable();
    let mut merged: Vec<(Time, Time)> = Vec::new();
    for (a, b) in spans {
        if let Some(last) = merged.last_mut()
            && a <= last.1
        {
            last.1 = last.1.max(b);
        } else {
            merged.push((a, b));
        }
    }
    merged
}
fn contains(spans: &[(Time, Time)], at: Time) -> bool {
    let i = spans.partition_point(|(a, _)| *a <= at);
    i > 0 && at < spans[i - 1].1
}
fn seconds(spans: &[(Time, Time)]) -> f64 {
    union_seconds(
        spans,
        Bounds {
            start: None,
            end: None,
        },
    )
}
const COUNTERS: [(&str, &str); 10] = [
    ("automation_action", "actions_performed"),
    ("processed_page", "pages_processed"),
    ("check", "checks"),
    ("office", "offices_seen"),
    ("booking", "booking_screens"),
    ("pass", "flow_passes"),
    ("expired", "expired_sessions"),
    ("server_error", "server_errors"),
    ("rate_limited", "rate_limited"),
    ("blocked", "blocked"),
];
pub fn selection_summary(rows: &[Value], history: &[Value]) -> Result<Value, StoreError> {
    let mut spans = Vec::new();
    let mut known = Vec::new();
    let mut recovery = Vec::new();
    let mut restored = Vec::new();
    let mut explicit = Vec::new();
    for row in rows {
        if event(row) {
            explicit.extend(
                row["source_rows"]
                    .as_array()
                    .cloned()
                    .unwrap_or_else(|| vec![row.clone()]),
            );
            continue;
        }
        let a = parse_time(row["_display_start"].as_str().unwrap_or(text(row, "start")))?;
        let b = parse_time(
            row["_display_end"]
                .as_str()
                .or(row["end"].as_str())
                .unwrap_or(text(row, "last_seen")),
        )?;
        spans.push((a, b));
        let gap = text(row, "timeline_state") == "gap"
            || ["unobserved", "sleep", "suspended"].contains(&text(row, "mode"));
        if !gap {
            known.push((a, b));
            if text(row, "timeline_state") == "recovery"
                || (text(row, "kind") != "clip"
                    && ["unknown", "server_error", "rate_limited"].contains(&text(row, "stage")))
            {
                recovery.push((a, b));
            }
            for pair in row["restored_ranges"].as_array().into_iter().flatten() {
                let left = pair
                    .get(0)
                    .and_then(Value::as_str)
                    .ok_or(StoreError::Corrupt)?;
                let right = pair
                    .get(1)
                    .and_then(Value::as_str)
                    .ok_or(StoreError::Corrupt)?;
                restored.push((a.max(parse_time(left)?), b.min(parse_time(right)?)));
            }
        }
    }
    let known = merge(known);
    let recovery = merge(recovery);
    let mut counts: BTreeMap<&str, u64> = COUNTERS
        .iter()
        .map(|(_, key)| (*key, 0))
        .chain([
            ("browser_resets", 0),
            ("unknown_errors", 0),
            ("fatal_errors", 0),
        ])
        .collect();
    let mut seen = BTreeSet::new();
    let mut offices = Vec::new();
    for (row, is_explicit) in history
        .iter()
        .map(|row| (row, false))
        .chain(explicit.iter().map(|row| (row, true)))
    {
        if !event(row) || row["counted"] == false {
            continue;
        }
        let at = start(row)?;
        if !is_explicit && !contains(&known, at) {
            continue;
        }
        let identity = (
            text(row, "session_id").to_owned(),
            text(row, "kind").to_owned(),
            row["id"].to_string(),
            text(row, "start").to_owned(),
        );
        if !seen.insert(identity) {
            continue;
        }
        let kind = text(row, "kind");
        if let Some((_, counter)) = COUNTERS.iter().find(|(value, _)| *value == kind)
            && (kind != "office"
                || row["offices"]
                    .as_array()
                    .is_some_and(|values| !values.is_empty()))
        {
            *counts.get_mut(counter).unwrap() += 1;
        }
        if kind == "browser_session_started"
            && row["evidence"]["browser_generation"].as_u64().unwrap_or(0) >= 2
        {
            *counts.get_mut("browser_resets").unwrap() += 1;
        }
        if ["fatal", "fatal_error", "browser_error"].contains(&kind) {
            *counts.get_mut("fatal_errors").unwrap() += 1;
        }
        if kind == "office" {
            for office in row["offices"].as_array().into_iter().flatten() {
                if !offices.contains(office) {
                    offices.push(office.clone());
                }
            }
        }
    }
    let mut unknown = BTreeSet::new();
    for row in history {
        if text(row, "kind") == "page"
            && text(row, "stage") == "unknown"
            && contains(&recovery, start(row)?)
        {
            unknown.insert((
                text(row, "session_id"),
                row["id"].to_string(),
                text(row, "start"),
            ));
        }
    }
    counts.insert("unknown_errors", unknown.len() as u64);
    if history.is_empty() {
        let mut clips = BTreeSet::new();
        counts.insert(
            "checks",
            rows.iter()
                .filter(|row| {
                    !event(row) && clips.insert((row["id"].to_string(), text(row, "start")))
                })
                .map(|row| row["checks"].as_u64().unwrap_or(0))
                .sum(),
        );
    }
    let total = seconds(&spans);
    let observed = seconds(&known);
    let number = |value: f64| if value == 0.0 { json!(0) } else { json!(value) };
    Ok(
        json!({"seconds":number(total),"observed_seconds":number(observed),"gap_seconds":number((total-observed).max(0.0)),"restored_seconds":number(seconds(&restored)),"counts":counts,"offices":offices}),
    )
}
