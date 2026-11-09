//! Per-profile daily appointment budget. The browser adapter supplies observations
//! and submission receipts; this state does not interact with the browser.
use crate::appointment_schedule::{attempt_windows, earliest_submission};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Europe::Madrid;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct AppointmentPolicy {
    pub confirm_new_round: bool,
    pub daily_target: u32,
    pub adaptive_target: bool,
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

#[derive(Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct AppointmentBudget {
    day: Option<NaiveDate>,
    sent: u32,
    pub banners: u32,
    pub no_slots: u32,
    pub offices_during_restriction: u32,
    pub episode_offices: Vec<RestrictionOffice>,
    pub episode_history: Vec<RestrictionEpisode>,
    pub distribution_disabled: bool,
    target_override: Option<u32>,
    next_target: Option<u32>,
    last_submission: Option<DateTime<Utc>>,
    first_cod_oper: Option<DateTime<Utc>>,
    last_cod_oper: Option<DateTime<Utc>>,
    restriction_until: Option<DateTime<Utc>>,
    windows: Vec<(DateTime<Utc>, DateTime<Utc>)>,
}

impl AppointmentBudget {
    fn refresh(&mut self, now: DateTime<Utc>, policy: &AppointmentPolicy) {
        let day = now.with_timezone(&Madrid).date_naive();
        if self.day != Some(day) {
            self.day = Some(day);
            self.sent = 0;
            self.banners = 0;
            self.no_slots = 0;
            self.distribution_disabled = false;
            self.target_override = self.next_target.take();
            self.windows.clear();
        } else if self.windows.first().is_some_and(|(_, end)| now > *end) {
            self.windows = attempt_windows(now, self.remaining(policy)).unwrap_or_default();
        }
    }

    pub fn target(&self, policy: &AppointmentPolicy) -> u32 {
        if policy.adaptive_target {
            self.target_override.unwrap_or(policy.daily_target)
        } else {
            policy.daily_target
        }
    }

    pub fn remaining(&self, policy: &AppointmentPolicy) -> u32 {
        self.target(policy).saturating_sub(self.sent)
    }

    pub fn next_submission(
        &mut self,
        now: DateTime<Utc>,
        policy: &AppointmentPolicy,
        bypass_distribution: bool,
    ) -> DateTime<Utc> {
        self.refresh(now, policy);
        let ready = earliest_submission(
            self.last_submission,
            self.restriction_until,
            Duration::minutes(i64::from(policy.minimum_pause_minutes)),
        )
        .map_or(now, |deadline| now.max(deadline));
        let remaining = self.remaining(policy);
        if self.sent == 0 || self.distribution_disabled || bypass_distribution || remaining == 0 {
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
        self.last_submission = Some(now);
        self.windows = attempt_windows(now, self.remaining(policy)).unwrap_or_default();
        if self.first_cod_oper.is_some() {
            self.restriction_until = now.checked_add_signed(Duration::minutes(i64::from(policy.next_cod_pause_minutes)));
        }
    }

    pub fn cod_oper(&mut self, now: DateTime<Utc>, policy: &AppointmentPolicy) {
        self.refresh(now, policy);
        if self.next_target.is_none() && self.sent > 0 {
            self.next_target = Some(self.sent - 1);
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
        self.restriction_until = None;
    }
}
