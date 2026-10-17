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
    store.path.write_text('tasks:\n- id: demo\n  timings:\n    reload_seconds: [40, 50]\n    rate_limit_seconds: 240\n    recovery_jitter_seconds: [1, 3]\n')
    settings = store.settings('demo')
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
