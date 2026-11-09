//! Read-only queries over the existing statistics schema.
use crate::{
    migrate::StoreError,
    periods::{Period, bounds},
};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Appearance {
    pub time: String,
    pub session_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct OfficeHistory {
    pub label: String,
    pub count: Option<u64>,
    pub last_seen: Option<String>,
    pub appearances: Vec<Appearance>,
}
pub struct StatisticsReader<'a> {
    db: &'a Connection,
}
impl<'a> StatisticsReader<'a> {
    pub fn new(db: &'a Connection) -> Self {
        Self { db }
    }
    fn baseline(&self, profile: &str, period: Period) -> Result<Option<Value>, StoreError> {
        if period != Period::All {
            return Ok(None);
        }
        let raw: Option<String> = self
            .db
            .query_row(
                "SELECT data FROM baseline WHERE profile=?1",
                [profile],
                |r| r.get(0),
            )
            .optional()?;
        raw.map(|v| serde_json::from_str(&v).map_err(|_| StoreError::Corrupt))
            .transpose()
    }
    pub fn total(
        &self,
        profile: &str,
        period: Period,
        now: DateTime<Utc>,
    ) -> Result<u64, StoreError> {
        let b = bounds(period, now)?;
        let lower = b.start.map(|v| v.to_rfc3339());
        let upper = b.end.map(|v| v.to_rfc3339());
        let count:i64=self.db.query_row("SELECT COUNT(*) FROM events WHERE profile=?1 AND kind='check' AND counted=1 AND (?2 IS NULL OR time>=?2) AND (?3 IS NULL OR time<?3)",params![profile,lower,upper],|r|r.get(0))?;
        let legacy = self
            .baseline(profile, period)?
            .and_then(|v| v.get("checks").and_then(Value::as_u64))
            .unwrap_or(0);
        u64::try_from(count)
            .map_err(|_| StoreError::Corrupt)?
            .checked_add(legacy)
            .ok_or(StoreError::Corrupt)
    }
    pub fn summary(
        &self,
        profile: &str,
        period: Period,
        now: DateTime<Utc>,
    ) -> Result<Value, StoreError> {
        let mapping = [
            ("check", "checks"),
            ("office", "offices_seen"),
            ("server_error", "server_errors"),
            ("rate_limited", "rate_limited"),
            ("blocked", "blocked"),
            ("booking", "booking_screens"),
            ("expired", "expired_sessions"),
            ("pass", "flow_passes"),
            ("automation_action", "actions_performed"),
            ("processed_page", "pages_processed"),
        ];
        let baseline = self.baseline(profile, period)?;
        let historical = baseline.is_some();
        let provenance: Option<(String, String)> = if historical {
            self.db
                .query_row(
                    "SELECT data,migrated_at FROM baseline_provenance WHERE profile=?1",
                    [profile],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?
        } else {
            None
        };
        let origin = provenance
            .as_ref()
            .map(|(v, _)| serde_json::from_str::<Value>(v).map_err(|_| StoreError::Corrupt))
            .transpose()?
            .unwrap_or_else(|| serde_json::json!({"known":[],"positive":[],"active_seconds":null}));
        let known = origin["known"].as_array().ok_or(StoreError::Corrupt)?;
        let positive = origin["positive"].as_array().ok_or(StoreError::Corrupt)?;
        let b = bounds(period, now)?;
        let lower = b.start.map(|v| v.to_rfc3339());
        let upper = b.end.map(|v| v.to_rfc3339());
        let mut row = serde_json::Map::new();
        for (kind, counter) in mapping {
            let count:i64=self.db.query_row("SELECT COUNT(*) FROM events WHERE profile=?1 AND kind=?2 AND counted=1 AND (kind!='office' OR offices!='[]') AND (?3 IS NULL OR time>=?3) AND (?4 IS NULL OR time<?4)",params![profile,kind,lower,upper],|r|r.get(0))?;
            let old = baseline
                .as_ref()
                .and_then(|v| v.get(counter))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            row.insert(
                counter.into(),
                serde_json::json!(
                    old.checked_add(u64::try_from(count).map_err(|_| StoreError::Corrupt)?)
                        .ok_or(StoreError::Corrupt)?
                ),
            );
        }
        let first = |kind: Option<&str>| -> Result<Option<String>, StoreError> {
            Ok(self.db.query_row("SELECT MIN(time) FROM events WHERE profile=?1 AND (?2 IS NULL OR kind=?2) AND (?2!='office' OR offices!='[]' OR ?2 IS NULL) AND (?3 IS NULL OR time>=?3) AND (?4 IS NULL OR time<?4)",params![profile,kind,lower,upper],|r|r.get(0))?)
        };
        let mut started = baseline
            .as_ref()
            .and_then(|v| v.get("started_at"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let earliest = first(None)?;
        if earliest
            .as_ref()
            .is_some_and(|v| started.as_ref().is_none_or(|s| v < s))
        {
            started = earliest;
        }
        let mut active = self
            .db
            .prepare("SELECT start,end FROM active WHERE profile=?1")?;
        for interval in active.query_map([profile], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            let (start, end) = interval?;
            let begin = crate::migrate::parse_time(&start)?;
            let finish = crate::migrate::parse_time(&end)?;
            if b.start.is_some_and(|l| finish <= l) || b.end.is_some_and(|u| begin >= u) {
                continue;
            }
            let start = b.start.map_or(begin, |l| begin.max(l)).to_rfc3339();
            if started.as_ref().is_none_or(|v| &start < v) {
                started = Some(start);
            }
        }
        row.insert(
            "active_seconds".into(),
            serde_json::json!(self.active_seconds(profile, period, now)?),
        );
        row.insert("started_at".into(), serde_json::json!(started));
        let effort:Option<String>=self.db.query_row("SELECT MIN(time) FROM events WHERE profile=?1 AND kind IN ('effort_started','automation_action','processed_page')",[profile],|r|r.get(0))?;
        row.insert("effort_started_at".into(), serde_json::json!(effort));
        let offices = self.known_offices(profile, period, now)?;
        row.insert("unique_offices".into(), serde_json::json!(offices.len()));
        let days:i64=self.db.query_row("SELECT COUNT(*) FROM activity_days WHERE profile=?1 AND (?2 IS NULL OR day>=?2) AND (?3 IS NULL OR day<?3)",params![profile,b.start.map(|v|v.with_timezone(&chrono_tz::Europe::Madrid).date_naive().to_string()),b.end.map(|v|v.with_timezone(&chrono_tz::Europe::Madrid).date_naive().to_string())],|r|r.get(0))?;
        row.insert("days".into(), serde_json::json!(days));
        let mut availability = baseline
            .as_ref()
            .and_then(|v| v.get("first_available_at"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let mut elapsed = baseline
            .as_ref()
            .and_then(|v| v.get("first_available_seconds"))
            .cloned()
            .unwrap_or(Value::Null);
        for (kind, counter, other, prefix) in [
            ("office", "offices_seen", "booking_screens", "first_office"),
            (
                "booking",
                "booking_screens",
                "offices_seen",
                "first_booking",
            ),
        ] {
            let mut value = first(Some(kind))?;
            let mut legacy = false;
            if let Some(old) = origin["legacy_first_available_at"].as_str()
                && positive.contains(&serde_json::json!(counter))
                && known.contains(&serde_json::json!(other))
                && !positive.contains(&serde_json::json!(other))
                && value.as_ref().is_none_or(|v| old <= v.as_str())
            {
                value = Some(old.to_owned());
                legacy = true;
            }
            let absent = !historical
                || (known.contains(&serde_json::json!(counter))
                    && !positive.contains(&serde_json::json!(counter)));
            let status = if value.is_some() {
                if absent || legacy {
                    "known"
                } else {
                    "recorded"
                }
            } else if absent {
                "not_seen"
            } else {
                "unknown"
            };
            let (seconds, quality) = if let Some(value) = &value {
                self.first_active(
                    profile,
                    value,
                    b,
                    if historical {
                        Some((
                            &origin,
                            provenance.as_ref().map(|(_, at)| at.as_str()),
                            absent,
                        ))
                    } else {
                        None
                    },
                )?
            } else {
                (None, "unknown")
            };
            row.insert(format!("{prefix}_at"), serde_json::json!(value));
            row.insert(format!("{prefix}_seconds"), serde_json::json!(seconds));
            row.insert(format!("{prefix}_status"), serde_json::json!(status));
            row.insert(
                format!("{prefix}_active_quality"),
                serde_json::json!(quality),
            );
            if value
                .as_ref()
                .is_some_and(|v| availability.as_ref().is_none_or(|old| v < old))
            {
                availability = value;
                if let (Some(at), Some(start)) = (&availability, &started) {
                    elapsed = serde_json::json!(crate::periods::clipped_seconds(
                        crate::migrate::parse_time(start)?,
                        crate::migrate::parse_time(at)?,
                        crate::periods::Bounds {
                            start: None,
                            end: None
                        }
                    ));
                }
            }
        }
        row.insert("first_available_at".into(), serde_json::json!(availability));
        row.insert("first_available_seconds".into(), elapsed);
        row.insert(
            "legacy_first_available_at".into(),
            origin
                .get("legacy_first_available_at")
                .cloned()
                .unwrap_or(Value::Null),
        );
        row.insert(
            "legacy_first_available_elapsed_seconds".into(),
            origin
                .get("legacy_first_available_elapsed_seconds")
                .cloned()
                .unwrap_or(Value::Null),
        );
        let fields = [
            "checks",
            "offices_seen",
            "server_errors",
            "rate_limited",
            "blocked",
            "booking_screens",
            "expired_sessions",
            "flow_passes",
            "actions_performed",
            "pages_processed",
            "active_seconds",
            "started_at",
            "first_available_at",
            "first_available_seconds",
            "_offices",
        ];
        row.insert(
            "legacy_unknown_fields".into(),
            serde_json::json!(
                fields
                    .into_iter()
                    .filter(|field| historical && !known.contains(&serde_json::json!(field)))
                    .map(|field| if field == "_offices" {
                        "unique_offices"
                    } else {
                        field
                    })
                    .collect::<Vec<_>>()
            ),
        );
        row.insert(
            "legacy_baseline".into(),
            serde_json::json!(self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM baseline WHERE profile=?1)",
                [profile],
                |r| r.get::<_, bool>(0)
            )?),
        );
        Ok(Value::Object(row))
    }
    fn first_active(
        &self,
        profile: &str,
        value: &str,
        b: crate::periods::Bounds,
        historical: Option<(&Value, Option<&str>, bool)>,
    ) -> Result<(Option<f64>, &'static str), StoreError> {
        let at = crate::migrate::parse_time(value)?;
        let mut observed = Vec::new();
        let mut active = Vec::new();
        for (table, target) in [("active", &mut active), ("observed_active", &mut observed)] {
            let mut stmt = self
                .db
                .prepare(&format!("SELECT start,end FROM {table} WHERE profile=?1"))?;
            for row in stmt.query_map([profile], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })? {
                let (start, end) = row?;
                let start = crate::migrate::parse_time(&start)?;
                let end = crate::migrate::parse_time(&end)?;
                if start < at && b.start.is_none_or(|l| end > l) {
                    target.push((start, end));
                }
            }
        }
        let limit = crate::periods::Bounds {
            start: b.start,
            end: Some(at),
        };
        if !observed.is_empty() {
            active.extend(observed);
            return Ok((
                Some(crate::periods::union_seconds(&active, limit)),
                "minimum",
            ));
        }
        let mut seconds = 0.0;
        let mut stmt = self
            .db
            .prepare("SELECT start,end,seconds FROM active WHERE profile=?1")?;
        for row in stmt.query_map([profile], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, f64>(2)?,
            ))
        })? {
            let (start, end, recorded) = row?;
            let start = crate::migrate::parse_time(&start)?;
            let end = crate::migrate::parse_time(&end)?;
            if start >= at || b.start.is_some_and(|l| end <= l) {
                continue;
            }
            seconds += if end <= at && b.start.is_none_or(|l| start >= l) {
                recorded
            } else {
                crate::periods::clipped_seconds(start, end, limit)
            };
        }
        if let Some((origin, migrated, absent)) = historical {
            if let Some(old) = origin["active_seconds"].as_f64()
                && absent
                && (old == 0.0 || migrated.is_some_and(|time| value >= time))
            {
                seconds += old;
            } else {
                return Ok((
                    if seconds != 0.0 { Some(seconds) } else { None },
                    if seconds != 0.0 { "minimum" } else { "unknown" },
                ));
            }
        }
        Ok((Some(seconds), "exact"))
    }
    pub fn active_seconds(
        &self,
        profile: &str,
        period: Period,
        now: DateTime<Utc>,
    ) -> Result<f64, StoreError> {
        let b = bounds(period, now)?;
        let mut total = self
            .baseline(profile, period)?
            .and_then(|v| v.get("active_seconds").and_then(Value::as_f64))
            .unwrap_or(0.0);
        let mut stmt = self
            .db
            .prepare("SELECT start,end,seconds FROM active WHERE profile=?1")?;
        for row in stmt.query_map([profile], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, f64>(2)?,
            ))
        })? {
            let (start, end, seconds) = row?;
            if !seconds.is_finite() || seconds < 0.0 {
                return Err(StoreError::Corrupt);
            }
            let start = crate::migrate::parse_time(&start)?;
            let end = crate::migrate::parse_time(&end)?;
            if b.start.is_none_or(|lower| start >= lower) && b.end.is_none_or(|upper| end <= upper)
            {
                total += seconds;
            } else {
                total += crate::periods::clipped_seconds(start, end, b);
            }
        }
        if total.is_finite() {
            Ok(total)
        } else {
            Err(StoreError::Corrupt)
        }
    }
    pub fn history(
        &self,
        profile: &str,
        period: Period,
        now: DateTime<Utc>,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    ) -> Result<Vec<Value>, StoreError> {
        let b = bounds(period, now)?;
        let lower = start.or(b.start).map(|v| v.to_rfc3339());
        let upper = end.or(b.end).map(|v| v.to_rfc3339());
        let mut result = Vec::new();
        let queries = [
            "SELECT json_object('kind',kind,'stage',stage,'mode',mode,'start',time,'end',time,'last_seen',time,'session_id',session,'confirmed_end',json('true'),'event',json('true'),'counted',json(CASE WHEN counted!=0 THEN 'true' ELSE 'false' END),'offices',json(offices),'evidence',json(evidence),'id',id) FROM events WHERE profile=?1 AND (?2 IS NULL OR time>=?2) AND (?3 IS NULL OR time<?3) ORDER BY time,id",
            "SELECT json_object('kind',kind,'stage',stage,'mode',mode,'start',start,'end',end,'last_seen',last_seen,'session_id',session,'confirmed_end',json(CASE WHEN confirmed!=0 THEN 'true' ELSE 'false' END),'event',json('false'),'id',id,'recognition',recognition,'evidence',json(evidence),'variant',variant) FROM episodes WHERE profile=?1 AND (?2 IS NULL OR COALESCE(end,last_seen)>=?2) AND (?3 IS NULL OR start<?3) ORDER BY start,id",
        ];
        for query in queries {
            let mut stmt = self.db.prepare(query)?;
            for raw in stmt.query_map(params![profile, lower, upper], |r| r.get::<_, String>(0))? {
                let row: Value = serde_json::from_str(&raw?).map_err(|_| StoreError::Corrupt)?;
                result.push(row);
            }
        }
        result.sort_by(|a, b| a["start"].as_str().cmp(&b["start"].as_str()));
        Ok(result)
    }
    pub fn history_bounds(
        &self,
        profile: &str,
        period: Period,
        now: DateTime<Utc>,
    ) -> Result<Value, StoreError> {
        let b = bounds(period, now)?;
        let lower = b.start.map(|v| v.to_rfc3339());
        let upper = b.end.map(|v| v.to_rfc3339());
        let (event_start,event_end):(Option<String>,Option<String>)=self.db.query_row("SELECT MIN(time),MAX(time) FROM events WHERE profile=?1 AND (?2 IS NULL OR time>=?2) AND (?3 IS NULL OR time<?3)",params![profile,lower,upper],|r|Ok((r.get(0)?,r.get(1)?)))?;
        let (page_start,page_end):(Option<String>,Option<String>)=self.db.query_row("SELECT MIN(start),MAX(COALESCE(end,last_seen)) FROM episodes WHERE profile=?1 AND (?2 IS NULL OR COALESCE(end,last_seen)>=?2) AND (?3 IS NULL OR start<?3)",params![profile,lower,upper],|r|Ok((r.get(0)?,r.get(1)?)))?;
        Ok(
            serde_json::json!({"start":event_start.into_iter().chain(page_start).min(),"end":event_end.into_iter().chain(page_end).max()}),
        )
    }
    pub fn timeline_bounds(
        &self,
        profile: &str,
        period: Period,
        now: DateTime<Utc>,
    ) -> Result<Value, StoreError> {
        let mut result = self.history_bounds(profile, period, now)?;
        let b = bounds(period, now)?;
        let (start,end):(Option<String>,Option<String>)=self.db.query_row("SELECT MIN(start),MAX(COALESCE(end,last_seen)) FROM sessions WHERE profile=?1 AND (?2 IS NULL OR COALESCE(end,last_seen)>=?2) AND (?3 IS NULL OR start<?3)",params![profile,b.start.map(|v|v.to_rfc3339()),b.end.map(|v|v.to_rfc3339())],|r|Ok((r.get(0)?,r.get(1)?)))?;
        result["start"] = serde_json::json!(
            result["start"]
                .as_str()
                .map(str::to_owned)
                .into_iter()
                .chain(start)
                .min()
        );
        result["end"] = serde_json::json!(
            result["end"]
                .as_str()
                .map(str::to_owned)
                .into_iter()
                .chain(end)
                .max()
        );
        Ok(result)
    }
    pub fn timeline_history(
        &self,
        profile: &str,
        period: Period,
        now: DateTime<Utc>,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    ) -> Result<Value, StoreError> {
        let b = bounds(period, now)?;
        let lower = start.or(b.start);
        let upper = end.or(b.end);
        let mut stmt=self.db.prepare("SELECT json_object('profile',profile,'session',session,'start',start,'end',end,'last_seen',last_seen,'reason',reason,'context',context) FROM sessions WHERE profile=?1 AND (?2 IS NULL OR COALESCE(end,last_seen)>=?2) AND (?3 IS NULL OR start<?3) ORDER BY start")?;
        let mut sessions = Vec::new();
        for raw in stmt.query_map(
            params![
                profile,
                lower.map(|v| v.to_rfc3339()),
                upper.map(|v| v.to_rfc3339())
            ],
            |r| r.get::<_, String>(0),
        )? {
            sessions.push(serde_json::from_str::<Value>(&raw?).map_err(|_| StoreError::Corrupt)?);
        }
        let context = sessions
            .iter()
            .filter_map(|row| row["start"].as_str())
            .map(crate::migrate::parse_time)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .chain(lower)
            .min();
        let rows = self.history(profile, Period::All, now, context, upper)?;
        let projected = crate::timeline::build_timeline(&rows, &sessions)?;
        let filtered = projected
            .as_array()
            .ok_or(StoreError::Corrupt)?
            .iter()
            .filter(|row| {
                let start = row["start"]
                    .as_str()
                    .and_then(|v| crate::migrate::parse_time(v).ok());
                let end = row["end"]
                    .as_str()
                    .or(row["last_seen"].as_str())
                    .and_then(|v| crate::migrate::parse_time(v).ok());
                start.is_some_and(|v| upper.is_none_or(|u| v < u))
                    && end.is_some_and(|v| lower.is_none_or(|l| v >= l))
            })
            .cloned()
            .collect::<Vec<_>>();
        Ok(serde_json::json!(filtered))
    }
    pub fn office_history(
        &self,
        profile: &str,
        period: Period,
        now: DateTime<Utc>,
    ) -> Result<Vec<OfficeHistory>, StoreError> {
        let b = bounds(period, now)?;
        self.office_history_between(profile, b.start, b.end)
    }
    fn office_history_between(
        &self,
        profile: &str,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    ) -> Result<Vec<OfficeHistory>, StoreError> {
        let lower = start.map(|v| v.to_rfc3339());
        let upper = end.map(|v| v.to_rfc3339());
        let mut stmt=self.db.prepare("SELECT time,session,offices FROM events WHERE profile=?1 AND kind='office' AND (?2 IS NULL OR time>=?2) AND (?3 IS NULL OR time<?3) ORDER BY time,id")?;
        let mut result: BTreeMap<String, (usize, OfficeHistory)> = BTreeMap::new();
        let mut order = 0;
        for row in stmt.query_map(params![profile, lower, upper], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })? {
            let (time, session, raw) = row?;
            let labels: Vec<String> =
                serde_json::from_str(&raw).map_err(|_| StoreError::Corrupt)?;
            for label in labels {
                let row = result.entry(label.clone()).or_insert_with(|| {
                    order += 1;
                    (
                        order,
                        OfficeHistory {
                            label,
                            count: Some(0),
                            last_seen: None,
                            appearances: Vec::new(),
                        },
                    )
                });
                row.1.count = Some(
                    row.1
                        .count
                        .unwrap_or(0)
                        .checked_add(1)
                        .ok_or(StoreError::Corrupt)?,
                );
                row.1.last_seen = Some(time.clone());
                row.1.appearances.push(Appearance {
                    time: time.clone(),
                    session_id: session.clone(),
                });
            }
        }
        if start.is_none()
            && end.is_none()
            && let Some(baseline) = self.baseline(profile, Period::All)?
            && let Some(labels) = baseline.get("_offices").and_then(Value::as_array)
        {
            for label in labels {
                let label = label.as_str().ok_or(StoreError::Corrupt)?.to_string();
                result.entry(label.clone()).or_insert_with(|| {
                    order += 1;
                    (
                        order,
                        OfficeHistory {
                            label,
                            count: None,
                            last_seen: None,
                            appearances: Vec::new(),
                        },
                    )
                });
            }
        }
        let mut rows = result.into_values().collect::<Vec<_>>();
        rows.sort_by(|a, b| b.1.last_seen.cmp(&a.1.last_seen).then(a.0.cmp(&b.0)));
        Ok(rows.into_iter().map(|(_, row)| row).collect())
    }
    pub fn known_offices(
        &self,
        profile: &str,
        period: Period,
        now: DateTime<Utc>,
    ) -> Result<Vec<String>, StoreError> {
        let mut labels = self
            .office_history(profile, period, now)?
            .into_iter()
            .map(|row| row.label)
            .collect::<Vec<_>>();
        labels.sort();
        Ok(labels)
    }
    pub fn last_office(
        &self,
        profile: &str,
        period: Period,
        now: DateTime<Utc>,
    ) -> Result<String, StoreError> {
        let rows = self.office_history(profile, period, now)?;
        // Python resolves equal timestamps by the last row in its stable history order.
        if let Some(row) = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.last_seen.is_some())
            .max_by(|(ai, a), (bi, b)| a.last_seen.cmp(&b.last_seen).then(ai.cmp(bi)))
        {
            return Ok(row.1.label.clone());
        }
        Ok(self
            .baseline(profile, period)?
            .and_then(|v| {
                v.get("_last_office")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_default())
    }
}

