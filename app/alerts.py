"""Local attention alerts for the human-in-the-loop assist mode."""

from __future__ import annotations

import asyncio
import logging
import platform
import shutil
import time
from pathlib import Path

from .logging_utils import log_event

log = logging.getLogger(__name__)

REPEAT_SECONDS = 3.0


class Alerter:
    """Play an attention sound until stop() so an urgent action is not missed."""

    def __init__(self, sound_path: str, *, repeat_seconds: float = REPEAT_SECONDS):
        self.sound_path = sound_path
        self.repeat_seconds = repeat_seconds
        self._task: asyncio.Task | None = None

    async def start(self, reason: str, *, repeat: bool = True) -> None:
        await self.stop()
        log_event(log, logging.INFO, "assist_attention_started", reason=reason, repeat=repeat)
        self._task = asyncio.create_task(self._loop(reason, repeat))

    async def stop(self) -> None:
        task = self._task
        self._task = None
        if task is None:
            return
        task.cancel()
        try:
            await task
        except asyncio.CancelledError:
            pass
        log_event(log, logging.INFO, "assist_attention_stopped")

    async def _loop(self, reason: str, repeat: bool) -> None:
        while True:
            started = time.monotonic()
            await self._chime()
            if not repeat:
                return
            await asyncio.sleep(max(0.0, self.repeat_seconds - (time.monotonic() - started)))

    def sound_file(self):
        if self.sound_path and Path(self.sound_path).is_file():
            return Path(self.sound_path)
        return Path(__file__).parent / 'desktop' / 'assets' / 'attention.wav'

    async def _chime(self) -> None:
        sound = str(self.sound_file())
        if platform.system() == 'Windows':
            import winsound
            try:
                await asyncio.to_thread(winsound.PlaySound, sound, winsound.SND_FILENAME | winsound.SND_NODEFAULT)
            except (OSError, RuntimeError):
                log_event(log, logging.WARNING, 'assist_alert_sound_failed')
            return
        if platform.system() == "Darwin" and shutil.which("afplay"):
            command = ["afplay", sound]
        elif shutil.which("paplay"):
            command = ["paplay", sound]
        elif shutil.which('aplay'):
            command = ['aplay', '-q', sound]
        else:
            log_event(log, logging.WARNING, "assist_alert_sound_unavailable")
            return
        try:
            process = await asyncio.create_subprocess_exec(*command)
            await process.wait()
        except Exception:
            log_event(log, logging.WARNING, "assist_alert_sound_failed")
