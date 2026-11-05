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
    r.control.save_appointment_budget = Mock()
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
    assert r.budget.target(r.protection) == 5

    asyncio.run(r.step())

    assert r.budget.target(r.protection) == 50
    assert r.budget.remaining(r.protection) == 47
    assert r.budget.windows == ()
    assert r.budget.restriction_until == restriction
    assert r.control.effective_appointment_protection == r.protection


def test_manual_daily_budget_command_applies_at_next_step_and_persists():
    from datetime import datetime, timezone, timedelta
    from unittest.mock import Mock
    from app.appointment_budget import AppointmentBudget
    from app.config import AppointmentProtection
    from app.desktop.worker import apply_command

    r = runner()
    r.protection = AppointmentProtection(daily_target=50)
    now = datetime.now(timezone.utc)
    from app.appointment_schedule import MADRID
    r.budget = AppointmentBudget(day=now.astimezone(MADRID).date(),
                                 sent=3, target_override=1,
                                 restriction_until=now + timedelta(minutes=30))
    restriction = r.budget.restriction_until
    r.control.save_appointment_budget = Mock()
    r.observe = AsyncMock(return_value=Observed('applicant', 'doc', 'sig', None))
    r.observe_appointment_budget = Mock()
    r.manual_signature = 'sig'
    assert apply_command(r.control, {'command': 'appointment_daily_target', 'target': 20})
    assert r.budget.target(r.protection) == 5
    asyncio.run(r.step())
    assert r.budget.remaining(r.protection) == 17
    assert r.budget.restriction_until == restriction
    r.control.save_appointment_budget.assert_called_once_with(r.budget)
    assert apply_command(r.control, {'command': 'appointment_daily_target', 'target': None})
    asyncio.run(r.step())
    assert r.budget.target(r.protection) == 5
    r.budget.manual_target = 20
    assert apply_command(r.control, {'command': 'profile_timings', 'timings': {},
                                     'protection': {'daily_target': 40, 'adaptive_target': False}})
    from app.config import Settings
    r.checker.settings = Settings()
    r.page.set_default_timeout = Mock()
    asyncio.run(r.step())
    assert r.budget.target(r.protection) == 40
    assert r.budget.manual_target is None
    for invalid in (True, 0, -1, 2**31, '20'):
        assert not apply_command(r.control, {'command': 'appointment_daily_target', 'target': invalid})


@pytest.mark.parametrize('manual_first, expected', [(True, 40), (False, 20)])
def test_queued_budget_changes_preserve_order_when_adaptation_is_disabled(manual_first, expected):
    from datetime import datetime
    from unittest.mock import Mock
    from app.appointment_budget import AppointmentBudget
    from app.appointment_schedule import MADRID
    from app.config import AppointmentProtection, Settings
    from app.desktop.worker import apply_command

    r = runner()
    r.checker.settings = Settings()
    r.page.set_default_timeout = Mock()
    r.protection = AppointmentProtection(daily_target=50)
    r.budget = AppointmentBudget(day=datetime.now(MADRID).date(), sent=3, target_override=10)
    r.control.save_appointment_budget = Mock()
    r.observe = AsyncMock(return_value=Observed('applicant', 'doc', 'sig', None))
    r.observe_appointment_budget = Mock()
    r.manual_signature = 'sig'
    manual = {'command': 'appointment_daily_target', 'target': 20}
    disable = {'command': 'profile_timings', 'timings': {},
               'protection': {'daily_target': 40, 'adaptive_target': False}}
    for command in ([manual, disable] if manual_first else [disable, manual]):
        assert apply_command(r.control, command)

    asyncio.run(r.step())

    assert r.budget.target(r.protection) == expected


