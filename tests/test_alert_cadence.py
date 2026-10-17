import asyncio
from types import SimpleNamespace
from unittest.mock import AsyncMock

import pytest

from app.alerts import Alerter


def test_default_sound_is_bundled_wav():
    import wave
    alerter = Alerter('')
    with wave.open(str(alerter.sound_file()), 'rb') as audio:
        assert audio.getnframes() > 0


def test_linux_uses_bundled_sound_when_legacy_mac_path_is_missing(monkeypatch):
    from app import alerts
    commands = []
    monkeypatch.setattr(alerts.platform, 'system', lambda: 'Linux')
    monkeypatch.setattr(alerts.shutil, 'which', lambda cmd: '/usr/bin/aplay' if cmd == 'aplay' else None)
    async def launch(*args):
        commands.append(args)
        return SimpleNamespace(wait=AsyncMock(return_value=0))
    monkeypatch.setattr(alerts.asyncio, 'create_subprocess_exec', launch)
    asyncio.run(Alerter('/System/Library/Sounds/Missing.aiff')._chime())
    assert commands[0][0] == 'aplay'
    assert str(commands[0][-1]).endswith('attention.wav')


def test_repeated_chimes_start_every_three_seconds(monkeypatch):
    from app import alerts
    clock = [0.0]
    starts = []
    alerter = Alerter('test-sound')
    class Finished(Exception):
        pass
    async def chime():
        starts.append(clock[0])
        if len(starts) == 3:
            raise Finished()
        clock[0] += 0.5  # playback itself takes time
    async def sleep(seconds):
        clock[0] += seconds
    monkeypatch.setattr(alerter, '_chime', chime)
    monkeypatch.setattr(alerts, 'time', SimpleNamespace(monotonic=lambda: clock[0]), raising=False)
    monkeypatch.setattr(alerts.asyncio, 'sleep', sleep)
    try:
        asyncio.run(alerter._loop('office', repeat=True))
    except Finished:
        pass
    assert starts == [0.0, 3.0, 6.0]
