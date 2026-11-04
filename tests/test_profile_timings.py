import asyncio
from unittest.mock import AsyncMock

import pytest

from app.desktop.profiles import ProfileStore, ProfileValidationError
from app.assist import reuse_policy_from_settings
from tests.test_desktop_recovery import runner
from app.assist_flow import Observed


def test_new_install_defaults_to_english_preserving_explicit_language(tmp_path, catalog_path):
    store = ProfileStore(tmp_path/'tasks.yml', catalog_path)
    assert store.preferences()['language'] == 'en'
    store.save_preferences(dark=False, diagnostics=True, sound=True, language='es')
    assert store.preferences()['language'] == 'es'


def test_profile_reload_and_recovery_timings_reach_runner(tmp_path, catalog_path):
    store = ProfileStore(tmp_path/'tasks.yml', catalog_path)
    store.path.write_text('tasks:\n- id: demo\n  timings:\n    reload_seconds: [40, 50]\n    rate_limit_seconds: 240\n    adaptive_reload_on_429: false\n    recovery_jitter_seconds: [1, 3]\n')
    settings = store.settings('demo')
    assert not settings.assist_timings.adaptive_reload_on_429
    policy = reuse_policy_from_settings(settings)
    assert all(40 <= policy.next_delay(recent_activity=False, empty_streak=1) <= 50 for _ in range(30))
    r = runner(); r.checker.settings = settings; r.page.goto = AsyncMock(); r.last_method = 'GET'
    asyncio.run(r.recover(Observed('rate_limited', 'doc', 'sig', None)))
    assert 241 <= r.countdown.await_args.args[1] <= 243


@pytest.mark.parametrize('timings', [
    'reload_seconds: [60, 30]', 'rate_limit_seconds: 0',
    'expired_seconds: []', 'typing_ms: [140, 70]',
    'recovery_jitter_seconds: [0, .nan]', 'navigation_seconds: true',
])
def test_invalid_profile_timing_rejected(tmp_path, catalog_path, timings):
    store = ProfileStore(tmp_path/'tasks.yml', catalog_path)
    store.path.write_text('tasks:\n- id: demo\n  timings:\n    '+timings+'\n')
    with pytest.raises(ProfileValidationError):
        store.settings('demo')


def test_unset_timings_keep_existing_recovery_defaults(tmp_path, catalog_path):
    store = ProfileStore(tmp_path/'tasks.yml', catalog_path)
    store.path.write_text('tasks:\n- id: demo\n')
    settings = store.settings('demo')
    assert settings.assist_timings.reload_seconds == (30, 60)
    assert settings.assist_timings.expired_seconds == (0, 30, 60, 120, 300)
    assert settings.assist_timings.recovery_jitter_seconds == (0, 0)
    assert settings.assist_timings.typing_ms == (70, 140)
    assert settings.assist_timings.adaptive_reload_on_429


def test_saved_protection_applies_next_step_without_resetting_budget():
    from datetime import datetime, timedelta, timezone
    from unittest.mock import Mock

    from app.appointment_budget import AppointmentBudget
    from app.config import AppointmentProtection, Settings
    from app.desktop.worker import apply_command

    r = runner()
    r.checker.settings = Settings()
    r.page.set_default_timeout = Mock()
    now = datetime.now(timezone.utc)
    r.budget = AppointmentBudget(sent=3, target_override=1,
                                 windows=((now, now + timedelta(hours=1)),),
                                 restriction_until=now + timedelta(minutes=30))
    restriction = r.budget.restriction_until
    r.protection = AppointmentProtection(daily_target=50)
    r.observe = AsyncMock(return_value=Observed('applicant', 'doc', 'sig', None))
    r.observe_appointment_budget = Mock()
    r.manual_signature = 'sig'

    assert apply_command(r.control, {'command': 'profile_timings', 'timings': {},
                                     'protection': {'daily_target': 50, 'adaptive_target': False}})
    assert r.budget.target(r.protection) == 1

    asyncio.run(r.step())

    assert r.budget.target(r.protection) == 50
    assert r.budget.remaining(r.protection) == 47
    assert r.budget.windows == ()
    assert r.budget.restriction_until == restriction
    assert r.control.effective_appointment_protection == r.protection