def test_live_cod_pause_change_and_manual_attempt_preserve_other_timing():
    from datetime import datetime, timedelta
    from unittest.mock import Mock
    from app.appointment_budget import AppointmentBudget
    from app.appointment_schedule import MADRID
    from app.config import AppointmentProtection, Settings
    from app.desktop.worker import apply_command

    r = runner()
    r.checker.settings = Settings()
    r.page.set_default_timeout = Mock()
    r.protection = AppointmentProtection()
    now = datetime.now(MADRID)
    r.budget = AppointmentBudget()
    r.budget.submitted(now, r.protection)
    r.budget.cod_oper(now, r.protection)
    r.control.save_appointment_budget = Mock()
    r.observe = AsyncMock(return_value=Observed('applicant', 'doc', 'sig', None))
    r.observe_appointment_budget = Mock()
    r.manual_signature = 'sig'
    assert apply_command(r.control, {'command': 'profile_timings', 'timings': {},
                                    'protection': {'first_cod_pause_minutes': 10, 'minimum_pause_minutes': 15}})
    asyncio.run(r.step())
    assert r.budget.restriction_until == now + timedelta(minutes=10)
    assert apply_command(r.control, {'command': 'appointment_try_now'})
    asyncio.run(r.step())
    assert r.budget.next_submission(now, r.protection) == now + timedelta(minutes=15)
    assert r.control.pending_attempt_now is False
    assert 'Ручная ближайшая попытка' in r.last_action_detail


def test_try_now_waits_for_copied_document_budget():
    from datetime import datetime, timedelta
    from unittest.mock import Mock
    from app.appointment_budget import AppointmentBudget
    from app.appointment_schedule import MADRID
    from app.config import AppointmentProtection
    from app.desktop.worker import apply_command

    r = runner()
    r.protection = AppointmentProtection()
    now = datetime.now(MADRID)
    temporary = r.budget = AppointmentBudget(day=now.date())
    r.control.appointment_budget_is_temporary = True
    r.control.save_appointment_budget = Mock()
    r.observe = AsyncMock(return_value=Observed('applicant', 'doc', 'sig', None))
    r.observe_appointment_budget = Mock()
    r.manual_signature = 'sig'
    assert apply_command(r.control, {'command': 'appointment_try_now'})
    asyncio.run(r.step())
    assert r.control.pending_attempt_now
    assert not temporary.immediate_attempt

    document = r.budget = AppointmentBudget(day=now.date(), sent=3, first_cod_oper=now,
                                            last_submission=now)
    r.control.appointment_budget_is_temporary = False
    asyncio.run(r.step())
    assert not r.control.pending_attempt_now
    assert document.immediate_attempt
    assert document.sent == 3
    assert document.next_submission(now, r.protection) == now + timedelta(minutes=15)
    document.submitted(now + timedelta(minutes=15), r.protection)
    assert not document.immediate_attempt
    assert not temporary.immediate_attempt


@pytest.mark.parametrize('still_temporary', [False, True])
def test_pending_try_now_expires_at_madrid_midnight(monkeypatch, still_temporary):
    from datetime import datetime, timezone
    from unittest.mock import Mock
    import app.desktop.worker as worker
    import app.assist_flow as flow
    from app.appointment_budget import AppointmentBudget
    from app.appointment_schedule import MADRID
    from app.config import AppointmentProtection

    class Clock(datetime):
        current = datetime(2026, 11, 5, 22, 59, tzinfo=timezone.utc)

        @classmethod
        def now(cls, tz=None):
            return cls.current.astimezone(tz)

    monkeypatch.setattr(worker, 'datetime', Clock)
    monkeypatch.setattr(flow, 'datetime', Clock)
    r = runner()
    r.protection = AppointmentProtection()
    r.budget = AppointmentBudget(day=Clock.now(MADRID).date())
    r.control.appointment_budget_is_temporary = True
    r.control.save_appointment_budget = Mock()
    r.observe = AsyncMock(return_value=Observed('applicant', 'doc', 'sig', None))
    r.observe_appointment_budget = Mock()
    r.manual_signature = 'sig'
    assert worker.apply_command(r.control, {'command': 'appointment_try_now'})
    assert r.control.pending_attempt_now_day == Clock.now(MADRID).date()
    asyncio.run(r.step())
    assert r.control.pending_attempt_now
    Clock.current = datetime(2026, 11, 5, 23, 1, tzinfo=timezone.utc)
    r.control.appointment_budget_is_temporary = still_temporary
    asyncio.run(r.step())
    assert not r.control.pending_attempt_now
    assert not r.budget.immediate_attempt
    assert 'истёк' in r.last_action_detail
