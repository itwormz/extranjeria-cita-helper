//! Per-profile daily appointment budget. The browser adapter supplies observations
//! and submission receipts; this state does not interact with the browser.
//! Behaviour follows Python 0.3.6 (`app/appointment_budget.py`).
use crate::appointment_schedule::{attempt_windows, earliest_submission};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Europe::Madrid;

/// Adaptive estimates never drop below this many attempts.
const ADAPTIVE_FLOOR: u32 = 5;
const MAX_MANUAL_TARGET: u32 = i32::MAX as u32;

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct AppointmentPolicy {
    pub confirm_new_round: bool,
    pub daily_target: u32,
    pub adaptive_target: bool,
    pub distribute_attempts: bool,
    pub retry_after_office: bool,
    pub minimum_pause_minutes: u32,
    pub first_cod_pause_minutes: u32,
    pub next_cod_pause_minutes: u32,
}

impl Default for AppointmentPolicy {
    fn default() -> Self {
        Self {
            confirm_new_round: true,
            daily_target: 5,
            adaptive_target: true,
            distribute_attempts: false,
            retry_after_office: true,
            minimum_pause_minutes: 15,
            first_cod_pause_minutes: 30,
            next_cod_pause_minutes: 60,
        }
    }
}

impl AppointmentPolicy {
    pub fn is_valid(&self) -> bool {
        [self.daily_target, self.minimum_pause_minutes, self.first_cod_pause_minutes, self.next_cod_pause_minutes]
            .into_iter().all(|value| value > 0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BudgetError {
    /// A manual daily target must be an integer from 1 to 2 147 483 647.
    InvalidTarget,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct RestrictionOffice {
    pub at: DateTime<Utc>,
    pub names: Vec<String>,
    pub result: Option<String>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct RestrictionEpisode {
    pub first: DateTime<Utc>,
    pub last: DateTime<Utc>,
    pub recovered: DateTime<Utc>,
    pub offices: Vec<RestrictionOffice>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitReason {
    Window,
    MinimumPause,
    CodOper,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetSource {
    Manual,
    Adaptive,
    Base,
}

/// Same keys and meaning as Python 0.3.6 `AppointmentBudget.summary()`.
#[derive(Clone, Debug, serde::Serialize)]
pub struct BudgetSummary {
    pub wait_reason: Option<WaitReason>,
    pub attempt_window: Option<(DateTime<Utc>, DateTime<Utc>)>,
    pub target_source: TargetSource,
    pub manual_target: Option<u32>,
    pub adaptive_enabled: bool,
    pub adaptive_target: Option<u32>,
    pub base_target: u32,
    pub target: u32,
    pub sent: u32,
    pub remaining: u32,
    pub disabled: bool,
    pub banners: u32,
    pub no_slots: u32,
    pub immediate_attempt: bool,
    pub cod_oper_count: u32,
    pub ready: DateTime<Utc>,
    pub restricted: bool,
    pub offices: u32,
    pub next_target: Option<u32>,
    pub episode_history: Vec<RestrictionEpisode>,
    pub episode_offices: Vec<RestrictionOffice>,
    pub first_cod_oper: Option<DateTime<Utc>>,
    pub last_cod_oper: Option<DateTime<Utc>>,
}

#[derive(Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct AppointmentBudget {
    day: Option<NaiveDate>,
    sent: u32,
    pub banners: u32,
    pub no_slots: u32,
    cod_oper_count: u32,
    pub offices_during_restriction: u32,
    pub episode_offices: Vec<RestrictionOffice>,
    pub episode_history: Vec<RestrictionEpisode>,
    pub distribution_disabled: bool,
    distribution_enabled_today: bool,
    immediate_attempt: bool,
    manual_target: Option<u32>,
    target_override: Option<u32>,
    next_target: Option<u32>,
    last_submission: Option<DateTime<Utc>>,
    first_cod_oper: Option<DateTime<Utc>>,
    last_cod_oper: Option<DateTime<Utc>>,
    restriction_until: Option<DateTime<Utc>>,
    windows: Vec<(DateTime<Utc>, DateTime<Utc>)>,
}

impl AppointmentBudget {
    /// Apply the Python 0.3.x load-time rules to a stored state: adaptive estimates are at least five
    /// attempts; a raised estimate invalidates the planned windows.
    pub fn normalized(mut self) -> Self {
        if self.target_override.is_some_and(|estimate| estimate < ADAPTIVE_FLOOR) {
            self.target_override = Some(ADAPTIVE_FLOOR);
            self.windows.clear();
        }
        if let Some(next) = self.next_target {
            self.next_target = Some(next.max(ADAPTIVE_FLOOR));
        }
        self
    }

    pub fn refresh(&mut self, now: DateTime<Utc>, policy: &AppointmentPolicy) {
        let day = now.with_timezone(&Madrid).date_naive();
        if self.day != Some(day) {
            self.day = Some(day);
            self.manual_target = None;
            self.sent = 0;
            self.banners = 0;
            self.no_slots = 0;
            self.cod_oper_count = 0;
            self.distribution_disabled = false;
            self.distribution_enabled_today = false;
            self.immediate_attempt = false;
            self.windows.clear();
            self.target_override = self.next_target.take();
        } else if self.windows.first().is_some_and(|(_, end)| now > *end) {
            self.windows = attempt_windows(now, self.remaining(policy)).unwrap_or_default();
        }
    }

    pub fn target(&self, policy: &AppointmentPolicy) -> u32 {
        if let Some(manual) = self.manual_target {
            return manual;
        }
        match self.target_override {
            Some(estimate) if policy.adaptive_target => estimate.max(ADAPTIVE_FLOOR),
            _ => policy.daily_target,
        }
    }

    pub fn remaining(&self, policy: &AppointmentPolicy) -> u32 {
        self.target(policy).saturating_sub(self.sent)
    }

    /// Set (or clear, with `None`) today's manual target. Expires at Madrid midnight.
    pub fn set_daily_target(
        &mut self,
        target: Option<u32>,
        now: DateTime<Utc>,
        policy: &AppointmentPolicy,
    ) -> Result<(), BudgetError> {
        if target.is_some_and(|value| value == 0 || value > MAX_MANUAL_TARGET) {
            return Err(BudgetError::InvalidTarget);
        }
        self.refresh(now, policy);
        self.manual_target = target;
        self.windows.clear();
        Ok(())
    }

    pub fn distributes(&self, policy: &AppointmentPolicy) -> bool {
        !self.distribution_disabled && (policy.distribute_attempts || self.distribution_enabled_today)
    }

    /// Today-only choice; overrides the profile's `distribute_attempts` until midnight.
    pub fn set_distribution(&mut self, disabled: bool, now: DateTime<Utc>, policy: &AppointmentPolicy) {
        self.refresh(now, policy);
        self.distribution_disabled = disabled;
        self.distribution_enabled_today = !disabled;
        self.windows.clear();
    }

    /// Allow one attempt now: skips the window and the `cod. oper` pause, keeps the minimum pause.
    pub fn try_now(&mut self, now: DateTime<Utc>, policy: &AppointmentPolicy) {
        self.refresh(now, policy);
        self.immediate_attempt = true;
    }

    /// Recompute an active `cod. oper` pause from its original start under the given policy.
    pub fn recalculate_restriction(&mut self, policy: &AppointmentPolicy) {
        let Some(first) = self.first_cod_oper else { return };
        let (base, minutes) = match self.last_submission {
            Some(last) if last > first => (last, policy.next_cod_pause_minutes),
            _ => (first, policy.first_cod_pause_minutes),
        };
        self.restriction_until = base.checked_add_signed(Duration::minutes(i64::from(minutes)));
    }

    /// A copy-mode profile gets its own budget file once the document is known. Today's explicit
    /// choices made before that (manual target, distribution) must survive the swap
    /// (Python `load_bound_budget`). Choices from another Madrid day are ignored.
    pub fn carry_today_choices_from(&mut self, previous: &AppointmentBudget, now: DateTime<Utc>, policy: &AppointmentPolicy) {
        self.refresh(now, policy);
        if previous.day != self.day {
            return;
        }
        if let Some(target) = previous.manual_target {
            // Already validated when it was first set.
            let _ = self.set_daily_target(Some(target), now, policy);
        }
        if previous.distribution_disabled || previous.distribution_enabled_today {
            self.set_distribution(previous.distribution_disabled, now, policy);
        }
    }

    /// Adjust state when the profile's policy changes during a search (Python 0.3.2+ `AssistRunner.step`).
    pub fn policy_changed(&mut self, old: &AppointmentPolicy, new: &AppointmentPolicy) {
        if old == new {
            return;
        }
        self.windows.clear();
        self.recalculate_restriction(new);
        if old.adaptive_target && !new.adaptive_target {
            self.manual_target = None;
        }
        if old.distribute_attempts != new.distribute_attempts {
            self.distribution_disabled = false;
            self.distribution_enabled_today = false;
        }
    }

    pub fn restriction_until(&self) -> Option<DateTime<Utc>> {
        self.restriction_until
    }

    /// Earliest time the next availability request may be sent; never before `now`.
    pub fn next_submission(&mut self, now: DateTime<Utc>, policy: &AppointmentPolicy, hot: bool) -> DateTime<Utc> {
        self.refresh(now, policy);
        self.recalculate_restriction(policy);
        let restriction = if self.immediate_attempt { None } else { self.restriction_until };
        let ready = earliest_submission(
            self.last_submission,
            restriction,
            Duration::minutes(i64::from(policy.minimum_pause_minutes)),
        )
        .map_or(now, |deadline| now.max(deadline));
        let distributes = self.distributes(policy);
        if !distributes {
            self.windows.clear();
        }
        let remaining = self.remaining(policy);
        if self.immediate_attempt
            || self.first_cod_oper.is_some()
            || self.sent == 0
            || !distributes
            || hot
            || remaining == 0
        {
            return ready;
        }
        if self.windows.is_empty() {
            self.windows = attempt_windows(now, remaining).unwrap_or_default();
        }
        self.windows
            .iter()
            .find(|(_, end)| ready <= *end)
            .map_or(ready, |(start, _)| ready.max(*start))
    }

    pub fn submitted(&mut self, now: DateTime<Utc>, policy: &AppointmentPolicy) {
        self.refresh(now, policy);
        self.sent = self.sent.saturating_add(1);
        self.immediate_attempt = false;
        self.last_submission = Some(now);
        self.windows = if self.distributes(policy) {
            attempt_windows(now, self.remaining(policy)).unwrap_or_default()
        } else {
            Vec::new()
        };
        if self.first_cod_oper.is_some() {
            self.restriction_until = now.checked_add_signed(Duration::minutes(i64::from(policy.next_cod_pause_minutes)));
        }
    }

    pub fn cod_oper(&mut self, now: DateTime<Utc>, policy: &AppointmentPolicy) {
        self.refresh(now, policy);
        self.cod_oper_count = self.cod_oper_count.saturating_add(1);
        self.immediate_attempt = false;
        if self.next_target.is_none() && self.sent > 0 {
            self.next_target = Some((self.sent - 1).max(ADAPTIVE_FLOOR));
        }
        if self.first_cod_oper.is_none() {
            self.offices_during_restriction = 0;
            self.episode_offices.clear();
            self.first_cod_oper = Some(now);
            self.restriction_until = now.checked_add_signed(Duration::minutes(i64::from(policy.first_cod_pause_minutes)));
        }
        self.last_cod_oper = Some(now);
    }

    pub fn sent(&self) -> u32 { self.sent }

    pub fn restricted(&self) -> bool { self.first_cod_oper.is_some() }

    pub fn office_during_restriction(&mut self, now: DateTime<Utc>, names: Vec<String>) {
        self.offices_during_restriction = self.offices_during_restriction.saturating_add(1);
        self.episode_offices.push(RestrictionOffice { at: now, names, result: None });
    }

    pub fn ordinary_result(&mut self, now: DateTime<Utc>) {
        if let Some(first) = self.first_cod_oper.take() {
            self.episode_history.push(RestrictionEpisode {
                first,
                last: self.last_cod_oper.take().unwrap_or(first),
                recovered: now,
                offices: std::mem::take(&mut self.episode_offices),
            });
            if self.episode_history.len() > 30 {
                self.episode_history.drain(..self.episode_history.len() - 30);
            }
        }
        self.last_cod_oper = None;
        self.restriction_until = None;
    }

    pub fn summary(&mut self, now: DateTime<Utc>, policy: &AppointmentPolicy, hot: bool) -> BudgetSummary {
        let ready = self.next_submission(now, policy, hot);
        let attempt_window = if self.sent > 0
            && !self.immediate_attempt
            && self.distributes(policy)
            && !hot
            && self.first_cod_oper.is_none()
            && self.remaining(policy) > 0
        {
            self.windows.iter().find(|(start, end)| *start <= ready && ready <= *end).copied()
        } else {
            None
        };
        let wait_reason = (ready > now).then(|| {
            let minimum = self
                .last_submission
                .and_then(|last| last.checked_add_signed(Duration::minutes(i64::from(policy.minimum_pause_minutes))));
            if !self.immediate_attempt && self.restriction_until.is_some_and(|until| until >= ready) {
                WaitReason::CodOper
            } else if minimum.is_some_and(|minimum| minimum >= ready) {
                WaitReason::MinimumPause
            } else {
                WaitReason::Window
            }
        });
        let target_source = if self.manual_target.is_some() {
            TargetSource::Manual
        } else if policy.adaptive_target && self.target_override.is_some() {
            TargetSource::Adaptive
        } else {
            TargetSource::Base
        };
        BudgetSummary {
            wait_reason,
            attempt_window,
            target_source,
            manual_target: self.manual_target,
            adaptive_enabled: policy.adaptive_target,
            adaptive_target: self.target_override.map(|estimate| estimate.max(ADAPTIVE_FLOOR)),
            base_target: policy.daily_target,
            target: self.target(policy),
            sent: self.sent,
            remaining: self.remaining(policy),
            disabled: !self.distributes(policy),
            banners: self.banners,
            no_slots: self.no_slots,
            immediate_attempt: self.immediate_attempt,
            cod_oper_count: self.cod_oper_count,
            ready,
            restricted: self.first_cod_oper.is_some(),
            offices: self.offices_during_restriction,
            next_target: self.next_target,
            episode_history: self.episode_history.clone(),
            episode_offices: self.episode_offices.clone(),
            first_cod_oper: self.first_cod_oper,
            last_cod_oper: self.last_cod_oper,
        }
    }
}
