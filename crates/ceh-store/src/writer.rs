//! Typed writes compatible with the existing event/episode schema.
use crate::migrate::{StoreError, parse_time};
use ceh_core::{
    model::{DocumentId, Observation},
    statistics::{EventScope, EvidenceId, PublicEvent},
};
use chrono::{DateTime, Datelike, SecondsFormat, Utc};
use chrono_tz::Europe::Madrid;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn at(value: i64) -> Result<String, StoreError> {
    let time = DateTime::<Utc>::from_timestamp_millis(value)
        .filter(|time| (1..=9999).contains(&time.year()))
        .ok_or(StoreError::InvalidTimestamp)?;
    Ok(time.to_rfc3339_opts(
        if time.timestamp_subsec_micros() == 0 {
            SecondsFormat::Secs
        } else {
            SecondsFormat::Micros
        },
        false,
    ))
}
fn name<T: serde::Serialize>(value: &T) -> Result<String, StoreError> {
    serde_json::to_value(value)
        .map_err(|_| StoreError::Corrupt)?
        .as_str()
        .map(str::to_owned)
        .ok_or(StoreError::Corrupt)
}
fn activity_days(
    db: &Connection,
    scope: &EventScope,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<(), StoreError> {
    if start >= end {
        return Ok(());
    }
    let mut day = start.with_timezone(&Madrid).date_naive();
    let last = (end - chrono::Duration::microseconds(1))
        .with_timezone(&Madrid)
        .date_naive();
    while day <= last {
        db.execute(
            "INSERT OR IGNORE INTO activity_days VALUES (?1,?2)",
            params![scope.profile.as_str(), day.to_string()],
        )?;
        day = day.succ_opt().ok_or(StoreError::InvalidTimestamp)?;
    }
    Ok(())
}
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    Start,
    Finish,
    EffortStarted,
    FlowStart,
    FlowRestart,
    ManualRequired,
    Pause,
    Resume,
    Handoff,
    FatalError,
    HttpResponse,
    NetworkError,
    Timeout,
    ApplicantSubmitted,
    OfficeSelected,
    OfficeSubmitted,
    Recovery,
    RecoveryButton,
    InteractionRetry,
    Action,
    ActionCompleted,
    ActionInterrupted,
    Wait,
    BrowserSessionStarted,
}
#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ActivityEvidence {
    pub codes: Vec<String>,
    pub http_status: Option<u16>,
    pub browser_generation: Option<u64>,
    pub previous_stage: Option<ceh_core::model::PageKind>,
    pub initiator: Option<Initiator>,
}
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Initiator {
    User,
    Automatic,
}
pub struct StatisticsWriter<'a> {
    db: &'a mut Connection,
}
impl<'a> StatisticsWriter<'a> {
    pub fn new(db: &'a mut Connection) -> Self {
        Self { db }
    }
    pub fn activity(
        &mut self,
        scope: &EventScope,
        kind: ActivityKind,
        timestamp: i64,
        stage: Option<ceh_core::model::PageKind>,
        mode: &str,
        evidence: &ActivityEvidence,
    ) -> Result<(), StoreError> {
        let kind = name(&kind)?;
        let timestamp = at(timestamp)?;
        let stage = stage.as_ref().map(name).transpose()?.unwrap_or_default();
        let mode = if [
            "",
            "working",
            "waiting",
            "recovery",
            "manual",
            "paused",
            "finished",
            "booking_handoff",
            "unobserved",
            "unknown",
        ]
        .contains(&mode)
        {
            mode
        } else {
            "unknown"
        };
        const CODES: &[&str] = &[
            "loading",
            "unknown",
            "ambiguous",
            "known",
            "known_variant",
            "http_429",
            "http_5xx",
            "server_error",
            "rate_limited",
            "blocked",
            "expired",
            "no_appointments",
            "operation_code",
            "network_error",
            "timeout",
        ];
        let codes = evidence
            .codes
            .iter()
            .filter(|code| CODES.contains(&code.as_str()))
            .collect::<Vec<_>>();
        let mut safe = json!({"codes":codes,"http_status":evidence.http_status.filter(|status|(100..=599).contains(status))});
        if let Some(generation) = evidence.browser_generation {
            safe["browser_generation"] = json!(generation);
        }
        if let Some(stage) = evidence.previous_stage {
            safe["previous_stage"] =
                serde_json::to_value(stage).map_err(|_| StoreError::Corrupt)?;
        }
        if let Some(initiator) = evidence.initiator {
            safe["initiator"] = serde_json::to_value(initiator).map_err(|_| StoreError::Corrupt)?;
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("INSERT INTO events(profile,identity,kind,time,session,stage,mode,counted,offices,evidence) VALUES (?1,NULL,?2,?3,?4,?5,?6,0,'[]',?7)",params![scope.profile.as_str(),kind,timestamp,scope.session.as_str(),stage,mode,safe.to_string()])?;
        if ![
            "finish",
            "start",
            "pause",
            "resume",
            "handoff",
            "manual_required",
            "fatal_error",
        ]
        .contains(&kind.as_str())
        {
            tx.execute(
                "INSERT OR IGNORE INTO activity_days VALUES (?1,?2)",
                params![
                    scope.profile.as_str(),
                    parse_time(&timestamp)?
                        .with_timezone(&Madrid)
                        .date_naive()
                        .to_string()
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn counter(&mut self, event: &PublicEvent) -> Result<bool, StoreError> {
        let kind = name(&event.key.kind)?;
        let scope = &event.key.scope;
        let id = match &event.key.identity {
            EvidenceId::Document(id) => id.as_str(),
            EvidenceId::Operation(id) => id.as_str(),
        };
        let identity = digest(&format!(
            "{}:{}:{id}:{kind}",
            scope.profile.as_str(),
            scope.session.as_str()
        ));
        let timestamp = at(event.at)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut evidence = json!({"codes":[],"http_status":null});
        if event.booking_details.office.is_some() || !event.booking_details.offers.is_empty() {
            evidence["booking_details"] =
                serde_json::to_value(&event.booking_details).map_err(|_| StoreError::Corrupt)?;
        }
        let inserted=tx.execute("INSERT OR IGNORE INTO events(profile,identity,kind,time,session,stage,mode,counted,offices,evidence) VALUES (?1,?2,?3,?4,?5,'','',?6,?7,?8)",params![scope.profile.as_str(),identity,kind,timestamp,scope.session.as_str(),i64::from(event.counted),serde_json::to_string(&event.offices).map_err(|_|StoreError::Corrupt)?,evidence.to_string()])?;
        if inserted > 0 {
            let day = parse_time(&timestamp)?
                .with_timezone(&Madrid)
                .date_naive()
                .to_string();
            tx.execute(
                "INSERT OR IGNORE INTO activity_days VALUES (?1,?2)",
                params![scope.profile.as_str(), day],
            )?;
        }
        tx.commit()?;
        Ok(inserted > 0)
    }
    pub fn enrich_booking(
        &mut self,
        scope: &EventScope,
        document: &DocumentId,
        raw: &Value,
    ) -> Result<bool, StoreError> {
        let details = ceh_core::booking::normalize_booking_details(raw);
        if details.office.is_none() && details.offers.is_empty() {
            return Ok(false);
        }
        let identity = digest(&format!(
            "{}:{}:{}:booking",
            scope.profile.as_str(),
            scope.session.as_str(),
            document.as_str()
        ));
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old:Option<(i64,String)>=tx.query_row("SELECT id,evidence FROM events WHERE profile=?1 AND identity=?2 AND kind='booking'",params![scope.profile.as_str(),identity],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        let Some((id, raw)) = old else {
            return Ok(false);
        };
        let mut evidence: Value = serde_json::from_str(&raw).map_err(|_| StoreError::Corrupt)?;
        if !evidence.is_object() {
            evidence = json!({});
        }
        evidence["booking_details"] =
            serde_json::to_value(details).map_err(|_| StoreError::Corrupt)?;
        tx.execute(
            "UPDATE events SET evidence=?1 WHERE id=?2",
            params![evidence.to_string(), id],
        )?;
        tx.commit()?;
        Ok(true)
    }
    pub fn start_session(
        &mut self,
        scope: &EventScope,
        timestamp: i64,
        context: &Value,
    ) -> Result<(), StoreError> {
        let timestamp = at(timestamp)?;
        let mut safe = serde_json::Map::new();
        for (key, allowed) in [
            ("authentication", &["certificate", "clave"][..]),
            ("auth", &["certificate", "clave"]),
            ("clave_method", &["movil", "permanente"]),
            ("no_slots_mode", &["salir", "reload"]),
            ("applicant_no_slots_mode", &["wait", "continue"]),
            ("procedure", &["huellas", "regreso"]),
            ("browser", &["firefox", "chrome", "brave", "edge", "camoufox"]),
            ("app_version", &["historical"]),
        ] {
            if let Some(value) = context[key].as_str()
                && allowed.contains(&value)
            {
                safe.insert(key.into(), json!(value));
            }
        }
        for key in ["platform", "os"] {
            if let Some(value) = context[key].as_str()
                && ["Darwin", "Windows", "Linux"].contains(&value)
            {
                safe.insert(key.into(), json!(value));
            }
        }
        if let Some(value) = context["architecture"].as_str()
            && ["arm64", "aarch64", "x86_64", "AMD64", "i386", "x86"].contains(&value)
        {
            safe.insert("architecture".into(), json!(value));
        }
        if let Some(value) = context["app_version"].as_str()
            && regex::Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+(?:[.+-][A-Za-z0-9.-]+)?$")
                .expect("fixed version pattern")
                .is_match(value)
        {
            safe.insert("app_version".into(), json!(value));
        }
        static CATALOG: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| {
            serde_json::from_str(include_str!("../../../catalogs/snapshot-2026-08-12.json"))
                .expect("shipped public catalog")
        });
        if let Some(value) = context["province"].as_str()
            && CATALOG["selectors"]["provinces"]["options"]
                .as_array()
                .is_some_and(|options| options.iter().any(|option| option["label"] == value))
        {
            safe.insert("province".into(), json!(value));
        }
        self.db.execute(
            "INSERT OR IGNORE INTO sessions VALUES (?1,?2,?3,NULL,?3,'',?4)",
            params![
                scope.profile.as_str(),
                scope.session.as_str(),
                timestamp,
                Value::Object(safe).to_string()
            ],
        )?;
        Ok(())
    }
    pub fn end_session(
        &mut self,
        scope: &EventScope,
        timestamp: i64,
        reason: &str,
    ) -> Result<(), StoreError> {
        let timestamp = at(timestamp)?;
        let reason = if [
            "completed",
            "failed",
            "stopped",
            "interrupted",
            "historical_finish",
            "last_observation",
        ]
        .contains(&reason)
        {
            reason
        } else {
            "unknown"
        };
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "UPDATE sessions SET end=?1,last_seen=?1,reason=?2 WHERE profile=?3 AND session=?4",
            params![
                timestamp,
                reason,
                scope.profile.as_str(),
                scope.session.as_str()
            ],
        )?;
        tx.execute(
            "UPDATE episodes SET end=last_seen WHERE profile=?1 AND session=?2 AND end IS NULL",
            params![scope.profile.as_str(), scope.session.as_str()],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn heartbeat(&mut self, scope: &EventScope, timestamp: i64) -> Result<(), StoreError> {
        let timestamp = at(timestamp)?;
        let lower = parse_time(&timestamp)? - chrono::Duration::seconds(35);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "UPDATE sessions SET last_seen=?1 WHERE profile=?2 AND session=?3 AND end IS NULL",
            params![timestamp, scope.profile.as_str(), scope.session.as_str()],
        )?;
        let phases = {
            let mut statement=tx.prepare("SELECT id,last_seen FROM episodes WHERE profile=?1 AND session=?2 AND kind='phase' AND end IS NULL")?;
            statement
                .query_map(
                    params![scope.profile.as_str(), scope.session.as_str()],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
                )?
                .collect::<Result<Vec<_>, _>>()?
        };
        for (id, last) in phases {
            if parse_time(&last)? >= lower {
                tx.execute(
                    "UPDATE episodes SET last_seen=?1 WHERE id=?2",
                    params![timestamp, id],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }
    pub fn add_active(
        &mut self,
        scope: &EventScope,
        seconds: f64,
        timestamp: i64,
    ) -> Result<(), StoreError> {
        if !seconds.is_finite() || seconds < 0.0 || seconds * 1_000_000.0 >= i64::MAX as f64 {
            return Err(StoreError::InvalidTimestamp);
        }
        if seconds == 0.0 {
            return Ok(());
        }
        let end = parse_time(&at(timestamp)?)?;
        let start = end
            .checked_sub_signed(chrono::Duration::microseconds(
                (seconds * 1_000_000.0).round_ties_even() as i64,
            ))
            .ok_or(StoreError::InvalidTimestamp)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        activity_days(&tx, scope, start, end)?;
        let format = |value: DateTime<Utc>| {
            value.to_rfc3339_opts(
                if value.timestamp_subsec_micros() == 0 {
                    SecondsFormat::Secs
                } else {
                    SecondsFormat::Micros
                },
                false,
            )
        };
        tx.execute("INSERT OR IGNORE INTO active(profile,session,start,end,seconds) VALUES (?1,?2,?3,?4,?5)",params![scope.profile.as_str(),scope.session.as_str(),format(start),format(end),seconds])?;
        tx.commit()?;
        Ok(())
    }
    pub fn page(
        &mut self,
        scope: &EventScope,
        observed: &Observation,
        timestamp: i64,
    ) -> Result<(), StoreError> {
        let stage = name(&observed.kind)?;
        let recognition = if stage == "loading" {
            "loading"
        } else if stage == "unknown" {
            "unknown"
        } else {
            "known"
        };
        let codes = if observed.operation_code {
            vec!["operation_code"]
        } else {
            vec![]
        };
        self.episode(
            scope,
            "page",
            &stage,
            "",
            observed.document.as_str(),
            timestamp,
            recognition,
            &codes,
            &observed.variant,
        )
    }
    pub fn phase(
        &mut self,
        scope: &EventScope,
        mode: &str,
        timestamp: i64,
    ) -> Result<(), StoreError> {
        if mode == "unobserved" {
            self.db.execute(
                "UPDATE episodes SET end=last_seen WHERE profile=?1 AND session=?2 AND end IS NULL",
                params![scope.profile.as_str(), scope.session.as_str()],
            )?;
        }
        let mode = if [
            "working",
            "waiting",
            "recovery",
            "manual",
            "paused",
            "finished",
            "booking_handoff",
            "unobserved",
            "unknown",
        ]
        .contains(&mode)
        {
            mode
        } else {
            "unknown"
        };
        self.episode(scope, "phase", "", mode, "", timestamp, "known", &[], "")
    }
    #[allow(clippy::too_many_arguments)]
    fn episode(
        &mut self,
        scope: &EventScope,
        kind: &str,
        stage: &str,
        mode: &str,
        document: &str,
        timestamp: i64,
        recognition: &str,
        codes: &[&str],
        variant: &str,
    ) -> Result<(), StoreError> {
        let timestamp = at(timestamp)?;
        let variant = if variant.len() == 64
            && variant
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            variant
        } else {
            ""
        };
        let document = digest(document);
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if kind == "page" && stage == "applicant_no_slots" {
            // One banner per (session, document), counted at its earliest observation (Python 0.3.x).
            tx.execute(
                "INSERT INTO applicant_notices VALUES (?1,?2,?3,?4) ON CONFLICT(profile,session,document) DO UPDATE SET start=MIN(start,excluded.start)",
                params![scope.profile.as_str(), scope.session.as_str(), document, timestamp],
            )?;
        }
        type Current = (i64, String, String, String, String, String, String);
        let current:Option<Current>=tx.query_row("SELECT id,stage,mode,last_seen,recognition,evidence,variant FROM episodes WHERE profile=?1 AND session=?2 AND kind=?3 AND end IS NULL ORDER BY id DESC LIMIT 1",params![scope.profile.as_str(),scope.session.as_str(),kind],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional()?;
        if let Some((_, old_stage, _, _, _, _, _)) = &current
            && kind == "page"
            && stage == "loading"
            && [
                "expired",
                "blocked",
                "server_error",
                "rate_limited",
                "unknown",
            ]
            .contains(&old_stage.as_str())
        {
            return Ok(());
        }
        let mut continuous = if let Some((_, _, _, last, _, _, _)) = &current {
            (parse_time(&timestamp)? - parse_time(last)?) <= chrono::Duration::seconds(35)
        } else {
            false
        };
        if current.is_some() && kind == "page" && !continuous {
            let pulse:Option<String>=tx.query_row("SELECT last_seen FROM episodes WHERE profile=?1 AND session=?2 AND kind='phase' AND end IS NULL AND mode!='unobserved' ORDER BY id DESC LIMIT 1",params![scope.profile.as_str(),scope.session.as_str()],|r|r.get(0)).optional()?;
            if let Some(pulse) = pulse {
                let delta = parse_time(&timestamp)? - parse_time(&pulse)?;
                continuous =
                    delta >= chrono::Duration::zero() && delta <= chrono::Duration::seconds(35);
            }
        }
        let same =
            if let Some((_, old_stage, old_mode, _, old_recognition, old_codes, old_variant)) =
                &current
            {
                let old_codes: Vec<String> =
                    serde_json::from_str(old_codes).map_err(|_| StoreError::Corrupt)?;
                old_stage == stage
                    && old_mode == mode
                    && (kind != "page"
                        || (old_recognition == recognition
                            && old_codes
                                .iter()
                                .map(String::as_str)
                                .collect::<std::collections::BTreeSet<_>>()
                                == codes.iter().copied().collect()
                            && old_variant == variant))
            } else {
                false
            };
        if continuous && same {
            let (id, _, _, _, _, _, old_variant) = current.unwrap();
            tx.execute("UPDATE episodes SET last_seen=?1,recognition=?2,evidence=?3,variant=?4 WHERE id=?5",params![timestamp,recognition,serde_json::to_string(codes).map_err(|_|StoreError::Corrupt)?,if variant.is_empty(){old_variant.as_str()}else{variant},id])?;
        } else {
            if let Some((id, _, _, last, _, _, _)) = current {
                tx.execute(
                    "UPDATE episodes SET end=?1,last_seen=?2,confirmed=?3 WHERE id=?4",
                    params![
                        if continuous {
                            timestamp.as_str()
                        } else {
                            last.as_str()
                        },
                        if continuous && kind != "page" {
                            timestamp.as_str()
                        } else {
                            last.as_str()
                        },
                        i64::from(continuous),
                        id
                    ],
                )?;
            }
            tx.execute("INSERT INTO episodes(profile,session,kind,stage,mode,document,start,last_seen,recognition,evidence,variant) VALUES (?1,?2,?3,?4,?5,?6,?7,?7,?8,?9,?10)",params![scope.profile.as_str(),scope.session.as_str(),kind,stage,mode,document,timestamp,recognition,serde_json::to_string(codes).map_err(|_|StoreError::Corrupt)?,variant])?;
        }
        tx.commit()?;
        Ok(())
    }
}
