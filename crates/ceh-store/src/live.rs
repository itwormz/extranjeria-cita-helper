//! Real session writes into the same SQLite schema as migrated history.
//! No diagnostic captures, browser values, credentials or file paths enter this API.
use crate::{
    migrate::StoreError,
    writer::{ActivityEvidence, ActivityKind, StatisticsWriter},
};
use ceh_core::{control::ControlMode, model::*, statistics::*};
use rusqlite::Connection;
use serde_json::Value;
pub struct LiveStatistics {
    scope: EventScope,
    ledger: StatisticsLedger,
    phase: &'static str,
    last_mono: u64,
    finished: bool,
}
impl LiveStatistics {
    pub fn start(
        db: &mut Connection,
        scope: EventScope,
        context: &Value,
        wall: i64,
        mono: u64,
    ) -> Result<Self, StoreError> {
        let mut writer = StatisticsWriter::new(db);
        writer.start_session(&scope, wall, context)?;
        writer.activity(
            &scope,
            ActivityKind::Start,
            wall,
            None,
            "working",
            &ActivityEvidence::default(),
        )?;
        writer.activity(
            &scope,
            ActivityKind::EffortStarted,
            wall,
            None,
            "working",
            &ActivityEvidence::default(),
        )?;
        writer.phase(&scope, "working", wall)?;
        Ok(Self {
            scope,
            ledger: StatisticsLedger::new(),
            phase: "working",
            last_mono: mono,
            finished: false,
        })
    }
    pub fn counters(&self) -> Counters {
        self.ledger.totals(&self.scope.profile)
    }
    pub fn heartbeat(
        &mut self,
        db: &mut Connection,
        wall: i64,
        mono: u64,
    ) -> Result<(), StoreError> {
        if self.finished {
            return Err(StoreError::InvalidTimestamp);
        }
        let elapsed = mono
            .checked_sub(self.last_mono)
            .ok_or(StoreError::InvalidTimestamp)?;
        if !matches!(
            self.phase,
            "manual" | "paused" | "booking_handoff" | "unobserved" | "finished"
        ) && elapsed > 0
        {
            StatisticsWriter::new(db).add_active(&self.scope, elapsed as f64 / 1000.0, wall)?;
        }
        StatisticsWriter::new(db).heartbeat(&self.scope, wall)?;
        self.last_mono = mono;
        Ok(())
    }
    fn counter(
        &mut self,
        db: &mut Connection,
        kind: CounterKind,
        id: EvidenceId,
        mode: ControlMode,
        wall: i64,
        offices: Vec<String>,
    ) -> Result<(), StoreError> {
        let inserted = self
            .ledger
            .record(CounterObservation {
                key: EventKey {
                    scope: self.scope.clone(),
                    kind,
                    identity: id,
                },
                at: wall,
                offices,
                mode,
                automated: true,
            })
            .map_err(|_| StoreError::Corrupt)?;
        if inserted {
            let event = self.ledger.events().last().ok_or(StoreError::Corrupt)?;
            StatisticsWriter::new(db).counter(event)?;
        }
        Ok(())
    }
    pub fn page(
        &mut self,
        db: &mut Connection,
        observed: &Observation,
        offices: &[String],
        mode: ControlMode,
        wall: i64,
        mono: u64,
    ) -> Result<(), StoreError> {
        self.page_record(db,observed,offices,mode,(wall,mono),false)
    }
    pub fn preparation_page(&mut self,db:&mut Connection,observed:&Observation,mode:ControlMode,wall:i64,mono:u64)->Result<(),StoreError>{
        if !matches!(observed.kind,PageKind::Province|PageKind::Loading|PageKind::Unknown){return Err(StoreError::Corrupt);}
        self.page_record(db,observed,&[],mode,(wall,mono),true)
    }
    fn page_record(&mut self,db:&mut Connection,observed:&Observation,offices:&[String],mode:ControlMode,time:(i64,u64),preparing:bool)->Result<(),StoreError>{
        let (wall,mono)=time;
        self.heartbeat(db, wall, mono)?;
        self.ledger.observe_page(&self.scope, observed.kind);
        StatisticsWriter::new(db).page(&self.scope, observed, wall)?;
        let id = EvidenceId::Document(observed.document.clone());
        if !matches!(observed.kind, PageKind::Loading | PageKind::Unknown) {
            self.counter(
                db,
                CounterKind::ProcessedPage,
                id.clone(),
                mode,
                wall,
                vec![],
            )?;
        }
        let kind = match observed.kind {
            PageKind::Province if !preparing => Some(CounterKind::Pass),
            PageKind::Office if !offices.is_empty() => Some(CounterKind::Office),
            PageKind::Booking => Some(CounterKind::Booking),
            PageKind::ServerError => Some(CounterKind::ServerError),
            PageKind::RateLimited => Some(CounterKind::RateLimited),
            PageKind::Blocked => Some(CounterKind::Blocked),
            PageKind::Expired => Some(CounterKind::Expired),
            _ => None,
        };
        if let Some(kind) = kind {
            self.counter(db, kind, id, mode, wall, offices.to_vec())?;
        }
        Ok(())
    }
    pub fn operation(
        &mut self,
        db: &mut Connection,
        kind: CounterKind,
        id: OperationId,
        mode: ControlMode,
        wall: i64,
    ) -> Result<(), StoreError> {
        if self.finished || !matches!(kind, CounterKind::Check | CounterKind::AutomationAction) {
            return Err(StoreError::Corrupt);
        }
        self.counter(db, kind, EvidenceId::Operation(id), mode, wall, vec![])
    }
    pub fn set_phase(
        &mut self,
        db: &mut Connection,
        phase: &'static str,
        wall: i64,
        mono: u64,
    ) -> Result<(), StoreError> {
        if ![
            "working",
            "waiting",
            "recovery",
            "manual",
            "paused",
            "booking_handoff",
            "unobserved",
        ]
        .contains(&phase)
        {
            return Err(StoreError::Corrupt);
        }
        self.heartbeat(db, wall, mono)?;
        StatisticsWriter::new(db).phase(&self.scope, phase, wall)?;
        self.phase = phase;
        Ok(())
    }
    pub fn enrich_booking(
        &mut self,
        db: &mut Connection,
        document: &DocumentId,
        details: &Value,
    ) -> Result<bool, StoreError> {
        if !self.ledger.enrich_booking(&self.scope, document, details) {
            return Ok(false);
        }
        StatisticsWriter::new(db).enrich_booking(&self.scope, document, details)
    }
    pub fn finish(
        &mut self,
        db: &mut Connection,
        wall: i64,
        mono: u64,
        reason: &str,
    ) -> Result<(), StoreError> {
        self.heartbeat(db, wall, mono)?;
        StatisticsWriter::new(db).activity(
            &self.scope,
            ActivityKind::Finish,
            wall,
            None,
            "finished",
            &ActivityEvidence::default(),
        )?;
        StatisticsWriter::new(db).end_session(&self.scope, wall, reason)?;
        self.phase = "finished";
        self.finished = true;
        Ok(())
    }
}
