//! Pure projection. Original observations are never rewritten to imply durations.
use crate::migrate::{StoreError, parse_time};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
type Time = DateTime<Utc>;
const SEARCH: &[&str] = &[
    "province",
    "procedure",
    "auth",
    "auth_wait",
    "applicant",
    "request_menu",
    "applicant_no_slots",
    "request_no_slots",
    "request_no_slots_restart",
    "post_contact_no_slots",
];
const NO_SLOTS: &[&str] = &[
    "applicant_no_slots",
    "request_no_slots",
    "request_no_slots_restart",
    "post_contact_no_slots",
];
const MANUAL: &[&str] = &["manual", "paused", "pause", "booking_handoff"];
const GAPS: &[&str] = &["unobserved", "sleep", "suspended"];
const TERMINAL: &[&str] = &["finished", "stopped"];
const REASONS: &[&str] = &[
    "completed",
    "stopped",
    "failed",
    "interrupted",
    "historical_finish",
];
fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
fn event(v: &Value) -> bool {
    v["event"].as_bool().unwrap_or(false)
}
fn start(v: &Value) -> Result<Time, StoreError> {
    parse_time(if text(v, "start").is_empty() {
        text(v, "time")
    } else {
        text(v, "start")
    })
}
fn end(v: &Value) -> Result<Time, StoreError> {
    let begin = start(v)?;
    let value = ["end", "last_seen", "start", "time"]
        .into_iter()
        .find_map(|key| v[key].as_str().filter(|s| !s.is_empty()))
        .ok_or(StoreError::InvalidTimestamp)?;
    Ok(begin.max(parse_time(value)?))
}
fn stamp(v: Time) -> String {
    v.to_rfc3339_opts(
        if v.timestamp_subsec_micros() == 0 {
            SecondsFormat::Secs
        } else {
            SecondsFormat::Micros
        },
        false,
    )
}
fn session(v: &Value) -> &str {
    if v.get("session_id").is_some() {
        text(v, "session_id")
    } else {
        text(v, "session")
    }
}
fn identity(session: &str, state: &str, when: Time) -> String {
    format!(
        "timeline-{}",
        &format!(
            "{:x}",
            Sha256::digest(format!("{session}/{state}/{}", stamp(when)))
        )[..24]
    )
}
fn codes(v: &Value) -> Vec<&str> {
    let evidence = &v["evidence"];
    let values = if evidence.is_object() {
        &evidence["codes"]
    } else {
        evidence
    };
    values
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}
fn state(v: &Value) -> &'static str {
    let stage = text(v, "stage");
    let kind = text(v, "kind");
    if stage == "cod_oper" || (NO_SLOTS.contains(&stage) && codes(v).contains(&"operation_code")) {
        "cod"
    } else if ["expired", "blocked"].contains(&stage) || ["expired", "blocked"].contains(&kind) {
        "anomaly"
    } else if stage == "booking" {
        "manual"
    } else if [
        "error",
        "429",
        "500",
        "timeout",
        "unknown",
        "rate_limit",
        "loading",
    ]
    .iter()
    .any(|word| stage.contains(word) || kind.contains(word))
    {
        "recovery"
    } else {
        "search"
    }
}
fn generation(v: &Value) -> i64 {
    v["evidence"]["browser_generation"].as_i64().unwrap_or(0)
}
fn ordered(rows: &[Value]) -> Result<Vec<usize>, StoreError> {
    let mut values = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            Ok((
                start(row)?,
                if !event(row) {
                    0
                } else {
                    match text(row, "kind") {
                        "check" => 1,
                        "office" => 2,
                        "booking" => 3,
                        _ => 1,
                    }
                },
                i,
            ))
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    values.sort();
    Ok(values.into_iter().map(|(_, _, i)| i).collect())
}
fn availability(rows: &[Value]) -> Result<Vec<Value>, StoreError> {
    let mut result: Vec<Value> = Vec::new();
    let mut current = None;
    for i in ordered(rows)? {
        let row = &rows[i];
        let kind = text(row, "kind");
        let stage = text(row, "stage");
        let mode = text(row, "mode");
        if ["check", "flow_restart", "finish", "fatal_error"].contains(&kind)
            || (kind == "phase" && (GAPS.contains(&mode) || TERMINAL.contains(&mode)))
            || (kind == "browser_session_started" && generation(row) >= 2)
            || (kind == "page"
                && (SEARCH.contains(&stage)
                    || [
                        "expired",
                        "blocked",
                        "server_error",
                        "rate_limited",
                        "unknown",
                    ]
                    .contains(&stage)))
        {
            current = None;
        }
        if !event(row) || !["office", "booking", "appointment"].contains(&kind) {
            continue;
        }
        let index = if let Some(index) = current {
            index
        } else {
            let when = start(row)?;
            let mut marker = row.clone();
            let fields = json!({"kind":"availability","event":true,"id":identity(session(row),"availability",when),"timeline_state":"available","start":stamp(when),"end":stamp(when),"last_seen":stamp(when),"availability_kind":"office","office_seen_at":null,"booking_seen_at":null,"offices":[],"bookings":[],"source_rows":[],"restored":false,"restored_ranges":[]});
            marker
                .as_object_mut()
                .ok_or(StoreError::Corrupt)?
                .extend(fields.as_object().unwrap().clone());
            result.push(marker);
            let index = result.len() - 1;
            current = Some(index);
            index
        };
        let marker = &mut result[index];
        marker["source_rows"]
            .as_array_mut()
            .unwrap()
            .push(row.clone());
        if kind == "office" {
            if marker["office_seen_at"].is_null() {
                marker["office_seen_at"] = json!(text(row, "start"));
            }
        } else {
            marker["availability_kind"] = json!("booking");
            if marker["booking_seen_at"].is_null() {
                marker["booking_seen_at"] = json!(text(row, "start"));
            }
            if let Some(details) = row["evidence"].get("booking_details") {
                let normalized = ceh_core::booking::normalize_booking_details(details);
                let mut details =
                    serde_json::to_value(normalized).map_err(|_| StoreError::Corrupt)?;
                if details["office"].is_string()
                    || details["offers"].as_array().is_some_and(|a| !a.is_empty())
                {
                    details["observed_at"] = json!(text(row, "start"));
                    marker["bookings"].as_array_mut().unwrap().push(details);
                }
            }
        }
        for office in row["offices"].as_array().into_iter().flatten() {
            let offices = marker["offices"].as_array_mut().unwrap();
            if !offices.contains(office) {
                offices.push(office.clone());
            }
        }
    }
    Ok(result)
}
fn navigation_transients(
    rows: &[Value],
    pages: &[usize],
    phases: &[usize],
    metadata: &Value,
) -> Result<(BTreeMap<usize, Vec<usize>>, bool), StoreError> {
    let context = if let Some(raw) = metadata["context"].as_str() {
        serde_json::from_str::<Value>(raw).unwrap_or(Value::Null)
    } else {
        metadata["context"].clone()
    };
    let historical = session(metadata).starts_with("archive-")
        || session(metadata).starts_with("capture-")
        || context["app_version"] == "historical";
    let blocked_kinds = [
        "recovery",
        "timeout",
        "network_error",
        "fatal",
        "fatal_error",
        "browser_error",
        "manual_required",
        "finish",
        "handoff",
        "pause",
        "server_error",
        "rate_limited",
        "blocked",
        "expired",
    ];
    let blocked_codes = [
        "http_429",
        "http_5xx",
        "server_error",
        "rate_limited",
        "blocked",
        "expired",
        "network_error",
        "timeout",
    ];
    let mut blocked_times = Vec::new();
    for row in rows {
        let status = row["evidence"]["http_status"].as_i64();
        if (event(row) && blocked_kinds.contains(&text(row, "kind")))
            || (text(row, "kind") == "browser_session_started" && generation(row) >= 2)
            || status.is_some_and(|status| status == 429 || (500..=599).contains(&status))
            || codes(row).iter().any(|code| blocked_codes.contains(code))
        {
            blocked_times.push(start(row)?);
        }
    }
    let mut result = BTreeMap::new();
    let mut index = 0;
    while index < pages.len() {
        if !["unknown", "loading"].contains(&text(&rows[pages[index]], "stage")) {
            index += 1;
            continue;
        }
        let begin = index;
        while index < pages.len()
            && ["unknown", "loading"].contains(&text(&rows[pages[index]], "stage"))
        {
            index += 1;
        }
        if begin == 0 || index == pages.len() {
            continue;
        }
        let before = &rows[pages[begin - 1]];
        let after = &rows[pages[index]];
        let left = start(&rows[pages[begin]])?;
        let right = start(after)?;
        if state(before) != "search"
            || state(after) != "search"
            || (right - left).num_milliseconds() > 5000
        {
            continue;
        }
        let phase_index = phases
            .iter()
            .rposition(|&p| start(&rows[p]).is_ok_and(|t| t <= left));
        if !historical
            && (phase_index.is_none_or(|i| text(&rows[phases[i]], "mode") != "working")
                || phase_index
                    .and_then(|i| phases.get(i + 1))
                    .is_some_and(|&p| start(&rows[p]).unwrap() < right))
        {
            continue;
        }
        if blocked_times.iter().any(|&t| left <= t && t <= right) {
            continue;
        }
        let mut blocked = false;
        for i in phase_index.unwrap_or(0)..phases.len() {
            let phase = &rows[phases[i]];
            if start(phase)? >= right {
                break;
            }
            let mode = text(phase, "mode");
            if !MANUAL.contains(&mode)
                && !GAPS.contains(&mode)
                && !TERMINAL.contains(&mode)
                && mode != "recovery"
            {
                continue;
            }
            let mut finish = if let Some(&next) = phases.get(i + 1) {
                start(&rows[next])?
            } else {
                right
            };
            if phase["end"].is_string()
                && start(before)? > start(phase)?
                && start(before)? >= end(phase)?
            {
                finish = finish.min(end(phase)?);
            }
            if finish > left {
                blocked = true;
                break;
            }
        }
        if !blocked {
            let group = pages[begin..index].to_vec();
            for &p in &group {
                result.insert(p, group.clone());
            }
        }
    }
    Ok((result, historical))
}
fn facts(
    clip: &mut Value,
    sources: &[usize],
    rows: &[Value],
    right: Time,
    seen: &mut BTreeSet<usize>,
) -> Result<(), StoreError> {
    for &index in sources {
        let row = &rows[index];
        if seen.insert(index) {
            clip["source_rows"]
                .as_array_mut()
                .unwrap()
                .push(row.clone());
        }
        let stage = text(row, "stage");
        if !stage.is_empty()
            && !clip["page_stages"]
                .as_array()
                .unwrap()
                .contains(&json!(stage))
        {
            clip["page_stages"]
                .as_array_mut()
                .unwrap()
                .push(json!(stage));
        }
        for key in ["start", "last_seen"] {
            if let Some(value) = row[key].as_str() {
                let when = parse_time(value)?;
                if when <= right && stamp(when).as_str() > text(clip, "last_seen") {
                    clip["last_seen"] = json!(stamp(when));
                }
            }
        }
    }
    Ok(())
}
fn run(rows: &[Value], metadata: &Value, cap: Option<Time>) -> Result<Vec<Value>, StoreError> {
    let session_id = session(metadata);
    let known = metadata["start"].is_string();
    let begin = if known {
        start(metadata)?
    } else {
        rows.iter()
            .map(start)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .min()
            .ok_or(StoreError::Corrupt)?
    };
    let supported = rows
        .iter()
        .filter(|r| !event(r))
        .map(end)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max()
        .unwrap_or(begin);
    let mut finish = if known {
        metadata["end"]
            .as_str()
            .or(metadata["last_seen"].as_str())
            .map(parse_time)
            .transpose()?
            .unwrap_or(supported)
    } else {
        supported
    };
    if known && metadata["end"].is_null() {
        finish = finish.max(supported);
        for row in rows {
            if event(row)
                && [
                    "check",
                    "office",
                    "booking",
                    "appointment",
                    "browser_session_started",
                ]
                .contains(&text(row, "kind"))
            {
                finish = finish.max(start(row)?);
            }
        }
    }
    if known && metadata["end"].is_string() && !REASONS.contains(&text(metadata, "reason")) {
        finish = finish.min(supported);
    }
    let original_end = finish;
    let overlap = cap.is_some_and(|cap| cap < finish);
    if let Some(cap) = cap {
        finish = finish.min(cap);
    }
    let sorted = ordered(rows)?;
    let pages = sorted
        .iter()
        .copied()
        .filter(|&i| !event(&rows[i]) && text(&rows[i], "kind") == "page")
        .collect::<Vec<_>>();
    let phases = sorted
        .iter()
        .copied()
        .filter(|&i| !event(&rows[i]) && text(&rows[i], "kind") == "phase")
        .collect::<Vec<_>>();
    let (transients, historical) = navigation_transients(rows, &pages, &phases, metadata)?;
    let availability = availability(
        &rows
            .iter()
            .filter(|row| start(row).is_ok_and(|t| begin <= t && t <= finish))
            .cloned()
            .collect::<Vec<_>>(),
    )?;
    let mut forced = BTreeSet::new();
    for row in &availability {
        forced.insert(start(row)?);
    }
    let mut markers = Vec::new();
    let mut resets = Vec::new();
    for &i in &sorted {
        let row = &rows[i];
        if !event(row) || ["office", "booking", "appointment"].contains(&text(row, "kind")) {
            continue;
        }
        let kind = text(row, "kind");
        if kind == "browser_session_started" && generation(row) < 2 {
            continue;
        }
        let mut marker = row.clone();
        marker["timeline_state"] = json!(if kind == "browser_session_started" {
            "restart"
        } else if ["fatal", "fatal_error", "browser_error"].contains(&kind) {
            "fatal"
        } else {
            state(row)
        });
        marker["source_rows"] = json!([row]);
        marker["restored"] = json!(false);
        if [
            "browser_session_started",
            "finish",
            "fatal_error",
            "fatal",
            "browser_error",
        ]
        .contains(&kind)
        {
            resets.push(i);
            forced.insert(start(row)?);
        }
        markers.push(marker);
    }
    let mut edges = BTreeSet::from([begin, finish]);
    edges.extend(forced.iter().copied());
    for &i in pages.iter().chain(&phases).chain(&resets) {
        edges.insert(start(&rows[i])?);
        edges.insert(end(&rows[i])?);
    }
    let edges = edges
        .into_iter()
        .filter(|t| begin <= *t && *t <= finish)
        .collect::<Vec<_>>();
    let mut reset_starts = BTreeSet::from([begin]);
    for &i in &resets {
        reset_starts.insert(start(&rows[i])?);
    }
    for &i in &phases {
        let mode = text(&rows[i], "mode");
        if GAPS.contains(&mode) || TERMINAL.contains(&mode) {
            reset_starts.insert(start(&rows[i])?);
        }
    }
    let mut conditions = Vec::new();
    for &i in &sorted {
        let row = &rows[i];
        let stage = text(row, "stage");
        if text(row, "kind") == "page" && !event(row) {
            if state(row) == "cod" {
                conditions.push((start(row)?, true));
            } else if (NO_SLOTS.contains(&stage) && stage != "applicant_no_slots")
                || ["office", "contact", "booking"].contains(&stage)
            {
                conditions.push((start(row)?, false));
            }
        } else if event(row) && ["office", "booking", "appointment"].contains(&text(row, "kind")) {
            conditions.push((start(row)?, false));
        }
    }
    let mut result: Vec<Value> = Vec::new();
    let mut seen = BTreeSet::new();
    let mut transient_groups = BTreeSet::new();
    for edge in edges.windows(2) {
        let (left, right) = (edge[0], edge[1]);
        if right <= left {
            continue;
        }
        let reset = *reset_starts.range(..=left).next_back().unwrap();
        let phase = phases
            .iter()
            .copied()
            .rev()
            .find(|&i| start(&rows[i]).is_ok_and(|t| t <= left));
        let page = pages
            .iter()
            .copied()
            .rev()
            .find(|&i| {
                text(&rows[i], "stage") != "loading"
                    && start(&rows[i]).is_ok_and(|t| reset <= t && t <= left)
            })
            .or_else(|| {
                pages
                    .iter()
                    .copied()
                    .rev()
                    .find(|&i| start(&rows[i]).is_ok_and(|t| reset <= t && t <= left))
            });
        let mode = phase.map_or("", |i| text(&rows[i], "mode"));
        let stale = if let (Some(p), Some(q)) = (phase, page) {
            rows[p]["end"].is_string()
                && start(&rows[q])? > start(&rows[p])?
                && start(&rows[q])? >= end(&rows[p])?
        } else {
            false
        };
        let manual_error = page.is_some_and(|i| {
            [
                "expired",
                "blocked",
                "server_error",
                "rate_limited",
                "unknown",
            ]
            .contains(&text(&rows[i], "stage"))
        }) && mode == "manual";
        let mut sources = Vec::new();
        let mut category = None;
        if let Some(p) = phase {
            let row = &rows[p];
            if MANUAL.contains(&mode) && !stale && !manual_error && (end(row)? > left || known) {
                category = Some("manual");
                sources.push(p);
            } else if (GAPS.contains(&mode) || TERMINAL.contains(&mode))
                && page.is_none_or(|q| start(&rows[q]).unwrap() <= start(row).unwrap())
            {
                category = Some("gap");
                sources.push(p);
            } else if state(row) == "cod"
                && end(row)? > left
                && page.is_none_or(|q| start(&rows[q]).unwrap() <= start(row).unwrap())
            {
                category = Some("cod");
                sources.push(p);
            } else if mode == "recovery"
                && end(row)? > left
                && page.is_none_or(|q| start(&rows[q]).unwrap() <= start(row).unwrap())
                && page.is_none_or(|q| !["cod", "anomaly"].contains(&state(&rows[q])))
            {
                category = Some("recovery");
                sources.push(p);
                if let Some(q) = page {
                    sources.push(q);
                }
            }
        }
        if category.is_none() {
            if let Some(q) = page {
                category = Some(if transients.contains_key(&q) {
                    "search"
                } else {
                    state(&rows[q])
                });
                sources.push(q);
                if let Some(p) = phase
                    && start(&rows[p])? >= reset
                {
                    sources.push(p);
                }
            } else if let Some(p) = phase
                && start(&rows[p])? >= reset
            {
                category = Some(if mode == "recovery" {
                    "recovery"
                } else {
                    "search"
                });
                sources.push(p);
            } else {
                sources = resets
                    .iter()
                    .copied()
                    .filter(|&i| start(&rows[i]).unwrap() == reset)
                    .collect();
                if !sources.is_empty() {
                    category = Some("gap");
                }
            }
        }
        let Some(mut category) = category else {
            continue;
        };
        if category == "search"
            && page.is_some_and(|q| SEARCH.contains(&text(&rows[q], "stage")))
            && conditions
                .iter()
                .rev()
                .find(|(t, _)| *t <= left)
                .is_some_and(|(_, cod)| *cod)
        {
            category = "cod";
        }
        let transient = page.and_then(|q| transients.get(&q));
        let actual = &rows[sources[0]];
        let supported = (start(actual)? <= left && end(actual)? >= right)
            || (transient.is_some() && !historical);
        if !known && !supported {
            continue;
        }
        let boundary = forced.contains(&left)
            || result
                .last()
                .is_some_and(|clip| text(clip, "timeline_state") == "gap" && category != "gap");
        if !result.last().is_some_and(|clip| {
            text(clip, "timeline_state") == category
                && text(clip, "end") == stamp(left)
                && !boundary
        }) {
            seen.clear();
            transient_groups.clear();
            result.push(json!({"kind":"clip","event":false,"session_id":session_id,"id":identity(session_id,category,left),"start":stamp(left),"end":stamp(right),"last_seen":stamp(start(actual)?),"timeline_state":category,"stage":text(actual,"stage"),"mode":mode,"confirmed_end":false,"restored":false,"restored_ranges":[],"source_rows":[],"page_stages":[],"evidence":actual.get("evidence").cloned().unwrap_or(json!([])),"checks":0}));
        }
        let clip = result.last_mut().unwrap();
        clip["end"] = json!(stamp(right));
        if let Some(group) = transient
            && category == "search"
            && transient_groups.insert(group[0])
        {
            facts(clip, group, rows, right, &mut seen)?;
        }
        facts(clip, &sources, rows, right, &mut seen)?;
        if (!supported || (transient.is_some() && historical && category == "search"))
            && category != "gap"
        {
            clip["restored"] = json!(true);
            let ranges = clip["restored_ranges"].as_array_mut().unwrap();
            if let Some(last) = ranges.last_mut()
                && last[1] == stamp(left)
            {
                last[1] = json!(stamp(right));
            } else {
                ranges.push(json!([stamp(left), stamp(right)]));
            }
        }
        if let Some(group) = transient
            && category == "search"
        {
            if !historical {
                clip["provisional_reason"] = json!("working_navigation_pending");
            } else {
                clip["restored_reason"] = json!("historical_navigation_transient");
                if clip.get("restoration_provenance").is_none() {
                    clip["restoration_provenance"] = json!([]);
                }
                let provenance = clip["restoration_provenance"].as_array_mut().unwrap();
                let source = group.iter().map(|&i| rows[i].clone()).collect::<Vec<_>>();
                if let Some(last) = provenance.last_mut()
                    && last["end"] == stamp(left)
                    && last["source_rows"] == json!(source)
                {
                    last["end"] = json!(stamp(right));
                } else {
                    provenance.push(json!({"start":stamp(left),"end":stamp(right),"reason":"historical_navigation_transient","source_rows":source}));
                }
            }
        }
        clip["confirmed_end"] = json!(
            right < finish
                || (known
                    && metadata["end"].is_string()
                    && REASONS.contains(&text(metadata, "reason")))
        );
    }
    let mut latest = BTreeMap::new();
    for &i in &pages {
        latest.insert(start(&rows[i])?, i);
    }
    for (when, i) in latest {
        if when < begin
            || when > finish
            || result.iter().any(|clip| {
                clip["source_rows"].as_array().unwrap().contains(&rows[i])
                    || (start(clip).unwrap() <= when && when < end(clip).unwrap())
            })
        {
            continue;
        }
        if text(&rows[i], "stage") == "loading"
            && pages
                .iter()
                .any(|&q| text(&rows[q], "stage") != "loading" && start(&rows[q]).unwrap() < when)
        {
            continue;
        }
        let row = &rows[i];
        let category = state(row);
        result.push(json!({"kind":"clip","event":false,"session_id":session_id,"id":identity(session_id,category,when),"start":stamp(when),"end":stamp(when),"last_seen":stamp(when),"timeline_state":category,"stage":text(row,"stage"),"mode":"","confirmed_end":false,"restored":false,"restored_ranges":[],"source_rows":[row],"page_stages":[text(row,"stage")],"evidence":row.get("evidence").cloned().unwrap_or(json!([])),"checks":0}));
    }
    for clip in &mut result {
        if overlap && text(clip, "end") == stamp(finish) {
            clip["source_overlap"] = json!(true);
            clip["original_source_end"] = json!(stamp(original_end));
            clip["restored"] = json!(true);
        }
        let left = start(clip)?;
        let right = end(clip)?;
        clip["checks"] = json!(
            rows.iter()
                .filter(|row| event(row)
                    && text(row, "kind") == "check"
                    && start(row).is_ok_and(|t| left <= t && t < right))
                .count()
        );
    }
    result.extend(
        markers
            .into_iter()
            .chain(availability)
            .filter(|row| start(row).is_ok_and(|t| begin <= t && t <= finish)),
    );
    Ok(result)
}
pub fn build_timeline(rows: &[Value], sessions: &[Value]) -> Result<Value, StoreError> {
    let mut groups: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut encounter_order: Vec<String> = Vec::new();
    for row in rows {
        if text(row, "start").is_empty() && text(row, "time").is_empty() {
            continue;
        }
        start(row)?;
        end(row)?;
        if !groups.contains_key(session(row)) {
            encounter_order.push(session(row).to_owned());
        }
        groups
            .entry(session(row).into())
            .or_default()
            .push(row.clone());
    }
    let metadata = sessions
        .iter()
        .map(|row| (session(row).to_owned(), row))
        .collect::<BTreeMap<_, _>>();
    let mut runs = encounter_order
        .iter()
        .map(|key| {
            let begin = if let Some(row) = metadata.get(key)
                && row["start"].is_string()
            {
                start(row)?
            } else {
                groups[key]
                    .iter()
                    .map(start)
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .min()
                    .unwrap()
            };
            Ok((begin, key.clone()))
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    runs.sort_by_key(|(begin, _)| *begin);
    let mut result = Vec::new();
    for (i, (_, key)) in runs.iter().enumerate() {
        let fallback = json!({"session":key});
        let meta = metadata.get(key).copied().unwrap_or(&fallback);
        result.extend(run(
            &groups[key],
            meta,
            runs.get(i + 1).map(|(start, _)| *start),
        )?);
    }
    result.sort_by(|a, b| {
        start(a)
            .unwrap()
            .cmp(&start(b).unwrap())
            .then(event(a).cmp(&event(b)))
            .then(id_text(&a["id"]).cmp(&id_text(&b["id"])))
    });
    Ok(json!(result))
}

fn id_text(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}
