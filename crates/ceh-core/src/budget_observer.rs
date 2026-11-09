//! Turns observed pages into budget changes during a search (Python 0.3.6
//! `AssistRunner.observe_appointment_budget` and the matching parts of `_requested`/`act`).
use crate::appointment_budget::{AppointmentBudget, AppointmentPolicy};
use crate::model::PageKind;
use chrono::{DateTime, Utc};

/// The facts of one observed page the budget cares about.
#[derive(Clone, Copy, Debug)]
pub struct ObservedPage {
    pub kind: PageKind,
    /// The site answered with `cod. oper` (a possible restriction).
    pub operation_code: bool,
}

/// Round state: what the current pass has seen since the last availability request.
#[derive(Clone, Debug, Default)]
pub struct BudgetObserver {
    round_saw_banner: bool,
    banner_visible: bool,
    awaiting_result: bool,
    round_saw_office: bool,
    used_hot_retry: bool,
    office_recorded: bool,
    hot_retry: bool,
}

fn is_result(kind: PageKind) -> bool {
    matches!(
        kind,
        PageKind::RequestNoSlots | PageKind::RequestNoSlotsRestart | PageKind::PostContactNoSlots | PageKind::Booking
    )
}

impl BudgetObserver {
    pub fn hot_retry(&self) -> bool { self.hot_retry }
    pub fn awaiting_result(&self) -> bool { self.awaiting_result }
    pub fn round_saw_banner(&self) -> bool { self.round_saw_banner }
    pub fn round_saw_office(&self) -> bool { self.round_saw_office }
    pub fn banner_visible(&self) -> bool { self.banner_visible }
    pub fn used_hot_retry(&self) -> bool { self.used_hot_retry }

    /// A new pass starts at the province page.
    pub fn on_province(&mut self) {
        self.round_saw_banner = false;
    }

    /// The availability request (POST `acCitar`) was sent: count it and wait for its result.
    pub fn request_sent(&mut self, budget: &mut AppointmentBudget, now: DateTime<Utc>, policy: &AppointmentPolicy) {
        budget.submitted(now, policy);
        self.awaiting_result = true;
        self.round_saw_office = false;
        self.used_hot_retry = self.hot_retry;
        self.hot_retry = false;
        self.office_recorded = false;
    }

    /// Offices offered while a restriction is possible are recorded once per request.
    pub fn offices_seen(&mut self, budget: &mut AppointmentBudget, now: DateTime<Utc>, names: Vec<String>) -> bool {
        if budget.restricted() && self.awaiting_result && !self.office_recorded {
            budget.office_during_restriction(now, names);
            self.office_recorded = true;
            return true;
        }
        false
    }

    /// Apply one observation. Returns true when the budget changed and should be saved.
    pub fn observe(
        &mut self,
        page: &ObservedPage,
        budget: &mut AppointmentBudget,
        now: DateTime<Utc>,
        policy: &AppointmentPolicy,
    ) -> bool {
        budget.refresh(now, policy);
        let mut changed = false;
        if page.kind == PageKind::ApplicantNoSlots {
            self.round_saw_banner = true;
            if !self.banner_visible {
                budget.banners = budget.banners.saturating_add(1);
                changed = true;
            }
            self.banner_visible = true;
        } else if !matches!(page.kind, PageKind::Loading | PageKind::Unknown) {
            self.banner_visible = false;
        }
        if !self.awaiting_result {
            return changed;
        }
        if page.kind == PageKind::Office && !self.round_saw_office {
            self.round_saw_office = true;
        }
        if !is_result(page.kind) {
            return changed;
        }
        self.awaiting_result = false;
        if self.office_recorded
            && let Some(office) = budget.episode_offices.last_mut()
        {
            office.result = Some(if page.operation_code {
                "cod_oper".to_owned()
            } else {
                serde_json::to_value(page.kind).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
            });
        }
        if page.operation_code {
            budget.cod_oper(now, policy);
        } else {
            if page.kind != PageKind::Booking {
                budget.no_slots = budget.no_slots.saturating_add(1);
            }
            budget.ordinary_result(now);
            self.hot_retry = policy.retry_after_office
                && self.round_saw_office
                && !self.used_hot_retry
                && page.kind != PageKind::Booking;
        }
        true
    }
}
