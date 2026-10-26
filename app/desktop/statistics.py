"""Transactional, local statistics. Only typed public evidence is persisted."""
import hashlib
import json
import math
import os
import re
import sqlite3
from functools import lru_cache
from contextlib import contextmanager
from datetime import datetime, timezone, timedelta
from pathlib import Path
from zoneinfo import ZoneInfo

COUNTERS = ('checks', 'offices_seen', 'server_errors', 'rate_limited', 'blocked',
            'booking_screens', 'expired_sessions', 'flow_passes')
COUNTER_KIND = {'check': 'checks', 'office': 'offices_seen', 'booking': 'booking_screens',
                'pass': 'flow_passes', 'expired': 'expired_sessions',
                'server_error': 'server_errors', 'rate_limited': 'rate_limited', 'blocked': 'blocked'}
MADRID = ZoneInfo('Europe/Madrid')
STAGES = {'province','procedure','auth','auth_wait','applicant','applicant_no_slots','request_menu',
    'request_no_slots','request_no_slots_restart','post_contact_no_slots','office',
    'contact','booking','captcha','expired','server_error','blocked','rate_limited','loading','unknown'}
MODES = {'working','waiting','recovery','manual','paused','finished','booking_handoff','unobserved','unknown'}
EVENTS = set(COUNTER_KIND) | STAGES | {'start','finish','action','action_completed','action_interrupted',
    'browser_session_started','interaction_retry','flow_start','flow_restart','wait','recovery','recovery_button',
    'manual_required','office_selected','office_submitted','handoff','fatal_error',
    'pause','resume','http_response','network_error','timeout'}


def _empty():
    return dict.fromkeys(COUNTERS, 0) | dict(active_seconds=0, started_at=None,
        first_available_at=None, first_available_seconds=None, _offices=[], _observations=[])

def _time(value=None):
    value = value or datetime.now(timezone.utc)
    if isinstance(value, str):
        value = datetime.fromisoformat(value.replace('Z', '+00:00'))
    if value.tzinfo is None:
        value = value.replace(tzinfo=timezone.utc)
    return value.astimezone(timezone.utc)

def _stamp(value=None):
    return _time(value).isoformat()

@lru_cache(maxsize=1)
def _public_provinces():
    from .paths import resource_root
    catalog=json.loads((resource_root()/'catalogs/snapshot-2026-08-12.json').read_text())
    return {o['label'] for o in catalog['selectors']['provinces']['options']}


def _context(context):
    from ..config import TIE_PROCEDURE, REGRESO_PROCEDURE
    result={}
    allowed={
        'authentication':{'certificate','clave'}, 'auth':{'certificate','clave'},
        'clave_method':{'movil','permanente'},
        'platform':{'Darwin','Windows','Linux'}, 'os':{'Darwin','Windows','Linux'},
        'architecture':{'arm64','aarch64','x86_64','AMD64','i386','x86'},
        'procedure':{TIE_PROCEDURE,REGRESO_PROCEDURE,'huellas','regreso'},
        'province':_public_provinces(),
    }
    for key,values in allowed.items():
        if context.get(key) in values:result[key]=context[key]
    version=context.get('app_version','')
    if isinstance(version,str) and (version=='historical' or re.fullmatch(r'\d+\.\d+\.\d+(?:[.+-][A-Za-z0-9.-]+)?',version)):
        result['app_version']=version
    return result


def _digest(value):
    return hashlib.sha256(str(value).encode()).hexdigest()


def _active_clip(start, end, lower, upper):
    left, right = _time(start), _time(end)
    if lower:
        left = max(left, _time(lower))
    if upper:
        right = min(right, _time(upper))
    return max(0, (right-left).total_seconds())

def _active_days(start, end, lower, upper):
    left, right = _time(start), _time(end)
    if lower:
        left = max(left, _time(lower))
    if upper:
        right = min(right, _time(upper))
    days = []
    cursor = left.astimezone(MADRID)
    while cursor.astimezone(timezone.utc) < right:
        days.append(cursor.date().isoformat())
        cursor = (cursor + timedelta(days=1)).replace(hour=0, minute=0, second=0, microsecond=0)
    return json.dumps(days)

