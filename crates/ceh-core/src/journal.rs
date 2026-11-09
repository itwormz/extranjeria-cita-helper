//! Bounded human page visits. No diagnostic payload, URL, applicant data or I/O.
use crate::{
    booking::{PublicBookingDetails, normalize_booking_details},
    model::*,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalStatus {
    Processing,
    Passed,
    Checked,
    Waiting,
    LoginRequired,
    ChoiceRequired,
    OfficesFound,
    DateAvailable,
    Recovering,
    ReviewRequired,
    Failed,
    NewPass,
    Interrupted,
    Unconfirmed,
    Paused,
    Cancelled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalEvent {
    Visit,
    Start,
    Finish,
    Pause,
    Resume,
    FlowStart,
    FlowRestart,
    Handoff,
    StatisticsError,
    BrowserShown,
    BrowserShowFailed,
    Action,
    ActionCompleted,
    Check,
    OfficeSubmitted,
    Recovery,
    InteractionRetry,
    ActionInterrupted,
    ManualRequired,
    RecoveryButton,
    FatalError,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalMode {
    Working,
    Waiting,
    Manual,
    Recovery,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalError {
    SequenceExhausted,
    InvalidDuration,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JournalRow {
    pub id: u64,
    pub time: i64,
    pub ended_at: Option<i64>,
    pub event: JournalEvent,
    pub stage: Option<PageKind>,
    pub document: Option<DocumentId>,
    pub status: JournalStatus,
    pub destination: Option<PageKind>,
    pub submitted: bool,
    pub checked: bool,
    pub offices: Vec<String>,
    pub selected_office: String,
    pub applicant_continue: bool,
    pub booking_details: PublicBookingDetails,
    pub deadline: Option<i64>,
    pub pass_number: u64,
    #[serde(skip)]
    resume_status: Option<JournalStatus>,
}
fn error(kind: PageKind) -> bool {
    matches!(
        kind,
        PageKind::Expired | PageKind::ServerError | PageKind::Blocked | PageKind::RateLimited
    )
}
fn no_slots(kind: PageKind) -> bool {
    matches!(
        kind,
        PageKind::RequestNoSlots | PageKind::RequestNoSlotsRestart | PageKind::PostContactNoSlots
    )
}
fn expected(from: PageKind, to: PageKind) -> bool {
    use PageKind::*;
    match from {
        Province => to == Procedure,
        Procedure => matches!(to, Auth | AuthWait | Applicant | ApplicantNoSlots),
        Auth => matches!(to, AuthWait | Applicant | ApplicantNoSlots),
        AuthWait => matches!(to, Applicant | ApplicantNoSlots | RequestMenu),
        ApplicantNoSlots => matches!(to, Applicant | ApplicantNoSlots),
        Applicant => matches!(to, RequestMenu | Office | Contact | Booking) || no_slots(to),
        RequestMenu | Office => {
            matches!(to, Office | Contact | Booking) && (from != Office || to != Office)
                || no_slots(to)
        }
        Contact => to == Booking || no_slots(to),
        RequestNoSlots => {
            matches!(
                to,
                RequestMenu | Province | Applicant | ApplicantNoSlots | Office | Contact | Booking
            ) || no_slots(to)
        }
        RequestNoSlotsRestart | PostContactNoSlots => to == Province,
        Captcha => matches!(to, Booking | Office | Contact | Applicant | RequestMenu),
        Booking => matches!(to, Contact | Booking),
        _ => false,
    }
}
#[derive(Default)]
pub struct PageJournal {
    rows: Vec<JournalRow>,
    active: Option<u64>,
    sequence: u64,
    pass_number: u64,
    restart_pending: bool,
}
impl PageJournal {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn rows(&self) -> &[JournalRow] {
        &self.rows
    }
    fn active(&self) -> Option<&JournalRow> {
        self.rows.iter().find(|r| Some(r.id) == self.active)
    }
    fn active_mut(&mut self) -> Option<&mut JournalRow> {
        self.rows.iter_mut().find(|r| Some(r.id) == self.active)
    }
    fn add(
        &mut self,
        event: JournalEvent,
        status: JournalStatus,
        document: Option<DocumentId>,
        stage: Option<PageKind>,
        at: i64,
    ) -> Result<u64, JournalError> {
        let id = self
            .sequence
            .checked_add(1)
            .ok_or(JournalError::SequenceExhausted)?;
        self.sequence = id;
        self.rows.push(JournalRow {
            id,
            time: at,
            ended_at: if event == JournalEvent::Visit {
                None
            } else {
                Some(at)
            },
            event,
            stage,
            document,
            status,
            destination: None,
            submitted: false,
            checked: false,
            offices: vec![],
            selected_office: String::new(),
            applicant_continue: false,
            booking_details: PublicBookingDetails::default(),
            deadline: None,
            pass_number: self.pass_number,
            resume_status: None,
        });
        if self.rows.len() > 200 {
            let keep_active =
                self.rows[0].id == self.active.unwrap_or(0) && self.rows[0].ended_at.is_none();
            let removed = self.rows.remove(usize::from(keep_active));
            if Some(removed.id) == self.active {
                self.active = None;
            }
        }
        Ok(id)
    }
    fn complete(&mut self, destination: PageKind, at: i64) {
        let Some(row) = self.active_mut() else { return };
        let Some(stage) = row.stage else { return };
        if row.status != JournalStatus::Failed {
            row.status = if error(stage) && destination == stage {
                JournalStatus::Waiting
            } else if error(destination) {
                JournalStatus::Interrupted
            } else if destination == PageKind::Unknown {
                JournalStatus::Unconfirmed
            } else if error(stage) {
                JournalStatus::Passed
            } else if no_slots(stage) && destination == stage && row.checked {
                JournalStatus::Checked
            } else if expected(stage, destination)
                || (stage == PageKind::ApplicantNoSlots
                    && row.applicant_continue
                    && expected(PageKind::Applicant, destination))
            {
                if stage == PageKind::Office && !row.offices.is_empty() {
                    JournalStatus::OfficesFound
                } else {
                    JournalStatus::Passed
                }
            } else if stage == PageKind::Booking {
                JournalStatus::DateAvailable
            } else {
                JournalStatus::Unconfirmed
            };
            row.destination = Some(destination);
            row.deadline = None;
        }
        row.ended_at = Some(at);
    }
    pub fn observe(
        &mut self,
        document: DocumentId,
        stage: PageKind,
        applicant_continue: bool,
        at: i64,
    ) -> Result<(), JournalError> {
        if stage == PageKind::Loading {
            return Ok(());
        }
        if let Some(old) = self.active() {
            if old.ended_at.is_none()
                && old.stage == Some(stage)
                && (old.document.as_ref() == Some(&document) || stage == PageKind::AuthWait)
            {
                let row = self.active_mut().expect("active visit");
                row.document = Some(document);
                row.applicant_continue = applicant_continue;
                return Ok(());
            }
            if old.ended_at.is_none() {
                self.complete(stage, at);
            }
        }
        let status = if error(stage) {
            JournalStatus::Recovering
        } else if stage == PageKind::Unknown {
            JournalStatus::ReviewRequired
        } else if stage == PageKind::Booking {
            JournalStatus::DateAvailable
        } else {
            JournalStatus::Processing
        };
        let id = self.add(JournalEvent::Visit, status, Some(document), Some(stage), at)?;
        self.active = Some(id);
        self.active_mut().expect("new visit").applicant_continue = applicant_continue;
        if stage != PageKind::Province {
            self.restart_pending = false;
        }
        Ok(())
    }
    pub fn phase(
        &mut self,
        mode: JournalMode,
        remaining: Option<f64>,
        at: i64,
    ) -> Result<(), JournalError> {
        let Some(row) = self.active_mut() else {
            return Ok(());
        };
        if row.ended_at.is_some()
            || matches!(row.status, JournalStatus::Failed | JournalStatus::Paused)
            || (mode == JournalMode::Manual && row.status == JournalStatus::ReviewRequired)
        {
            return Ok(());
        }
        match mode {
            JournalMode::Waiting if remaining.is_some() => {
                if row.status != JournalStatus::Waiting || row.deadline.is_none() {
                    let seconds = remaining.expect("remaining provided");
                    if !seconds.is_finite()
                        || seconds < 0.0
                        || (seconds * 1000.0).ceil() >= i64::MAX as f64
                    {
                        return Err(JournalError::InvalidDuration);
                    }
                    let deadline = at
                        .checked_add((seconds * 1000.0).ceil() as i64)
                        .ok_or(JournalError::InvalidDuration)?;
                    row.status = JournalStatus::Waiting;
                    row.deadline = Some(deadline);
                }
            }
            JournalMode::Manual => {
                row.status = match row.stage {
                    Some(PageKind::Auth | PageKind::AuthWait) => JournalStatus::LoginRequired,
                    Some(PageKind::Office) if row.offices.len() > 1 => {
                        JournalStatus::ChoiceRequired
                    }
                    Some(PageKind::Booking) => JournalStatus::DateAvailable,
                    _ => JournalStatus::ReviewRequired,
                }
            }
            JournalMode::Recovery => row.status = JournalStatus::Recovering,
            _ => {}
        }
        Ok(())
    }
    pub fn offices(&mut self, labels: Vec<String>, selected: Option<String>) {
        if let Some(row) = self.active_mut()
            && row.stage == Some(PageKind::Office)
            && row.ended_at.is_none()
        {
            row.offices = labels;
            if let Some(selected) = selected.filter(|s| !s.is_empty()) {
                row.selected_office = selected;
            }
            if row.offices.len() == 1 {
                row.status = JournalStatus::OfficesFound;
            }
        }
    }
    pub fn booking(&mut self, document: &DocumentId, raw: &Value) -> bool {
        let details = normalize_booking_details(raw);
        if details.office.is_none() && details.offers.is_empty() {
            return false;
        }
        if let Some(row) = self.active_mut()
            && row.stage == Some(PageKind::Booking)
            && row.document.as_ref() == Some(document)
        {
            row.booking_details = details;
            return true;
        }
        false
    }
    pub fn event(
        &mut self,
        event: JournalEvent,
        document: Option<DocumentId>,
        stage: Option<PageKind>,
        user_requested: bool,
        at: i64,
    ) -> Result<(), JournalError> {
        use JournalEvent::*;
        let active_open = self.active().is_some_and(|r| r.ended_at.is_none());
        if event == FatalError && !active_open {
            self.add(event, JournalStatus::Failed, None, None, at)?;
            return Ok(());
        }
        if event == Start && self.sequence > 0 {
            return Ok(());
        }
        if matches!(
            event,
            Start
                | Finish
                | Pause
                | Resume
                | FlowStart
                | FlowRestart
                | Handoff
                | StatisticsError
                | BrowserShown
                | BrowserShowFailed
        ) {
            if matches!(event, FlowStart | FlowRestart) && self.restart_pending {
                if stage == Some(PageKind::Province) {
                    self.restart_pending = false;
                }
                return Ok(());
            }
            if event == FlowRestart
                && stage == Some(PageKind::Province)
                && active_open
                && self.active().and_then(|r| r.document.as_ref()) != document.as_ref()
            {
                self.complete(PageKind::Province, at);
            }
            if matches!(event, FlowStart | FlowRestart) {
                self.pass_number = self
                    .pass_number
                    .checked_add(1)
                    .ok_or(JournalError::SequenceExhausted)?;
                self.restart_pending = true;
            }
            if let Some(row) = self.active_mut() {
                if event == Pause && row.ended_at.is_none() {
                    row.resume_status = Some(row.status);
                    row.status = JournalStatus::Paused;
                }
                if event == Resume && row.status == JournalStatus::Paused {
                    row.status = row.resume_status.unwrap_or(JournalStatus::Processing);
                    row.deadline = None;
                }
                let preserve = event == FlowRestart
                    && stage == Some(PageKind::Province)
                    && row.document.as_ref() == document.as_ref();
                if matches!(event, Finish | FlowRestart) && row.ended_at.is_none() && !preserve {
                    if row.stage == Some(PageKind::Booking) {
                        row.status = JournalStatus::DateAvailable;
                    } else if row.status != JournalStatus::Failed {
                        row.status = if event == Finish && user_requested {
                            JournalStatus::Cancelled
                        } else {
                            JournalStatus::Interrupted
                        };
                    }
                    row.ended_at = Some(at);
                    row.deadline = None;
                }
            }
            let status = match event {
                StatisticsError => JournalStatus::Failed,
                FlowRestart => JournalStatus::NewPass,
                Pause => JournalStatus::Paused,
                Start | Resume | FlowStart => JournalStatus::Processing,
                Handoff | BrowserShowFailed => JournalStatus::ReviewRequired,
                _ => JournalStatus::Passed,
            };
            self.add(event, status, None, None, at)?;
            return Ok(());
        }
        let Some(row) = self.active_mut() else {
            return Ok(());
        };
        if row.ended_at.is_some()
            || document
                .as_ref()
                .is_some_and(|d| row.document.as_ref() != Some(d))
            || stage.is_some_and(|s| row.stage != Some(s))
        {
            return Ok(());
        }
        match event {
            Action | ActionCompleted | Check | OfficeSubmitted => {
                row.status = JournalStatus::Processing;
                row.submitted |= event != Action;
                row.checked |= event == Check;
                row.deadline = None;
            }
            Recovery | InteractionRetry | ActionInterrupted | RecoveryButton => {
                row.status = JournalStatus::Recovering;
                row.deadline = None;
            }
            ManualRequired => row.status = JournalStatus::ReviewRequired,
            FatalError => {
                row.status = JournalStatus::Failed;
                row.ended_at = Some(at);
                row.deadline = None;
            }
            _ => {}
        }
        Ok(())
    }
}