/// A coherent read of the dashboard: totals and office history share one SQLite snapshot.
/// Unknown legacy values retain their null/provenance representation from the reader.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Dashboard {
    pub all: Value,
    pub today: Value,
    pub offices: Vec<OfficeHistory>,
}
pub fn dashboard(
    db: &Connection,
    profile: &str,
    now: DateTime<Utc>,
) -> Result<Dashboard, StoreError> {
    let transaction = db.unchecked_transaction()?;
    let reader = StatisticsReader::new(&transaction);
    let result = Dashboard {
        all: reader.summary(profile, Period::All, now)?,
        today: reader.summary(profile, Period::Today, now)?,
        offices: reader.office_history(profile, Period::All, now)?,
    };
    transaction.commit()?;
    Ok(result)
}

/// Read existing storage only. Missing history is distinct from a failed query.
/// Schema migration belongs to startup compatibility, never to a dashboard read.
fn existing_database(path: &std::path::Path) -> Result<Option<Connection>, StoreError> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(StoreError::Unavailable),
        Ok(metadata) if !metadata.file_type().is_file() => return Err(StoreError::Unavailable),
        Ok(_) => {}
    }
    let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    db.busy_timeout(std::time::Duration::from_millis(100))?;
    Ok(Some(db))
}
pub fn dashboard_file(
    path: &std::path::Path,
    profile: &str,
    now: DateTime<Utc>,
) -> Result<Option<Dashboard>, StoreError> {
    existing_database(path)?
        .map(|db| dashboard(&db, profile, now))
        .transpose()
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct HistorySnapshot {
    pub offices: Vec<OfficeHistory>,
    pub bounds: Value,
    pub timeline: Value,
}
pub fn history_snapshot(
    db: &Connection,
    profile: &str,
    now: DateTime<Utc>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
) -> Result<HistorySnapshot, StoreError> {
    if start.zip(end).is_some_and(|(start, end)| start >= end) {
        return Err(StoreError::InvalidTimestamp);
    }
    let transaction = db.unchecked_transaction()?;
    let reader = StatisticsReader::new(&transaction);
    let result = HistorySnapshot {
        offices: reader.office_history_between(profile, start, end)?,
        bounds: reader.timeline_bounds(profile, Period::All, now)?,
        timeline: reader.timeline_history(profile, Period::All, now, start, end)?,
    };
    transaction.commit()?;
    Ok(result)
}
pub fn history_file(
    path: &std::path::Path,
    profile: &str,
    now: DateTime<Utc>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
) -> Result<Option<HistorySnapshot>, StoreError> {
    existing_database(path)?
        .map(|db| history_snapshot(&db, profile, now, start, end))
        .transpose()
}

/// Stable UI key; an event and clip never share the same identity namespace.
pub fn timeline_key(row: &Value) -> String {
    format!(
        "{}:{}",
        if row["event"] == true {
            "event"
        } else {
            "clip"
        },
        row["id"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| row["id"].to_string())
    )
}
pub fn selected_snapshot(
    db: &Connection,
    profile: &str,
    now: DateTime<Utc>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    ids: &[String],
) -> Result<Value, StoreError> {
    if ids.len() > 512
        || ids.iter().any(|id| id.len() > 256)
        || start.zip(end).is_some_and(|(a, b)| a >= b)
    {
        return Err(StoreError::InvalidLegacy);
    }
    let transaction = db.unchecked_transaction()?;
    let reader = StatisticsReader::new(&transaction);
    let rows = reader.timeline_history(profile, Period::All, now, start, end)?;
    let requested = ids.iter().collect::<std::collections::BTreeSet<_>>();
    let mut found = std::collections::BTreeSet::new();
    let mut selected = Vec::new();
    for row in rows.as_array().ok_or(StoreError::Corrupt)? {
        let key = timeline_key(row);
        if !requested.contains(&key) {
            continue;
        }
        found.insert(key);
        let mut row = row.clone();
        if row["event"] != true {
            let a = crate::migrate::parse_time(row["start"].as_str().ok_or(StoreError::Corrupt)?)?;
            let b = crate::migrate::parse_time(
                row["end"]
                    .as_str()
                    .or(row["last_seen"].as_str())
                    .ok_or(StoreError::Corrupt)?,
            )?;
            row["_display_start"] =
                serde_json::json!(start.map_or(a, |lower| lower.max(a)).to_rfc3339());
            row["_display_end"] =
                serde_json::json!(end.map_or(b, |upper| upper.min(b)).to_rfc3339());
        }
        selected.push(row);
    }
    if found.len() != requested.len() {
        return Err(StoreError::InvalidLegacy);
    }
    let history = reader.history(profile, Period::All, now, None, end)?;
    let mut result = crate::selection::selection_summary(&selected, &history)?;
    let first = selected
        .iter()
        .filter_map(|row| row["_display_start"].as_str().or(row["start"].as_str()))
        .map(crate::migrate::parse_time)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min();
    let last = selected
        .iter()
        .filter_map(|row| {
            row["_display_end"]
                .as_str()
                .or(row["end"].as_str())
                .or(row["last_seen"].as_str())
        })
        .map(crate::migrate::parse_time)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max();
    result["start"] = serde_json::json!(first.map(|at| at.to_rfc3339()));
    result["end"] = serde_json::json!(last.map(|at| at.to_rfc3339()));
    transaction.commit()?;
    Ok(result)
}
pub fn selection_file(
    path: &std::path::Path,
    profile: &str,
    now: DateTime<Utc>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    ids: &[String],
) -> Result<Option<Value>, StoreError> {
    existing_database(path)?
        .map(|db| selected_snapshot(&db, profile, now, start, end, ids))
        .transpose()
}

/// Known office options and the true last observed office share one read snapshot.
pub fn appointment_offices_file(
    path: &std::path::Path,
    profile: &str,
    now: DateTime<Utc>,
) -> Result<Option<(Vec<String>, String)>, StoreError> {
    let Some(db) = existing_database(path)? else {
        return Ok(None);
    };
    let tx = db.unchecked_transaction()?;
    let reader = StatisticsReader::new(&tx);
    let result = (
        reader.known_offices(profile, Period::All, now)?,
        reader.last_office(profile, Period::All, now)?,
    );
    tx.commit()?;
    Ok(Some(result))
}
