from types import SimpleNamespace
from unittest.mock import AsyncMock, Mock, MagicMock

import pytest


@pytest.mark.parametrize('failure', ['navigation', 'visible_control', 'observation'])
def test_office_fixture_setup_failure_saves_trace_and_closes_browser(monkeypatch, failure):
    from playwright import async_api
    from tests import test_local_assist_browser as fixture

    error = RuntimeError('synthetic fixture setup failure')
    visible = AsyncMock(side_effect=error if failure == 'visible_control' else None)
    page = SimpleNamespace(
        set_default_timeout=Mock(), set_default_navigation_timeout=Mock(),
        route=AsyncMock(), goto=AsyncMock(side_effect=error if failure == 'navigation' else None),
        locator=Mock(return_value=SimpleNamespace(wait_for=visible)))
    context = SimpleNamespace(new_page=AsyncMock(return_value=page))
    browser = SimpleNamespace(new_context=AsyncMock(return_value=context), close=AsyncMock())
    manager = MagicMock()
    manager.__aenter__ = AsyncMock(return_value=SimpleNamespace())
    manager.__aexit__ = AsyncMock(return_value=False)
    monkeypatch.setattr(async_api, 'async_playwright', lambda: manager)
    monkeypatch.setattr(fixture, 'launch_browser', AsyncMock(return_value=browser))
    monkeypatch.setattr(fixture, 'start_failure_trace', AsyncMock(return_value=True))
    save = AsyncMock()
    monkeypatch.setattr(fixture, 'save_failure_trace', save)
    monkeypatch.setattr(fixture, 'runner_for', lambda *_: SimpleNamespace(
        observe=AsyncMock(side_effect=error if failure == 'observation' else None)))

    with pytest.raises(RuntimeError) as raised:
        fixture.test_manual_office_navigation_during_capture_does_not_wait_for_old_select(False)

    assert raised.value is error
    save.assert_awaited_once_with(context)
    browser.close.assert_awaited_once()
