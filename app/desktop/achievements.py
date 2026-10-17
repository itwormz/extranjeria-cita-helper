"""Private, atomic achievement storage with immutable statistics snapshots."""
import copy
import json
import math
import os
import tempfile
import uuid
from datetime import datetime, timezone
from pathlib import Path
from ..locking import locked_file

COUNT_KEYS = ('checks', 'offices_seen', 'unique_offices', 'server_errors', 'rate_limited', 'blocked', 'booking_screens', 'flow_passes')
STAT_KEYS = COUNT_KEYS + ('active_seconds', 'started_at', 'first_available_at', 'first_available_seconds')
APPOINTMENT_KEYS = ('date', 'time', 'office', 'address', 'confirmation', 'timezone')


def snapshot(statistics):
    result = {}
    for key in COUNT_KEYS + ('active_seconds', 'first_available_seconds'):
        value = statistics.get(key)
        if key == 'first_available_seconds' and value is None:
            result[key] = None
        else:
            result[key] = value if type(value) in (int, float) and math.isfinite(value) and value >= 0 else 0
    for key in ('started_at', 'first_available_at'):
        value = statistics.get(key)
        try:
            result[key] = datetime.fromisoformat(value).isoformat() if isinstance(value, str) else None
        except ValueError:
            result[key] = None
    return result


def appointment_data(appointment):
    return {key: str(appointment.get(key, '') or '') for key in APPOINTMENT_KEYS}


class AchievementStore:
    def __init__(self, path):
        self.path = Path(path)

    def _read(self):
        if not self.path.exists():
            return {}
        raw = json.loads(self.path.read_text(encoding='utf-8'))
        if not isinstance(raw, dict) or any(not isinstance(v, list) for v in raw.values()):
            raise ValueError('Invalid achievements')
        return raw

    def _write(self, raw):
        fd, temporary = tempfile.mkstemp(prefix='.achievements-', dir=self.path.parent)
        try:
            with os.fdopen(fd, 'w', encoding='utf-8') as stream:
                json.dump(raw, stream, ensure_ascii=False)
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(temporary, self.path)
        finally:
            if os.path.exists(temporary):
                os.unlink(temporary)

    def list(self, profile_id):
        self.path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        with locked_file(self.path.with_suffix('.lock')):
            return copy.deepcopy(self._read().get(profile_id, []))

    def record(self, profile_id, statistics, appointment):
        self.path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        with locked_file(self.path.with_suffix('.lock')):
            raw = self._read()
            item = {'id': str(uuid.uuid4()), 'recorded_at': datetime.now(timezone.utc).isoformat(),
                    'statistics': snapshot(statistics), 'appointment': appointment_data(appointment)}
            raw.setdefault(profile_id, []).append(item)
            self._write(raw)
            return copy.deepcopy(item)

    def update_appointment(self, profile_id, achievement_id, appointment):
        self.path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        with locked_file(self.path.with_suffix('.lock')):
            raw = self._read()
            for item in raw.get(profile_id, []):
                if item['id'] == achievement_id:
                    item['appointment'] = appointment_data(appointment)
                    self._write(raw)
                    return copy.deepcopy(item)
            raise KeyError(achievement_id)
