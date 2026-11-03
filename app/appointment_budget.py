"""Daily attempt state shared by the scheduler and appointment statistics."""
from dataclasses import dataclass, field
from datetime import date, datetime, timedelta

from .appointment_schedule import MADRID, attempt_windows, earliest_submission
from .config import AppointmentProtection


@dataclass
class AppointmentBudget:
    day: date | None = None
    sent: int = 0
    banners: int = 0
    no_slots: int = 0
    offices_during_restriction: int = 0
    episode_offices: list[dict] = field(default_factory=list)
    episode_history: list[dict] = field(default_factory=list)
    target_override: int | None = None
    next_target: int | None = None
    last_submission: datetime | None = None
    first_cod_oper: datetime | None = None
    last_cod_oper: datetime | None = None
    restriction_until: datetime | None = None
    distribution_disabled: bool = False
    windows: tuple[tuple[datetime, datetime], ...] = ()

    def refresh(self, now: datetime, policy: AppointmentProtection) -> None:
        day = now.astimezone(MADRID).date()
        if self.day != day:
            self.day = day
            self.sent = 0
            self.banners = 0
            self.no_slots = 0
            self.distribution_disabled = False
            self.windows = ()
            if self.next_target is not None:
                self.target_override = self.next_target
                self.next_target = None
            else:
                self.target_override = None
        elif self.windows and now > self.windows[0][1]:
            self.windows = attempt_windows(now, self.remaining(policy))

    def target(self, policy: AppointmentProtection) -> int:
        if policy.adaptive_target and self.target_override is not None:
            return self.target_override
        return policy.daily_target

    def remaining(self, policy: AppointmentProtection) -> int:
        return max(0, self.target(policy) - self.sent)

    def next_submission(
        self, now: datetime, policy: AppointmentProtection, *, hot: bool = False,
    ) -> datetime:
        self.refresh(now, policy)
        earliest = earliest_submission(
            self.last_submission, self.restriction_until,
            timedelta(minutes=policy.minimum_pause_minutes),
        )
        ready = max(now, earliest) if earliest is not None else now
        if self.sent == 0 or self.distribution_disabled or hot or not self.remaining(policy):
            return ready
        if not self.windows:
            self.windows = attempt_windows(now, self.remaining(policy))
        for start, end in self.windows:
            if ready <= end:
                return max(start, ready)
        # A cooldown may cover every planned window. Replan when it ends.
        return ready

    def submitted(self, now: datetime, policy: AppointmentProtection) -> None:
        self.refresh(now, policy)
        self.sent += 1
        self.last_submission = now
        self.windows = attempt_windows(now, self.remaining(policy))
        if self.first_cod_oper is not None:
            self.restriction_until = now + timedelta(minutes=policy.next_cod_pause_minutes)

    def cod_oper(self, now: datetime, policy: AppointmentProtection) -> None:
        self.refresh(now, policy)
        if self.next_target is None and self.sent:
            self.next_target = self.sent - 1
        if self.first_cod_oper is None:
            self.offices_during_restriction = 0
            self.episode_offices = []
            self.first_cod_oper = now
            self.restriction_until = now + timedelta(minutes=policy.first_cod_pause_minutes)
        self.last_cod_oper = now

    def office_during_restriction(self, now: datetime, names: list[str]) -> None:
        self.offices_during_restriction += 1
        self.episode_offices.append({'at': now.isoformat(), 'names': names, 'result': None})

    def ordinary_result(self, now: datetime) -> None:
        if self.first_cod_oper is not None:
            self.episode_history.append({
                'first': self.first_cod_oper.isoformat(),
                'last': self.last_cod_oper.isoformat(),
                'recovered': now.isoformat(),
                'offices': self.episode_offices,
            })
            self.episode_history = self.episode_history[-30:]
            self.episode_offices = []
        self.first_cod_oper = None
        self.last_cod_oper = None
        self.restriction_until = None


    def summary(self, now: datetime, policy: AppointmentProtection) -> dict:
        ready = self.next_submission(now, policy)
        return dict(
            target=self.target(policy), sent=self.sent, remaining=self.remaining(policy),
            disabled=self.distribution_disabled, banners=self.banners, no_slots=self.no_slots,
            ready=ready.isoformat(), restricted=self.first_cod_oper is not None,
            offices=self.offices_during_restriction, next_target=self.next_target,
            episode_history=self.episode_history, episode_offices=self.episode_offices,
            first_cod_oper=self.first_cod_oper.isoformat() if self.first_cod_oper else None,
            last_cod_oper=self.last_cod_oper.isoformat() if self.last_cod_oper else None,
        )
