"""Schedule appointment submissions in Madrid civil time."""
from datetime import datetime, time, timedelta
from zoneinfo import ZoneInfo

MADRID = ZoneInfo("Europe/Madrid")
WINDOW_RADIUS = timedelta(minutes=30)


def attempt_windows(now: datetime, remaining: int) -> tuple[tuple[datetime, datetime], ...]:
    """Spread remaining attempts over the current search window.

    Recompute on submission and when an unused window expires. Pass the time
    of that event as now, rather than recomputing on each browser poll.
    """
    if remaining <= 0:
        return ()

    local = now.astimezone(MADRID)
    morning = datetime.combine(local.date(), time(9), MADRID)
    evening = datetime.combine(local.date(), time(18), MADRID)
    midnight = datetime.combine(local.date() + timedelta(days=1), time(), MADRID)
    start = max(local, morning)
    end = evening if local < evening else midnight
    step = (end - start) / remaining
    windows = []
    # Only near-term windows are needed: the schedule is rebuilt after a
    # submission or expiry of its first window. Bound memory for large targets.
    for index in range(1, min(remaining, 24 * 60) + 1):
        center = start + step * index
        windows.append((max(morning, center - WINDOW_RADIUS), min(midnight, center + WINDOW_RADIUS)))
    return tuple(windows)


def earliest_submission(
    last_submission: datetime | None,
    restriction_until: datetime | None,
    minimum_pause: timedelta,
) -> datetime | None:
    """Combine the ordinary submission pause with the active cod oper pause."""
    if last_submission is None:
        return restriction_until
    ordinary = last_submission.astimezone(MADRID) + minimum_pause
    return max(ordinary, restriction_until) if restriction_until is not None else ordinary
