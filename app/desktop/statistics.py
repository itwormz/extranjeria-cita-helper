"""Persistent public per-profile lifetime statistics."""
import hashlib
import json
import math
import os
from datetime import datetime, timezone
from pathlib import Path
import tempfile
from ..locking import locked_file

COUNTERS = ('checks', 'offices_seen', 'server_errors', 'rate_limited', 'blocked',
            'booking_screens', 'expired_sessions', 'flow_passes')


def _empty():
    return dict.fromkeys(COUNTERS, 0) | dict(active_seconds=0, started_at=None,
        first_available_at=None, first_available_seconds=None, _offices=[], _observations=[])


class ProfileStatistics:
    def __init__(self, path):
        self.path = Path(path)

    def _read(self):
        if not self.path.exists():
            return {}
        raw = json.loads(self.path.read_text(encoding='utf-8'))
        if not isinstance(raw, dict):
            raise ValueError('Invalid profile statistics')
        for key, value in list(raw.items()):
            if not isinstance(key, str):
                raise ValueError('Invalid profile statistics')
            if type(value) is int and value >= 0:
                raw[key] = _empty() | {'checks': value}
            elif isinstance(value, dict):
                row = _empty() | value
                if any(type(row[k]) is not int or row[k] < 0 for k in COUNTERS):
                    raise ValueError('Invalid profile statistics')
                if type(row['active_seconds']) not in (int, float) or not math.isfinite(row['active_seconds']) or row['active_seconds'] < 0:
                    raise ValueError('Invalid profile statistics')
                if any(not isinstance(row[k], list) or any(not isinstance(v, str) for v in row[k]) for k in ('_offices', '_observations')):
                    raise ValueError('Invalid profile statistics')
                if not isinstance(row.get('_last_office', ''), str):
                    raise ValueError('Invalid profile statistics')
                for k in ('started_at', 'first_available_at'):
                    if row[k] is not None:
                        datetime.fromisoformat(row[k])
                raw[key] = row
            else:
                raise ValueError('Invalid profile statistics')
        return raw

    @staticmethod
    def _public(row):
        return {k: row[k] for k in (*COUNTERS, 'active_seconds', 'started_at',
            'first_available_at', 'first_available_seconds')} | {'unique_offices': len(row['_offices'])}

    def _transaction(self, profile_id, change=None):
        self.path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        with locked_file(self.path.with_suffix('.lock')):
            raw = self._read()
            row = raw.setdefault(profile_id, _empty())
            if change is not None:
                before = json.dumps(row, sort_keys=True)
                change(row)
                if json.dumps(row, sort_keys=True) == before:
                    return self._public(row)
                fd, temporary = tempfile.mkstemp(prefix='.statistics-', dir=self.path.parent)
                try:
                    with os.fdopen(fd, 'w', encoding='utf-8') as stream:
                        json.dump(raw, stream, ensure_ascii=False)
                        stream.flush()
                        os.fsync(stream.fileno())
                    os.replace(temporary, self.path)
                finally:
                    if os.path.exists(temporary):
                        os.unlink(temporary)
            return self._public(row)

    def get(self, profile_id):
        return self._transaction(profile_id)

    def known_offices(self, profile_id):
        """Public labels observed for this profile; never infer another profile."""
        self.path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        with locked_file(self.path.with_suffix('.lock')):
            return list(self._read().get(profile_id, _empty())['_offices'])

    def last_office(self, profile_id):
        self.path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        with locked_file(self.path.with_suffix('.lock')):
            return self._read().get(profile_id, {}).get('_last_office', '')

    def total(self, profile_id):
        return self.get(profile_id)['checks']

    def increment(self, profile_id):
        def change(row):
            row['checks'] += 1
        return self._transaction(profile_id, change)['checks']

    def observe(self, profile_id, kind, document, *, offices=(), at=None):
        timestamp = (at or datetime.now(timezone.utc))
        timestamp = timestamp.isoformat() if isinstance(timestamp, datetime) else str(timestamp)
        parsed_at = datetime.fromisoformat(timestamp)
        if parsed_at.tzinfo is None:
            parsed_at = parsed_at.replace(tzinfo=timezone.utc)
        timestamp = parsed_at.isoformat()
        labels = list(dict.fromkeys(str(label) for label in offices if label))
        key = hashlib.sha256(f'{document}:{kind}'.encode()).hexdigest()
        def change(row):
            if key in row['_observations']:
                return
            row['_observations'] = (row['_observations'] + [key])[-1024:]
            row['started_at'] = row['started_at'] or timestamp
            counter = {'office': 'offices_seen', 'booking': 'booking_screens',
                'pass': 'flow_passes', 'expired': 'expired_sessions', 'server_error': 'server_errors',
                'rate_limited': 'rate_limited', 'blocked': 'blocked'}.get(kind)
            if counter and (kind != 'office' or labels):
                row[counter] += 1
            if kind == 'office':
                row['_offices'] = sorted(set(row['_offices']) | set(labels))
                if labels:
                    row['_last_office'] = labels[-1]
            if (kind == 'booking' or (kind == 'office' and labels)) and row['first_available_at'] is None:
                row['first_available_at'] = timestamp
                start = datetime.fromisoformat(row['started_at'])
                if start.tzinfo is None:
                    start = start.replace(tzinfo=timezone.utc)
                available = datetime.fromisoformat(timestamp)
                row['first_available_seconds'] = max(0, (available - start).total_seconds())
        return self._transaction(profile_id, change)

    def add_active(self, profile_id, seconds):
        if type(seconds) not in (int, float) or not math.isfinite(seconds) or seconds < 0:
            raise ValueError('Invalid active duration')
        def change(row):
            row['started_at'] = row['started_at'] or datetime.now(timezone.utc).isoformat()
            row['active_seconds'] += seconds
        return self._transaction(profile_id, change)