class ProfileStatistics:
    def __init__(self, path):
        self.path = Path(path)
        self.db_path = self.path.with_suffix('.sqlite3')
        self._initialized = False

    def _legacy(self, path=None):
        path = Path(path) if path is not None else self.path
        if not path.exists():
            return {}
        raw = json.loads(path.read_text(encoding='utf-8'))
        if not isinstance(raw, dict):
            raise ValueError('Invalid profile statistics')
        for key, value in list(raw.items()):
            if not isinstance(key, str):
                raise ValueError('Invalid profile statistics')
            if type(value) is int and value >= 0:
                raw[key] = _empty() | {'checks': value, '_provenance': {'known': ['checks'], 'positive': [], 'active_seconds': None}}
            elif isinstance(value, dict):
                provenance = {'known': [k for k in (*COUNTERS, 'active_seconds', 'started_at', 'first_available_at', 'first_available_seconds', '_offices') if k in value],
                    'positive': [k for k in ('offices_seen', 'booking_screens') if value.get(k, 0)],
                    'active_seconds': value.get('active_seconds')}
                if value.get('_offices') and 'offices_seen' not in provenance['positive']:
                    provenance['positive'].append('offices_seen')
                if value.get('first_available_at') and not provenance['positive']:
                    provenance['positive'] = ['offices_seen', 'booking_screens']
                row = _empty() | value
                if any(type(row[k]) is not int or row[k] < 0 for k in COUNTERS):
                    raise ValueError('Invalid profile statistics')
                if type(row['active_seconds']) not in (int, float) or not math.isfinite(row['active_seconds']) or row['active_seconds'] < 0:
                    raise ValueError('Invalid profile statistics')
                first_seconds = row['first_available_seconds']
                if first_seconds is not None and (type(first_seconds) not in (int, float) or not math.isfinite(first_seconds) or first_seconds < 0):
                    raise ValueError('Invalid profile statistics')
                if any(not isinstance(row[k], list) or any(not isinstance(v, str) for v in row[k]) for k in ('_offices', '_observations')):
                    raise ValueError('Invalid profile statistics')
                if not isinstance(row.get('_last_office', ''), str):
                    raise ValueError('Invalid profile statistics')
                for k in ('started_at', 'first_available_at'):
                    if row[k] is not None:
                        row[k] = _stamp(row[k])
                provenance['legacy_first_available_at'] = row['first_available_at']
                provenance['legacy_first_available_elapsed_seconds'] = row['first_available_seconds']
                row['_provenance'] = provenance
                raw[key] = row
            else:
                raise ValueError('Invalid profile statistics')
        return raw


    @contextmanager
    def _db(self, write=True):
        self.path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        try:
            db = sqlite3.connect(self.db_path, timeout=10)
        except sqlite3.Error:
            raise OSError('Statistics storage unavailable') from None
        os.chmod(self.db_path, 0o600)
        db.row_factory = sqlite3.Row
        try:
            db.execute('PRAGMA busy_timeout=30000')
            if not self._initialized:
                try:
                    db.execute('PRAGMA journal_mode=WAL')
                except sqlite3.OperationalError as exc:
                    # Another initializing connection may be switching the mode.
                    # BEGIN IMMEDIATE below still serializes schema/migration.
                    if 'locked' not in str(exc).lower():
                        raise
            db.create_function('madrid_day', 1, lambda value: _time(value).astimezone(MADRID).date().isoformat())
            db.create_function('active_clip', 4, _active_clip)
            db.create_function('active_days', 4, _active_days)
            db.execute('BEGIN IMMEDIATE' if write or not self._initialized else 'BEGIN')
            if self._initialized:
                yield db
                db.commit()
                return
            db.execute('CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value TEXT)')
            db.execute('CREATE TABLE IF NOT EXISTS baseline (profile TEXT PRIMARY KEY, data TEXT NOT NULL)')
            db.execute('CREATE TABLE IF NOT EXISTS baseline_provenance (profile TEXT PRIMARY KEY, data TEXT NOT NULL, migrated_at TEXT NOT NULL)')
            db.execute('CREATE TABLE IF NOT EXISTS observed_active (profile TEXT, session TEXT, start TEXT, end TEXT, UNIQUE(profile,session,start,end))')
            db.execute('CREATE INDEX IF NOT EXISTS observed_active_profile_range ON observed_active(profile,start,end)')
            db.execute('CREATE TABLE IF NOT EXISTS events (id INTEGER PRIMARY KEY, profile TEXT, identity TEXT UNIQUE, kind TEXT, time TEXT, session TEXT, stage TEXT, mode TEXT, counted INTEGER, offices TEXT, evidence TEXT)')
            db.execute('CREATE INDEX IF NOT EXISTS events_profile_time ON events(profile,time)')
            db.execute('CREATE TABLE IF NOT EXISTS active (id INTEGER PRIMARY KEY, profile TEXT, session TEXT, start TEXT, end TEXT, seconds REAL)')
            db.execute('CREATE TABLE IF NOT EXISTS sessions (profile TEXT, session TEXT, start TEXT, end TEXT, last_seen TEXT, reason TEXT, context TEXT, PRIMARY KEY(profile,session))')
            db.execute('CREATE TABLE IF NOT EXISTS episodes (id INTEGER PRIMARY KEY, profile TEXT, session TEXT, kind TEXT, stage TEXT, mode TEXT, document TEXT, start TEXT, end TEXT, last_seen TEXT, confirmed INTEGER DEFAULT 0)')
            db.execute('CREATE INDEX IF NOT EXISTS events_profile_kind_time ON events(profile,kind,time)')
            db.execute('CREATE INDEX IF NOT EXISTS active_profile_range ON active(profile,start,end)')
            db.execute('CREATE UNIQUE INDEX IF NOT EXISTS active_identity ON active(profile,session,start,end)')
            db.execute('CREATE TABLE IF NOT EXISTS activity_days(profile TEXT,day TEXT,PRIMARY KEY(profile,day))')
            db.execute('CREATE INDEX IF NOT EXISTS episodes_profile_range ON episodes(profile,start,last_seen)')
            db.execute('CREATE INDEX IF NOT EXISTS episodes_open ON episodes(profile,session,kind,end)')
            episode_columns = {row[1] for row in db.execute('PRAGMA table_info(episodes)')}
            for column, default in (('recognition', "'known'"), ('evidence', "'[]'"), ('variant', "''")):
                if column not in episode_columns:
                    db.execute(f'ALTER TABLE episodes ADD COLUMN {column} TEXT DEFAULT {default}')
            if not db.execute("SELECT 1 FROM metadata WHERE key='migrated'").fetchone():
                legacy = self._legacy()
                if self.path.exists():
                    backup = self.path.with_suffix(self.path.suffix + '.bak')
                    try:
                        fd = os.open(backup, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o400)
                    except FileExistsError:
                        if backup.read_bytes() != self.path.read_bytes():
                            raise ValueError('Legacy statistics backup differs')
                    else:
                        with os.fdopen(fd, 'wb') as stream:
                            stream.write(self.path.read_bytes())
                            stream.flush()
                            os.fsync(stream.fileno())
                for profile, row in legacy.items():
                    # Strict whitelist: never carry unknown legacy fields into SQLite.
                    safe = {key: row[key] for key in (*COUNTERS, 'active_seconds', 'started_at', 'first_available_at', 'first_available_seconds', '_offices', '_observations')}
                    safe['_observations'] = [v for v in safe['_observations'] if re.fullmatch(r'[0-9a-f]{64}', v)]
                    safe['_last_office'] = row.get('_last_office', '')
                    db.execute('INSERT INTO baseline VALUES (?,?)', (profile, json.dumps(safe)))
                    db.execute('INSERT OR IGNORE INTO baseline_provenance VALUES (?,?,?)',
                        (profile, json.dumps(row['_provenance']), _stamp()))
                db.execute("INSERT INTO metadata VALUES ('migrated','1')")
            if not db.execute("SELECT 1 FROM metadata WHERE key='baseline_provenance_ready'").fetchone() or not db.execute("SELECT 1 FROM metadata WHERE key='baseline_provenance_v2_ready'").fetchone():
                # Recover original facts, never infer them from reconciled/default-filled totals.
                backup = self.path.with_suffix(self.path.suffix + '.bak')
                try:
                    original = self._legacy(backup if backup.exists() else self.path)
                except (ValueError, TypeError, OSError):
                    original = {}
                for baseline_row in db.execute('SELECT profile FROM baseline').fetchall():
                    profile = baseline_row['profile']
                    provenance = original.get(profile, {}).get('_provenance',
                        {'known': [], 'positive': [], 'active_seconds': None})
                    db.execute('INSERT OR IGNORE INTO baseline_provenance VALUES (?,?,?)',
                        (profile, json.dumps(provenance), _stamp()))
                    existing = json.loads(db.execute('SELECT data FROM baseline_provenance WHERE profile=?', (profile,)).fetchone()[0])
                    if 'legacy_first_available_at' not in existing:
                        existing.update({key: provenance.get(key) for key in
                            ('legacy_first_available_at', 'legacy_first_available_elapsed_seconds')})
                        existing['known'] = list(dict.fromkeys(existing['known'] + provenance['known']))
                        db.execute('UPDATE baseline_provenance SET data=? WHERE profile=?', (json.dumps(existing), profile))
                db.execute("INSERT OR IGNORE INTO metadata VALUES ('baseline_provenance_ready','1')")
                db.execute("INSERT OR IGNORE INTO metadata VALUES ('baseline_provenance_v2_ready','1')")
            if not db.execute("SELECT 1 FROM metadata WHERE key='activity_days_ready'").fetchone():
                db.execute("INSERT OR IGNORE INTO activity_days SELECT profile,madrid_day(time) FROM events WHERE kind NOT IN ('finish','start','pause','resume','handoff','manual_required','fatal_error')")
                db.execute("INSERT OR IGNORE INTO activity_days SELECT profile,j.value FROM active,json_each(active_days(start,end,NULL,NULL)) j")
                db.execute("INSERT INTO metadata VALUES ('activity_days_ready','1')")
            db.execute("INSERT OR IGNORE INTO metadata VALUES ('schema_version','1')")
            db.commit()
            self._initialized = True
            db.execute('BEGIN IMMEDIATE' if write else 'BEGIN')
            yield db
            db.commit()
        except sqlite3.Error:
            db.rollback()
            raise OSError('Statistics storage unavailable') from None
        except BaseException:
            db.rollback()
            raise
        finally:
            db.close()

    @staticmethod
    def _bounds(period='all', at=None):
        if period == 'all':
            return None, None
        if period != 'today':
            raise ValueError('Invalid statistics period')
        local = _time(at).astimezone(MADRID)
        start = local.replace(hour=0, minute=0, second=0, microsecond=0)
        return _stamp(start), _stamp(start + timedelta(days=1))

    @staticmethod
    def _event_where(profile_id, lower=None, upper=None, kind=None):
        clauses, args = ['profile=?'], [profile_id]
        if lower:
            clauses.append('time>=?')
            args.append(lower)
        if upper:
            clauses.append('time<?')
            args.append(upper)
        if kind:
            clauses.append('kind=?')
            args.append(kind)
        return ' AND '.join(clauses), args

    def get(self, profile_id, period='all', at=None):
        lower, upper = self._bounds(period, at)
        where, args = self._event_where(profile_id, lower, upper)
        active_where, active_args = ['profile=?'], [profile_id]
        if lower:
            active_where.append('end>?')
            active_args.append(lower)
        if upper:
            active_where.append('start<?')
            active_args.append(upper)
        active_where = ' AND '.join(active_where)
        counters = ','.join(f"SUM(CASE WHEN counted=1 AND kind='{kind}' AND (kind!='office' OR offices!='[]') THEN 1 ELSE 0 END) AS {counter}" for kind, counter in COUNTER_KIND.items())
        with self._db(write=False) as db:
            baseline = db.execute('SELECT data FROM baseline WHERE profile=?', (profile_id,)).fetchone()
            row = (_empty() | json.loads(baseline[0])) if baseline and period == 'all' else _empty()
            origin = db.execute('SELECT data,migrated_at FROM baseline_provenance WHERE profile=?', (profile_id,)).fetchone() if baseline and period == 'all' else None
            provenance = json.loads(origin['data']) if origin else {'known': [], 'positive': [], 'active_seconds': None}
            if row['started_at']:
                row['started_at'] = _stamp(row['started_at'])
            if row['first_available_at']:
                row['first_available_at'] = _stamp(row['first_available_at'])
            facts = db.execute(f"SELECT {counters},MIN(time) AS started,MIN(CASE WHEN kind='office' AND offices!='[]' THEN time END) AS office,MIN(CASE WHEN kind='booking' THEN time END) AS booking FROM events WHERE {where}", args).fetchone()
            for counter in COUNTERS:
                row[counter] += facts[counter] or 0
            first_office, first_booking = facts['office'], facts['booking']
            if lower is None and upper is None:
                activity=db.execute('SELECT SUM(seconds) AS seconds,MIN(start) AS started FROM active WHERE profile=?',(profile_id,)).fetchone()
            else:
                activity = db.execute(f'SELECT SUM(CASE WHEN start>=? AND end<=? THEN seconds ELSE active_clip(start,end,?,?) END) AS seconds,MIN(CASE WHEN start<? THEN ? ELSE start END) AS started FROM active WHERE {active_where}', [lower,upper,lower,upper,lower,lower,*active_args]).fetchone()
            row['active_seconds'] += activity['seconds'] or 0
            row['started_at'] = min(filter(None, [row['started_at'], facts['started'], activity['started']]), default=None)
            offices = set(row['_offices'])
            offices.update(r[0] for r in db.execute(f"SELECT DISTINCT j.value FROM events,json_each(events.offices) j WHERE {where} AND kind='office'", args))
            day_where,day_args='profile=?',[profile_id]
            if lower:
                day_where+=' AND day>=? AND day<?'
                day_args.extend([_time(lower).astimezone(MADRID).date().isoformat(),_time(upper).astimezone(MADRID).date().isoformat()])
            days=db.execute('SELECT COUNT(*) FROM activity_days WHERE '+day_where,day_args).fetchone()[0]
            def legacy_first(value, counter, other):
                legacy_at = provenance.get('legacy_first_available_at')
                unambiguous = (baseline and period == 'all' and legacy_at
                    and counter in provenance['positive']
                    and other in provenance['known'] and other not in provenance['positive'])
                if unambiguous and (not value or _time(legacy_at) <= _time(value)):
                    return legacy_at, True
                return value, False
            first_office, legacy_office = legacy_first(first_office, 'offices_seen', 'booking_screens')
            first_booking, legacy_booking = legacy_first(first_booking, 'booking_screens', 'offices_seen')
            def first_fact(value, counter, legacy_known=False):
                historical = bool(baseline and period == 'all')
                absent = not historical or (counter in provenance['known'] and counter not in provenance['positive'])
                status = ('known' if absent or legacy_known else 'recorded') if value else ('not_seen' if absent else 'unknown')
                if not value:
                    return status, None, 'unknown'
                # Union supported intervals across sources/sessions, clipping at the event.
                # Window aggregation avoids materializing the lifetime timeline in Python.
                bounds = 'profile=? AND start<?' + (' AND end>?' if lower else '')
                interval_args = [profile_id, value, *([lower] if lower else [])]
                observed = db.execute('SELECT 1 FROM observed_active WHERE ' + bounds + ' LIMIT 1', interval_args).fetchone()
                if observed:
                    duration = db.execute("""WITH intervals AS (
                        SELECT MAX(start,?) AS start,MIN(end,?) AS end FROM active WHERE """ + bounds + """
                        UNION ALL SELECT MAX(start,?),MIN(end,?) FROM observed_active WHERE """ + bounds + """
                    ), ordered AS (
                        SELECT start,end,MAX(end) OVER (ORDER BY start,end ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING) AS previous FROM intervals
                    ) SELECT COALESCE(SUM(active_clip(MAX(start,COALESCE(previous,start)),end,NULL,NULL)),0) FROM ordered""",
                        [lower or '', value, *interval_args, lower or '', value, *interval_args]).fetchone()[0]
                else:
                    duration = db.execute('SELECT COALESCE(SUM(CASE WHEN end<=?' + (' AND start>=?' if lower else '') + ' THEN seconds ELSE active_clip(start,end,?,?) END),0) FROM active WHERE ' + bounds,
                        [value, *([lower] if lower else []), lower, value, *interval_args]).fetchone()[0]
                quality = 'exact'
                if observed:
                    quality = 'minimum'
                elif historical:
                    old_active = provenance.get('active_seconds')
                    after_migration = origin and _time(value) >= _time(origin['migrated_at'])
                    if absent and old_active is not None and (old_active == 0 or after_migration):
                        duration += old_active
                    else:
                        quality = 'minimum' if duration else 'unknown'
                return status, duration if quality != 'unknown' else None, quality
            office_status, office_seconds, office_quality = first_fact(first_office, 'offices_seen', legacy_office)
            booking_status, booking_seconds, booking_quality = first_fact(first_booking, 'booking_screens', legacy_booking)
        available = min(filter(None, [row['first_available_at'], first_office, first_booking]), default=None)
        if available and (not row['first_available_at'] or available != row['first_available_at']):
            row['first_available_seconds'] = max(0, (_time(available)-_time(row['started_at'])).total_seconds())
        return {k: row[k] for k in (*COUNTERS, 'active_seconds', 'started_at', 'first_available_seconds')} | dict(
            first_available_at=available, unique_offices=len(offices), days=days,
            legacy_first_available_at=provenance.get('legacy_first_available_at'),
            legacy_first_available_elapsed_seconds=provenance.get('legacy_first_available_elapsed_seconds'),
            legacy_unknown_fields=[('unique_offices' if key == '_offices' else key) for key in
                (*COUNTERS, 'active_seconds', 'started_at', 'first_available_at', 'first_available_seconds', '_offices')
                if baseline and period == 'all' and key not in provenance['known']],
            first_office_at=first_office, first_office_seconds=office_seconds,
            first_office_status=office_status, first_office_active_quality=office_quality,
            first_booking_at=first_booking, first_booking_seconds=booking_seconds,
            first_booking_status=booking_status, first_booking_active_quality=booking_quality, legacy_baseline=bool(baseline))

    def total(self, profile_id, period='all', at=None):
        lower, upper = self._bounds(period, at)
        where, args = self._event_where(profile_id, lower, upper, 'check')
        with self._db(write=False) as db:
            baseline = db.execute('SELECT data FROM baseline WHERE profile=?', (profile_id,)).fetchone() if period == 'all' else None
            legacy = json.loads(baseline[0])['checks'] if baseline else 0
            return legacy + db.execute(f'SELECT COUNT(*) FROM events WHERE {where} AND counted=1', args).fetchone()[0]

    def increment(self, profile_id, *, at=None, session_id=''):
        self.event(profile_id, 'check', at=at, session_id=session_id)
        return self.total(profile_id)

    def observe(self, profile_id, kind, document, *, offices=(), at=None, session_id='', summary=True):
        self.event(profile_id, kind, document, offices=offices, at=at, session_id=session_id)
        return self.get(profile_id) if summary else None

    def event(self, profile_id, kind, document='', at=None, session_id='', stage='', mode='', offices=(), counted=True, **safe_fields):
        timestamp = _stamp(at)
        stage = stage if stage in STAGES else ('unknown' if stage else '')
        mode = mode if mode in MODES else ('unknown' if mode else '')
        if kind not in EVENTS:
            raise ValueError('Invalid event kind')
        labels = list(dict.fromkeys(str(label)[:300] for label in offices if label))
        evidence = [v for v in safe_fields.get('evidence', ()) if v in {'loading', 'unknown', 'ambiguous', 'known', 'known_variant', 'http_429', 'http_5xx', 'server_error', 'rate_limited', 'blocked', 'expired', 'no_appointments', 'operation_code', 'network_error', 'timeout'}]
        identity = _digest(f'{profile_id}:{session_id}:{document}:{kind}') if document and kind in COUNTER_KIND else None
        http_status = safe_fields.get('http_status')
        evidence = {'codes': evidence, 'http_status': http_status if type(http_status) is int and 100 <= http_status <= 599 else None}
        if type(safe_fields.get('browser_generation')) is int:evidence['browser_generation']=safe_fields['browser_generation']
        if safe_fields.get('previous_stage') in STAGES:evidence['previous_stage']=safe_fields['previous_stage']
        if safe_fields.get('initiator') in {'user','automatic'}:evidence['initiator']=safe_fields['initiator']
        with self._db() as db:
            legacy = db.execute('SELECT data FROM baseline WHERE profile=?', (profile_id,)).fetchone()
            legacy_key = _digest(f'{document}:{kind}')
            migrated_duplicate = legacy and not session_id and legacy_key in json.loads(legacy[0]).get('_observations', [])
            if not migrated_duplicate:
                inserted = db.execute('INSERT OR IGNORE INTO events(profile,identity,kind,time,session,stage,mode,counted,offices,evidence) VALUES (?,?,?,?,?,?,?,?,?,?)',
                    (profile_id, identity, kind, timestamp, session_id, stage, mode, int(bool(counted)), json.dumps(labels), json.dumps(evidence))).rowcount
                if inserted and kind not in {'finish','start','pause','resume','handoff','manual_required','fatal_error'}:
                    db.execute('INSERT OR IGNORE INTO activity_days VALUES (?,?)',(profile_id,_time(timestamp).astimezone(MADRID).date().isoformat()))
        return None

    def add_active(self, profile_id, seconds, *, at=None, session_id='', summary=True):
        if type(seconds) not in (int, float) or not math.isfinite(seconds) or seconds < 0:
            raise ValueError('Invalid active duration')
        end = _time(at)
        with self._db() as db:
            if seconds:
                for day in json.loads(_active_days(_stamp(end-timedelta(seconds=seconds)),_stamp(end),None,None)):
                    db.execute('INSERT OR IGNORE INTO activity_days VALUES (?,?)',(profile_id,day))
                db.execute('INSERT OR IGNORE INTO active(profile,session,start,end,seconds) VALUES (?,?,?,?,?)',
                    (profile_id, session_id, _stamp(end-timedelta(seconds=seconds)), _stamp(end), seconds))
        return self.get(profile_id) if summary else None

    def add_observed_active(self, profile_id, start, end, session_id=''):
        """Store conservative dated evidence separately from measured lifetime totals."""
        start, end = _time(start), _time(end)
        if end < start:
            raise ValueError('Invalid observed active range')
        if end == start:
            return None
        with self._db() as db:
            db.execute('INSERT OR IGNORE INTO observed_active VALUES (?,?,?,?)',
                (profile_id, session_id, _stamp(start), _stamp(end)))
            for day in json.loads(_active_days(start, end, None, None)):
                db.execute('INSERT OR IGNORE INTO activity_days VALUES (?,?)', (profile_id, day))
        return None

    def known_offices(self, profile_id, period='all', at=None):
        labels = {r['label'] for r in self.office_history(profile_id, period, at)}
        if period == 'all':
            with self._db(write=False) as db:
                baseline = db.execute('SELECT data FROM baseline WHERE profile=?', (profile_id,)).fetchone()
                if baseline:
                    labels.update(json.loads(baseline[0])['_offices'])
        return sorted(labels)

    def last_office(self, profile_id, period='all', at=None):
        history = [r for r in self.office_history(profile_id, period, at) if r['last_seen']]
        if history:
            return max(enumerate(history), key=lambda item: (item[1]['last_seen'], item[0]))[1]['label']
        if period == 'all':
            with self._db(write=False) as db:
                baseline = db.execute('SELECT data FROM baseline WHERE profile=?', (profile_id,)).fetchone()
                if baseline:
                    return json.loads(baseline[0]).get('_last_office', '')
        return ''

    def start_session(self, profile_id, session_id, context, at=None):
        timestamp = _stamp(at)
        context = _context(context)
        with self._db() as db:
            db.execute('INSERT OR IGNORE INTO sessions VALUES (?,?,?,?,?,?,?)', (profile_id, session_id, timestamp, None, timestamp, '', json.dumps(context)))

    def end_session(self, profile_id, session_id, reason, at=None):
        timestamp = _stamp(at)
        with self._db() as db:
            db.execute('UPDATE sessions SET end=?,last_seen=?,reason=? WHERE profile=? AND session=?', (timestamp, timestamp, reason if reason in {'completed','failed','stopped','interrupted','historical_finish','last_observation'} else 'unknown', profile_id, session_id))
            db.execute('UPDATE episodes SET end=last_seen WHERE profile=? AND session=? AND end IS NULL', (profile_id, session_id))

    def _episode(self, profile_id, session_id, kind, stage='', mode='', document='', at=None, recognition='known', evidence=(), variant=''):
        timestamp = _stamp(at)
        document = _digest(document)
        variant = variant if isinstance(variant,str) and re.fullmatch(r'[0-9a-f]{64}',variant) else ''
        stage = stage if stage in STAGES else ('unknown' if stage else '')
        mode = mode if mode in MODES else ('unknown' if mode else '')
        recognition = recognition if recognition in {'known', 'known_variant', 'ambiguous', 'unknown', 'loading', 'read_error'} else 'unknown'
        evidence = [v for v in evidence if v in {'loading', 'unknown', 'ambiguous', 'known', 'known_variant', 'http_429', 'http_5xx', 'server_error', 'rate_limited', 'blocked', 'expired', 'no_appointments', 'operation_code', 'network_error', 'timeout'}]
        with self._db() as db:
            current = db.execute('SELECT * FROM episodes WHERE profile=? AND session=? AND kind=? AND end IS NULL ORDER BY id DESC LIMIT 1', (profile_id, session_id, kind)).fetchone()
            if current and kind == 'page' and stage == 'loading' and current['stage'] in {'expired','blocked','server_error','rate_limited','unknown'}:
                return None  # Loading is transitional, not evidence of recovery.
            continuous = current and (_time(timestamp)-_time(current['last_seen'])).total_seconds() <= 35
            if current and kind == 'page' and not continuous:
                # A live worker waiting between requests retains the last state.
                # Explicit observation gaps close pages before this path.
                pulse = db.execute("SELECT last_seen FROM episodes WHERE profile=? AND session=? AND kind='phase' AND end IS NULL AND mode!='unobserved' ORDER BY id DESC LIMIT 1", (profile_id,session_id)).fetchone()
                continuous = bool(pulse and 0 <= (_time(timestamp)-_time(pulse['last_seen'])).total_seconds() <= 35)
            same_state = current and (current['stage'], current['mode']) == (stage, mode)
            if same_state and kind == 'page':
                same_state = (current['recognition'] == recognition
                    and set(json.loads(current['evidence'])) == set(evidence)
                    and current['variant'] == variant)
            if continuous and same_state:
                db.execute('UPDATE episodes SET last_seen=?,recognition=?,evidence=?,variant=? WHERE id=?', (timestamp, recognition, json.dumps(evidence), variant or current['variant'], current['id']))
            else:
                if current:
                    db.execute('UPDATE episodes SET end=?,last_seen=?,confirmed=? WHERE id=?', (timestamp if continuous else current['last_seen'],timestamp if continuous and kind != 'page' else current['last_seen'],int(bool(continuous)),current['id']))
                db.execute('INSERT INTO episodes(profile,session,kind,stage,mode,document,start,last_seen,recognition,evidence,variant) VALUES (?,?,?,?,?,?,?,?,?,?,?)', (profile_id, session_id, kind, stage, mode, document, timestamp, timestamp, recognition, json.dumps(evidence), variant))
        return None

    def observe_page(self, profile_id, session_id, stage, document, at=None, recognition='known', evidence=(), variant=''):
        return self._episode(profile_id, session_id, 'page', stage=stage, document=document, at=at, recognition=recognition, evidence=evidence, variant=variant)

    def phase(self, profile_id, session_id, mode, at=None):
        if mode == 'unobserved':
            with self._db() as db:
                db.execute('UPDATE episodes SET end=last_seen WHERE profile=? AND session=? AND end IS NULL', (profile_id, session_id))
        return self._episode(profile_id, session_id, 'phase', mode=mode, at=at)

    def heartbeat(self, profile_id, session_id, at=None):
        timestamp = _stamp(at)
        with self._db() as db:
            db.execute('UPDATE sessions SET last_seen=? WHERE profile=? AND session=? AND end IS NULL', (timestamp, profile_id, session_id))
            db.execute("UPDATE episodes SET last_seen=? WHERE profile=? AND session=? AND kind='phase' AND end IS NULL AND last_seen>=?",(timestamp,profile_id,session_id,_stamp(_time(timestamp)-timedelta(seconds=35))))
            # Worker liveness confirms its phase, never page contents.

    def history_bounds(self, profile_id, period='all', at=None):
        lower,upper=self._bounds(period,at)
        where,args=self._event_where(profile_id,lower,upper)
        with self._db(write=False) as db:
            points=db.execute('SELECT MIN(time),MAX(time) FROM events WHERE '+where,args).fetchone()
            pages=db.execute('SELECT MIN(start),MAX(COALESCE(end,last_seen)) FROM episodes WHERE profile=?'+(' AND COALESCE(end,last_seen)>=? AND start<?' if lower else ''),[profile_id]+([lower,upper] if lower else [])).fetchone()
        return dict(start=min(filter(None,[points[0],pages[0]]),default=None),
                    end=max(filter(None,[points[1],pages[1]]),default=None))

    def history(self, profile_id, period='all', at=None, start=None, end=None):
        lower, upper = self._bounds(period, at)
        lower, upper = _stamp(start) if start else lower, _stamp(end) if end else upper
        where, args = self._event_where(profile_id, lower, upper)
        episode_where, episode_args = ['profile=?'], [profile_id]
        if upper:
            episode_where.append('start<?')
            episode_args.append(upper)
        if lower:
            episode_where.append('COALESCE(end,last_seen)>=?')
            episode_args.append(lower)
        with self._db(write=False) as db:
            events = list(db.execute(f'SELECT * FROM events WHERE {where} ORDER BY time,id', args))
            episodes = list(db.execute('SELECT * FROM episodes WHERE ' + ' AND '.join(episode_where) + ' ORDER BY start,id', episode_args))
        result = [dict(kind=e['kind'], stage=e['stage'], mode=e['mode'], start=e['time'], end=e['time'], last_seen=e['time'], session_id=e['session'], confirmed_end=True, event=True, counted=bool(e['counted']), offices=json.loads(e['offices']), evidence=json.loads(e['evidence']), id=e['id']) for e in events]
        result += [dict(kind=e['kind'], stage=e['stage'], mode=e['mode'], start=e['start'], end=e['end'], last_seen=e['last_seen'], session_id=e['session'], confirmed_end=bool(e['confirmed']), event=False, id=e['id'], recognition=e['recognition'], evidence=json.loads(e['evidence']),variant=e['variant']) for e in episodes]
        return sorted(result, key=lambda r: r['start'])

    def timeline_bounds(self, profile_id, period='all', at=None):
        bounds=self.history_bounds(profile_id,period,at)
        lower,upper=self._bounds(period,at)
        clauses=['profile=?'];args=[profile_id]
        if upper:clauses.append('start<?');args.append(upper)
        if lower:clauses.append('COALESCE(end,last_seen)>=?');args.append(lower)
        with self._db(write=False) as db:
            runs=db.execute('SELECT MIN(start),MAX(COALESCE(end,last_seen)) FROM sessions WHERE '+' AND '.join(clauses),args).fetchone()
        return dict(start=min(filter(None,[bounds.get('start'),runs[0]]),default=None),end=max(filter(None,[bounds.get('end'),runs[1]]),default=None))

    def timeline_history(self, profile_id, period='all', at=None, start=None, end=None):
        from .statistics_timeline import build_timeline
        lower,upper=self._bounds(period,at)
        lower,upper=_stamp(start) if start else lower,_stamp(end) if end else upper
        clauses=['profile=?'];args=[profile_id]
        if upper:clauses.append('start<?');args.append(upper)
        if lower:clauses.append('COALESCE(end,last_seen)>=?');args.append(lower)
        with self._db(write=False) as db:
            sessions=[dict(row) for row in db.execute('SELECT * FROM sessions WHERE '+' AND '.join(clauses)+' ORDER BY start',args)]
        # Context before the visible lower bound is necessary to restore a clip
        # spanning that bound. Raw history and lifetime aggregates stay untouched.
        context_start=min([run['start'] for run in sessions]+([lower] if lower else []),default=None)
        rows=self.history(profile_id,period='all',start=context_start,end=upper)
        if not hasattr(self,'_timeline_projection_cache'):self._timeline_projection_cache={}
        result=build_timeline(rows,sessions,cache=self._timeline_projection_cache)
        return [row for row in result if (not upper or row['start']<upper) and
            (not lower or (row.get('end') or row['last_seen'])>=lower)]

    def office_history(self, profile_id, period='all', at=None):
        lower, upper = self._bounds(period, at)
        where, args = self._event_where(profile_id, lower, upper, 'office')
        with self._db(write=False) as db:
            events = list(db.execute(f'SELECT time,session,offices FROM events WHERE {where} ORDER BY time,id', args))
            legacy=db.execute('SELECT data FROM baseline WHERE profile=?',(profile_id,)).fetchone() if period=='all' else None
        result = {}
        for event in events:
            for label in json.loads(event['offices']):
                row = result.setdefault(label, dict(label=label, count=0, last_seen=event['time'], appearances=[]))
                row['count'] += 1
                row['last_seen'] = event['time']
                row['appearances'].append(dict(time=event['time'], session_id=event['session']))
        if legacy:
            for label in json.loads(legacy[0])['_offices']:
                result.setdefault(label,dict(label=label,count=None,last_seen=None,appearances=[]))
        return sorted(result.values(),key=lambda row:row['last_seen'] or '',reverse=True)
