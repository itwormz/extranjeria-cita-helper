"""Cooldown behavior, including profiles saved with the previous defaults."""
import asyncio
from types import SimpleNamespace
from unittest.mock import AsyncMock
import pytest
from app.assist_control import AssistControl
from app.assist_flow import AssistRunner,Observed
from app.config import Settings
from app.timings import parse_timings

@pytest.mark.parametrize('raw,attempt,delay',[
    ({},0,30),({},1,60),
    ({'blocked_seconds':[30,60]},0,30),
    ({'blocked_seconds':[300,360,420,540,600]},0,30),
    ({'blocked_seconds':[300]},0,300),
])
def test_block_recovery_uses_short_default_and_preserves_custom_schedule(raw,attempt,delay):
    timings=parse_timings(raw);control=AssistControl();control.blocked_streak=attempt
    runner=AssistRunner(SimpleNamespace(url="https://example.test/blocked"),SimpleNamespace(),SimpleNamespace(settings=Settings(assist_timings=timings)),SimpleNamespace(id='test'),'run',control,SimpleNamespace(stop=AsyncMock()),SimpleNamespace())
    runner.save=AsyncMock();runner.countdown=AsyncMock(return_value=False)
    asyncio.run(runner.recover(Observed('blocked','doc','sig',None)))
    assert runner.countdown.await_args.args[1]==delay
    assert control.blocked_streak==attempt

def test_429_keeps_its_own_three_minute_cooldown():
    assert parse_timings({'blocked_seconds':[30]}).rate_limit_seconds==180

def test_block_schedule_rejects_tighter_than_30_seconds():
    with pytest.raises(ValueError):parse_timings({'blocked_seconds':[29]})
