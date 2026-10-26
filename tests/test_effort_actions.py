"""Count confirmed semantic operations rather than keystrokes or failed attempts."""
import asyncio
from types import SimpleNamespace
from unittest.mock import AsyncMock
import pytest
from app.site import IcpPlusChecker
from app.config import Settings


def test_failed_activation_is_not_counted():
    async def run():
        checker=IcpPlusChecker(Settings());counts=[];checker._assist_action=lambda:counts.append(1)
        locator=SimpleNamespace(click=AsyncMock(side_effect=RuntimeError('not actionable')))
        with pytest.raises(RuntimeError):await checker._activate_control(locator)
        assert not counts
        locator.click=AsyncMock();await checker._activate_control(locator)
        assert counts==[1]
    asyncio.run(run())


def test_missing_option_is_not_counted():
    async def run():
        checker=IcpPlusChecker(Settings());counts=[];checker._assist_action=lambda:counts.append(1)
        locator=SimpleNamespace(is_visible=AsyncMock(return_value=True),is_enabled=AsyncMock(return_value=True))
        page=SimpleNamespace(locator=lambda selector:SimpleNamespace(first=locator),evaluate=AsyncMock(return_value=False))
        with pytest.raises(Exception):await checker._select_without_focus(page,'select',value='test')
        assert not counts
        page.evaluate=AsyncMock(return_value=True);await checker._select_without_focus(page,'select',value='test')
        assert counts==[1]
    asyncio.run(run())
