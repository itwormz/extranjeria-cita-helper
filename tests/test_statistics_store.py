from datetime import datetime, timezone
import json
from app.desktop.statistics import ProfileStatistics

def test_migration_keeps_original_and_daily_is_not_legacy(tmp_path):
    path = tmp_path / 'statistics.json'
    original = '{"a": 7}'
    path.write_text(original)
    stats = ProfileStatistics(path)
    assert stats.total('a') == 7
    assert stats.get('a', period='today')['checks'] == 0
    stats.increment('a')
    assert path.read_text() == original
    assert path.with_suffix('.json.bak').read_text() == original
    assert ProfileStatistics(path).total('a') == 8

def test_period_active_split_and_office_history(tmp_path):
    stats = ProfileStatistics(tmp_path / 'statistics.json')
    stats.add_active('a', 120, at='2026-01-02T00:01:00+01:00', session_id='s')
    stats.observe('a', 'office', 'private document', offices=['Office A'], at='2026-01-02T00:00:10+01:00', session_id='s')
    stats.observe('a', 'office', 'private document', offices=['Office A'], session_id='s')
    row = stats.get('a', period='today', at='2026-01-02T12:00:00+01:00')
    assert row['active_seconds'] == 60
    assert row['offices_seen'] == 1
    assert row['days'] == 1
    assert stats.office_history('a')[0]['count'] == 1
    assert stats.get('a')['first_office_at']
    assert b'private document' not in stats.db_path.read_bytes()

def test_page_episode_and_phase_end_are_confirmed_only_on_transition(tmp_path):
    stats = ProfileStatistics(tmp_path / 'statistics.json')
    stats.start_session('a', 's', {'province': 'Madrid', 'name': 'SECRET'})
    stats.observe_page('a', 's', 'loading', 'doc', at='2026-01-01T10:00:00Z', recognition='unknown', evidence=('loading', 'SECRET'))
    stats.observe_page('a', 's', 'loading', 'doc', at='2026-01-01T10:00:10Z')
    stats.observe_page('a', 's', 'office', 'next', at='2026-01-01T10:00:20Z')
    rows = [r for r in stats.history('a') if r['kind'] == 'page']
    assert len(rows) == 3  # changed recognition/evidence starts a new state
    assert rows[0]['evidence'] == ['loading']
    assert rows[1]['evidence'] == []
    assert rows[1]['confirmed_end']
    assert rows[1]['end'] == '2026-01-01T10:00:20+00:00'
    assert rows[1]['last_seen'] == '2026-01-01T10:00:10+00:00'
    assert not rows[2]['confirmed_end']
    assert b'SECRET' not in stats.db_path.read_bytes()


def test_dst_day_and_active_first_event(tmp_path):
    stats = ProfileStatistics(tmp_path / 's.json')
    stats.add_active('a', 120, at='2026-03-29T03:01:00+02:00')
    stats.observe('a', 'office', 'd', offices=['A'], at='2026-03-29T03:00:30+02:00')
    row = stats.get('a', 'today', at='2026-03-29T12:00:00+02:00')
    assert row['active_seconds'] == 120
    assert row['first_office_seconds'] == 90
    assert row['days'] == 1


def test_full_baseline_unknown_fields_are_discarded(tmp_path):
    path = tmp_path / 's.json'
    original = {'a': {'checks': 8, 'active_seconds': 99, '_offices': ['A'],
                      'person': 'DO NOT COPY', 'first_available_at': '2026-01-01T00:00:00Z',
                      'first_available_seconds': 11}}
    path.write_text(json.dumps(original))
    stats = ProfileStatistics(path)
    assert stats.get('a')['active_seconds'] == 99
    assert stats.get('a')['first_available_seconds'] == 11
    assert stats.known_offices('a') == ['A']
    assert b'DO NOT COPY' not in stats.db_path.read_bytes()
    stats.observe('a', 'office', 'new', offices=['B'])
    assert stats.get('a')['first_office_seconds'] is None


def test_event_privacy_and_history_only_counts(tmp_path):
    stats = ProfileStatistics(tmp_path / 's.json')
    stats.event('a', 'booking', 'RAW DOCUMENT', counted=False,
                detail='SECRET DETAIL', url='https://secret.invalid', evidence=['SECRET PERSON', 'http_429'])
    assert stats.get('a')['booking_screens'] == 0
    assert len(stats.history('a')) == 1
    data = stats.db_path.read_bytes()
    assert all(value not in data for value in [b'RAW DOCUMENT', b'SECRET', b'secret.invalid'])


def test_migration_once_and_session_scoped_dedup(tmp_path):
    path = tmp_path / 's.json'
    path.write_text('{"a":2}')
    stats = ProfileStatistics(path)
    stats.observe('a', 'blocked', 'd', session_id='one')
    stats.observe('a', 'blocked', 'd', session_id='one')
    stats.observe('a', 'blocked', 'd', session_id='two')
    assert ProfileStatistics(path).get('a')['blocked'] == 2
    assert stats.get('a')['checks'] == 2
    assert path.with_suffix('.json.bak').stat().st_mode & 0o222 == 0


def test_gap_and_heartbeat_do_not_invent_page_observation(tmp_path):
    stats = ProfileStatistics(tmp_path / 's.json')
    stats.observe_page('a', 's', 'loading', 'd', at='2026-01-01T10:00:00Z')
    stats.heartbeat('a', 's', at='2026-01-01T11:00:00Z')
    stats.phase('a', 's', 'unobserved', at='2026-01-01T11:00:00Z')
    row = [r for r in stats.history('a') if r['kind'] == 'page'][0]
    assert row['last_seen'] == row['end'] == '2026-01-01T10:00:00+00:00'
    assert not row['confirmed_end']
    stats.event('a', 'http_response', http_status=429)
    assert stats.history('a')[-1]['evidence']['http_status'] == 429


def _process_increment(path):
    ProfileStatistics(path).increment('a')


def test_process_writers_serialize_without_lost_updates(tmp_path):
    from concurrent.futures import ProcessPoolExecutor
    path = tmp_path / 's.json'
    with ProcessPoolExecutor(max_workers=3) as pool:
        list(pool.map(_process_increment, [path] * 12))
    assert ProfileStatistics(path).total('a') == 12


def test_corrupt_migration_has_no_baseline_and_can_retry(tmp_path):
    import pytest
    path = tmp_path / 's.json'
    path.write_text('{bad json')
    with pytest.raises(ValueError):
        ProfileStatistics(path).get('a')
    assert path.read_text() == '{bad json'
    assert not path.with_suffix('.json.bak').exists()
    path.write_text('{"a":4}')
    assert ProfileStatistics(path).total('a') == 4


def test_history_import_adds_dates_without_readding_lifetime_counters(tmp_path):
    path = tmp_path / 's.json'
    path.write_text('{"a":{"checks":100,"offices_seen":5}}')
    stats = ProfileStatistics(path)
    stats.event('a','office','archive-visit',at='2026-10-16T10:00:00Z',
                offices=['Office A'],counted=False,session_id='archive')
    row = stats.get('a')
    assert row['checks'] == 100 and row['offices_seen'] == 5
    assert row['days'] == 1
    assert row['first_office_at'] == '2026-10-16T10:00:00+00:00'
    assert row['unique_offices'] == 1
    assert row['first_office_seconds'] is None


def test_event_kinds_and_stages_are_enums_not_arbitrary_text(tmp_path):
    import pytest
    stats = ProfileStatistics(tmp_path / 's.json')
    with pytest.raises(ValueError):
        stats.event('a','SECRET PERSON NAME')
    stats.event('a','action',stage='SECRET PASSPORT',mode='SECRET FORM VALUE')
    assert b'SECRET' not in stats.db_path.read_bytes()


def test_last_office_preserves_order_of_last_result(tmp_path):
    stats = ProfileStatistics(tmp_path / 's.json')
    stats.observe('a','office','d',offices=['A','Z'])
    assert stats.last_office('a') == 'Z'


def test_repeated_action_events_in_one_document_are_not_discarded(tmp_path):
    stats = ProfileStatistics(tmp_path / 's.json')
    stats.event('a','action','d',at='2026-01-01T10:00:00Z')
    stats.event('a','action','d',at='2026-01-01T10:01:00Z')
    assert len([r for r in stats.history('a') if r['kind']=='action'])==2


def test_long_page_observation_gap_is_not_painted_as_continuous(tmp_path):
    stats = ProfileStatistics(tmp_path / 's.json')
    stats.observe_page('a','s','office','a',at='2026-01-01T10:00:00Z')
    stats.observe_page('a','s','office','b',at='2026-01-01T11:00:00Z')
    rows=[r for r in stats.history('a') if r['kind']=='page']
    assert rows[0]['end']==rows[0]['last_seen']=='2026-01-01T10:00:00+00:00'
    assert not rows[0]['confirmed_end']


def test_database_file_is_private(tmp_path):
    import os
    stats = ProfileStatistics(tmp_path/'s.json');stats.increment('a')
    if os.name!='nt':assert stats.db_path.stat().st_mode & 0o077 == 0


def test_hot_reads_use_filtered_aggregate_sql_and_read_transactions(tmp_path, monkeypatch):
    import app.desktop.statistics as module
    stats = ProfileStatistics(tmp_path / 's.json')
    stats.event('a', 'check', at='2020-01-01T00:00:00Z')
    statements = []
    connect = module.sqlite3.connect
    def traced(*args, **kwargs):
        db = connect(*args, **kwargs)
        db.set_trace_callback(statements.append)
        return db
    monkeypatch.setattr(module.sqlite3, 'connect', traced)
    stats.get('a', 'today', at='2026-01-01T12:00:00Z')
    assert not any('BEGIN IMMEDIATE' in sql for sql in statements)
    assert not any('SELECT * FROM events' in sql or 'SELECT * FROM active' in sql for sql in statements)
    assert any('time>=' in sql and '2026' in sql for sql in statements)
    statements.clear()
    stats.office_history('a')
    assert not any('FROM episodes' in sql for sql in statements)
    assert any("kind='office'" in sql for sql in statements)
    statements.clear()
    stats.history('a', start='2026-01-01T00:00:00Z', end='2026-01-02T00:00:00Z')
    assert all('2026' in sql for sql in statements if 'SELECT * FROM events' in sql or 'SELECT * FROM episodes' in sql)


def test_runtime_writes_can_skip_compatibility_summary(tmp_path, monkeypatch):
    stats = ProfileStatistics(tmp_path / 's.json')
    def no_summary(*args, **kwargs):
        raise AssertionError('unused summary queried')
    monkeypatch.setattr(stats, 'get', no_summary)
    assert stats.observe('a', 'office', 'd', offices=['A'], summary=False) is None
    assert stats.add_active('a', 2, summary=False) is None


def test_wal_reader_sees_committed_snapshot_while_writer_is_open(tmp_path):
    import sqlite3
    stats = ProfileStatistics(tmp_path / 's.json')
    stats.increment('a')
    writer = sqlite3.connect(stats.db_path)
    try:
        writer.execute('BEGIN IMMEDIATE')
        writer.execute("INSERT INTO events(profile,kind,time,counted,offices,evidence) VALUES ('a','check','2026-01-01T00:00:00+00:00',1,'[]','{}')")
        assert stats.total('a') == 1
        assert stats.get('a')['checks'] == 1
    finally:
        writer.rollback()
        writer.close()


def test_summary_aggregates_match_history_facts_with_mixed_profiles(tmp_path):
    stats = ProfileStatistics(tmp_path / 's.json')
    stats.event('a', 'office', 'old', offices=['A'], counted=False, at='2026-01-01T10:00:00Z')
    stats.event('a', 'booking', 'booking', counted=False, at='2026-01-02T10:00:00Z')
    stats.event('a', 'check', at='2026-01-02T10:00:00Z')
    stats.event('b', 'check', at='2026-01-02T10:00:00Z')
    stats.add_active('a', 120, at='2026-01-02T00:01:00+01:00', summary=False)
    today = stats.get('a', 'today', at='2026-01-02T12:00:00Z')
    assert today['checks'] == 1
    assert today['active_seconds'] == 60
    assert today['days'] == 1
    assert today['first_office_at'] is None
    all_time = stats.get('a')
    assert all_time['checks'] == 1
    assert all_time['offices_seen'] == 0
    assert all_time['unique_offices'] == 1
    assert all_time['days'] == 2
    assert all_time['first_booking_seconds'] == 120


def test_session_categories_and_unknown_shape_are_safe(tmp_path):
    stats=ProfileStatistics(tmp_path/'s.json')
    stats.start_session('a','s',{'province':'Barcelona','procedure':'huellas',
                               'authentication':'SECRET AUTH','os':'SECRET OS'})
    stats.observe_page('a','s','unknown','doc',recognition='unknown',variant='a'*64)
    stats.end_session('a','s','SECRET PRIVATE REASON')
    pages=[r for r in stats.history('a') if r['kind']=='page']
    assert pages[0]['variant']=='a'*64
    assert b'SECRET' not in stats.db_path.read_bytes()


def test_history_bounds_do_not_load_full_history(tmp_path,monkeypatch):
    stats=ProfileStatistics(tmp_path/'s.json')
    stats.observe_page('a','s','province','one',at='2026-01-01T10:00:00Z')
    stats.observe_page('a','s','office','two',at='2026-01-03T12:00:00Z')
    monkeypatch.setattr(stats,'history',lambda *a,**k: (_ for _ in ()).throw(AssertionError('unbounded history')))
    bounds=stats.history_bounds('a')
    assert bounds['start']=='2026-01-01T10:00:00+00:00'
    assert bounds['end']=='2026-01-03T12:00:00+00:00'


def test_invalid_sqlite_is_nonfatal_storage_error(tmp_path):
    import pytest
    stats=ProfileStatistics(tmp_path/'s.json')
    stats.db_path.write_bytes(b'broken sqlite')
    with pytest.raises(OSError):stats.get('a')


def test_finish_alone_is_not_a_search_day(tmp_path):
    stats=ProfileStatistics(tmp_path/'s.json')
    stats.event('a','finish',counted=False,at='2026-01-02T10:00:00Z')
    assert stats.get('a','today',at='2026-01-02T12:00:00Z')['days']==0


def test_lifetime_activity_does_not_call_per_interval_date_functions(tmp_path,monkeypatch):
    from app.desktop import statistics as module
    stats=ProfileStatistics(tmp_path/'s.json')
    stats.add_active('a',5,at='2026-01-02T10:00:00Z')
    def slow_path(*args):raise AssertionError('lifetime row-by-row date parsing')
    monkeypatch.setattr(module,'_active_clip',slow_path)
    monkeypatch.setattr(module,'_active_days',slow_path)
    assert stats.get('a')['active_seconds']==5


def test_worker_heartbeat_extends_only_current_bot_phase(tmp_path):
    stats=ProfileStatistics(tmp_path/'s.json')
    stats.phase('a','s','paused',at='2026-01-01T10:00:00Z')
    stats.observe_page('a','s','office','d',at='2026-01-01T10:00:00Z')
    stats.heartbeat('a','s',at='2026-01-01T10:00:30Z')
    rows=stats.history('a')
    assert next(r for r in rows if r['kind']=='phase')['last_seen']=='2026-01-01T10:00:30+00:00'
    assert next(r for r in rows if r['kind']=='page')['last_seen']=='2026-01-01T10:00:00+00:00'


def test_legacy_office_without_dates_stays_visible_with_unknown_count(tmp_path):
    path=tmp_path/'s.json';path.write_text('{"a":{"_offices":["Office A"],"offices_seen":3}}')
    stats=ProfileStatistics(path)
    row=stats.office_history('a')[0]
    assert row['label']=='Office A'
    assert row['count'] is None and row['appearances']==[]
    assert stats.office_history('a','today')==[]


# These regressions catch treating unknown legacy fields as known zero,
# suppressing known-zero baselines, and adding reconstructed evidence twice.
def _rich_zero(**changes):
    from app.desktop.statistics import COUNTERS
    return dict.fromkeys(COUNTERS, 0) | dict(active_seconds=0, started_at=None,
        first_available_at=None, first_available_seconds=None,
        _offices=[], _observations=[]) | changes


def test_scalar_history_new_event_is_recorded_not_first_ever(tmp_path):
    path = tmp_path / 's.json'; path.write_text('{"a": 8}')
    stats = ProfileStatistics(path)
    stats.add_active('a', 20, at='2030-01-01T10:00:20Z')
    stats.observe('a', 'office', 'd', offices=['A'], at='2030-01-01T10:00:15Z')
    row = ProfileStatistics(path).get('a')
    assert row['first_office_status'] == 'recorded'
    assert row['first_office_seconds'] == 15
    assert row['first_office_active_quality'] == 'minimum'
    assert row['first_booking_status'] == 'unknown'
    assert row['checks'] == 8


def test_known_zero_baseline_does_not_suppress_exact_first_active(tmp_path):
    path = tmp_path / 's.json'; path.write_text(json.dumps({'a': _rich_zero()}))
    stats = ProfileStatistics(path)
    stats.add_active('a', 20, at='2030-01-01T10:00:20Z')
    stats.observe('a', 'office', 'd', offices=['A'], at='2030-01-01T10:00:15Z')
    row = stats.get('a')
    assert row['first_office_status'] == 'known'
    assert row['first_office_seconds'] == 15
    assert row['first_office_active_quality'] == 'exact'
    assert row['first_booking_status'] == 'not_seen'


def test_prior_offices_do_not_block_known_first_booking_active(tmp_path):
    path = tmp_path / 's.json'
    path.write_text(json.dumps({'a': _rich_zero(offices_seen=3, active_seconds=40,
        first_available_at='2025-01-01T00:00:00Z', first_available_seconds=100)}))
    stats = ProfileStatistics(path); stats.get('a')
    stats.add_active('a', 20, at='2030-01-01T10:00:20Z')
    stats.observe('a', 'booking', 'd', at='2030-01-01T10:00:15Z')
    row = stats.get('a')
    assert row['first_booking_status'] == 'known'
    assert row['first_booking_seconds'] == 55
    assert row['first_booking_active_quality'] == 'exact'
    assert row['first_available_seconds'] == 100  # old wall elapsed remains intact


def test_observed_active_union_is_minimum_and_does_not_inflate_lifetime(tmp_path):
    stats = ProfileStatistics(tmp_path / 's.json')
    stats.add_active('a', 10, at='2026-01-01T10:00:20Z')
    stats.add_observed_active('a', '2026-01-01T10:00:00Z', '2026-01-01T10:00:15Z', session_id='archive')
    stats.add_observed_active('a', '2026-01-01T10:00:00Z', '2026-01-01T10:00:15Z', session_id='archive')
    stats.add_observed_active('a', '2026-01-01T10:00:12Z', '2026-01-01T10:00:30Z', session_id='archive')
    stats.observe('a', 'office', 'd', offices=['A'], at='2026-01-01T10:00:25Z')
    row = stats.get('a')
    assert row['active_seconds'] == 10
    assert row['first_office_seconds'] == 25
    assert row['first_office_active_quality'] == 'minimum'
    assert ProfileStatistics(stats.path).get('a')['first_office_seconds'] == 25


def test_reconciled_baseline_retains_original_positive_occurrence(tmp_path):
    path = tmp_path / 's.json'; path.write_text(json.dumps({'a': _rich_zero(offices_seen=2)}))
    stats = ProfileStatistics(path); stats.get('a')
    # Simulate a pre-provenance private importer which reset aggregate counters.
    with stats._db() as db:
        base = _rich_zero()
        db.execute('UPDATE baseline SET data=? WHERE profile=?', (json.dumps(base), 'a'))
        db.execute("DELETE FROM metadata WHERE key='baseline_provenance_ready'")
    stats = ProfileStatistics(path)
    stats.observe('a', 'office', 'd', offices=['A'], at='2030-01-01T00:00:00Z')
    assert stats.get('a')['first_office_status'] == 'recorded'


def test_partial_baseline_no_new_event_is_unknown(tmp_path):
    path = tmp_path / 's.json'; path.write_text('{"a":{"checks":9}}')
    row = ProfileStatistics(path).get('a')
    assert row['first_office_status'] == 'unknown'
    assert row['first_booking_status'] == 'unknown'


def test_observed_active_rejects_reversed_range(tmp_path):
    import pytest
    stats = ProfileStatistics(tmp_path / 's.json')
    with pytest.raises(ValueError):
        stats.add_observed_active('a', '2026-01-01T10:00:10Z', '2026-01-01T10:00:00Z')



def test_provenance_upgrade_uses_backup_and_survives_baseline_reconciliation(tmp_path):
    path = tmp_path / 's.json'
    original = json.dumps({'a': _rich_zero(offices_seen=4, booking_screens=2, active_seconds=80,
        first_available_at='2026-01-01T10:00:00+02:00', first_available_seconds=400)})
    path.write_text(original)
    stats = ProfileStatistics(path); stats.get('a')
    with stats._db() as db:
        db.execute('DELETE FROM baseline_provenance')
        db.execute("DELETE FROM metadata WHERE key='baseline_provenance_ready'")
        db.execute('UPDATE baseline SET data=?', (json.dumps(_rich_zero()),))
    # The unchanged backup is authoritative even if the live legacy file changes.
    path.write_text('{"a":0}')
    stats = ProfileStatistics(path)
    stats.observe('a', 'booking', 'd', at='2030-01-01T00:00:00Z')
    stats.add_observed_active('a', '2029-12-31T23:59:40Z', '2030-01-01T00:00:10Z')
    row = stats.get('a')
    assert row['first_booking_status'] == 'recorded'
    assert row['first_booking_seconds'] == 20
    assert row['first_booking_active_quality'] == 'minimum'
    assert row['active_seconds'] == 0
    assert path.with_suffix('.json.bak').read_text() == original


def test_legacy_utc_offsets_do_not_choose_later_generic_first(tmp_path):
    path = tmp_path / 's.json'
    path.write_text(json.dumps({'a': _rich_zero(offices_seen=1, booking_screens=1,
        started_at='2026-01-01T10:00:00+02:00',
        first_available_at='2026-01-01T11:00:00+02:00', first_available_seconds=3600)}))
    stats = ProfileStatistics(path)
    stats.observe('a', 'booking', 'd', at='2026-01-01T09:30:00Z')
    row = stats.get('a')
    assert row['first_available_at'] == '2026-01-01T09:00:00+00:00'
    assert row['started_at'] == '2026-01-01T08:00:00+00:00'
    assert row['first_available_seconds'] == 3600
    assert row['first_booking_status'] == 'recorded'
    assert row['first_office_status'] == 'unknown'


def test_observed_active_clips_today_at_dst_midnight_without_legacy_total(tmp_path):
    path = tmp_path / 's.json'
    path.write_text(json.dumps({'a': _rich_zero(active_seconds=100)}))
    stats = ProfileStatistics(path)
    stats.add_observed_active('a', '2026-03-28T23:59:00+01:00', '2026-03-29T00:02:00+01:00')
    stats.observe('a', 'office', 'd', offices=['A'], at='2026-03-29T00:01:00+01:00')
    row = stats.get('a', 'today', at='2026-03-29T12:00:00+02:00')
    assert row['first_office_seconds'] == 60
    assert row['first_office_active_quality'] == 'minimum'
    assert row['active_seconds'] == 0
    assert row['days'] == 1
    assert stats.get('a')['active_seconds'] == 100


def test_repeated_session_start_does_not_reset_first_or_increment_counters(tmp_path):
    path = tmp_path / 's.json'; path.write_text(json.dumps({'a': _rich_zero()}))
    stats = ProfileStatistics(path)
    stats.start_session('a', 's', {}, at='2030-01-01T10:00:00Z')
    stats.increment('a', at='2030-01-01T10:00:05Z', session_id='s')
    stats.add_active('a', 10, at='2030-01-01T10:00:10Z', session_id='s')
    stats.observe('a', 'office', 'd', offices=['A'], at='2030-01-01T10:00:10Z', session_id='s')
    before = stats.get('a')
    stats.start_session('a', 's', {}, at='2030-01-01T11:00:00Z')
    stats = ProfileStatistics(path)
    stats.start_session('a', 'next', {}, at='2030-01-02T10:00:00Z')
    assert stats.get('a') == before
    assert path.read_text() == path.with_suffix('.json.bak').read_text()



def test_original_legacy_availability_and_missing_fields_are_explicit(tmp_path):
    path = tmp_path / 's.json'
    path.write_text(json.dumps({'a': {'checks': 3, 'started_at': '2026-01-01T08:00:00Z',
        'first_available_at': '2026-01-01T10:00:00Z', 'first_available_seconds': 7200}}))
    stats = ProfileStatistics(path)
    stats.observe('a', 'office', 'archive', offices=['A'], at='2026-01-01T09:00:00Z')
    row = stats.get('a')
    assert row['legacy_first_available_at'] == '2026-01-01T10:00:00+00:00'
    assert row['legacy_first_available_elapsed_seconds'] == 7200
    assert row['first_available_at'] == '2026-01-01T09:00:00+00:00'
    assert row['first_available_seconds'] == 3600
    assert 'booking_screens' in row['legacy_unknown_fields']
    assert 'checks' not in row['legacy_unknown_fields']
    assert row['first_office_active_quality'] == 'unknown'



def test_unambiguous_legacy_first_date_is_known_for_its_kind(tmp_path):
    for kind, counter, other in [('office', 'offices_seen', 'booking_screens'),
                                 ('booking', 'booking_screens', 'offices_seen')]:
        path = tmp_path / (kind + '.json')
        path.write_text(json.dumps({'a': _rich_zero(**{counter: 2, other: 0,
            'active_seconds': 100, 'first_available_at': '2026-01-01T10:00:00Z',
            'first_available_seconds': 3600})}))
        stats = ProfileStatistics(path)
        row = stats.get('a')
        assert row['first_' + kind + '_at'] == '2026-01-01T10:00:00+00:00'
        assert row['first_' + kind + '_status'] == 'known'
        assert row['first_' + kind + '_seconds'] is None
        assert row['first_' + kind + '_active_quality'] == 'unknown'
        # A later detailed event must not replace the justified legacy first.
        stats.observe('a', kind, 'later', offices=['A'], at='2030-01-01T10:00:00Z')
        assert stats.get('a')['first_' + kind + '_at'] == '2026-01-01T10:00:00+00:00'
        stats.add_observed_active('a', '2026-01-01T09:59:30Z', '2026-01-01T10:00:10Z')
        assert stats.get('a')['first_' + kind + '_seconds'] == 30
        assert stats.get('a')['first_' + kind + '_active_quality'] == 'minimum'


def test_earlier_archive_date_stays_recorded_before_unambiguous_legacy_date(tmp_path):
    path = tmp_path / 's.json'
    path.write_text(json.dumps({'a': _rich_zero(offices_seen=2,
        first_available_at='2026-01-01T10:00:00Z', first_available_seconds=3600)}))
    stats = ProfileStatistics(path)
    stats.observe('a', 'office', 'archive', offices=['A'], at='2026-01-01T09:00:00Z')
    row = stats.get('a')
    assert row['first_office_at'] == '2026-01-01T09:00:00+00:00'
    assert row['first_office_status'] == 'recorded'


def test_ambiguous_legacy_first_date_is_not_assigned_to_either_kind(tmp_path):
    path = tmp_path / 's.json'
    path.write_text(json.dumps({'a': _rich_zero(offices_seen=2, booking_screens=1,
        first_available_at='2026-01-01T10:00:00Z', first_available_seconds=3600)}))
    row = ProfileStatistics(path).get('a')
    assert row['first_office_at'] is None and row['first_booking_at'] is None
    assert row['first_office_status'] == row['first_booking_status'] == 'unknown'


def test_semantic_reload_and_changed_evidence(tmp_path):
    stats=ProfileStatistics(tmp_path/'s.json')
    stats.observe_page('a','s','request_no_slots','first',at='2026-01-01T10:00:00Z')
    stats.observe_page('a','s','request_no_slots','reload',at='2026-01-01T10:00:20Z')
    assert len([r for r in stats.history('a') if r['kind']=='page'])==1
    stats.observe_page('a','s','request_no_slots','reload',at='2026-01-01T10:00:25Z',evidence=('operation_code','no_appointments'))
    assert len([r for r in stats.history('a') if r['kind']=='page'])==2


def test_live_phase_transition_preserves_actual_page_last_seen(tmp_path):
    stats=ProfileStatistics(tmp_path/'s.json')
    stats.phase('a','s','working',at='2026-01-01T10:00:00Z')
    stats.observe_page('a','s','server_error','first',at='2026-01-01T10:00:00Z')
    stats.heartbeat('a','s',at='2026-01-01T10:00:30Z')
    stats.heartbeat('a','s',at='2026-01-01T10:01:00Z')
    assert next(r for r in stats.history('a') if r['kind']=='page')['last_seen']=='2026-01-01T10:00:00+00:00'
    stats.observe_page('a','s','province','next',at='2026-01-01T10:01:10Z')
    page=next(r for r in stats.history('a') if r['kind']=='page' and r['stage']=='server_error')
    assert page['end']=='2026-01-01T10:01:10+00:00'
    assert page['last_seen']=='2026-01-01T10:00:00+00:00'


def test_timeline_history_uses_session_context_without_changing_raw_facts(tmp_path):
    stats=ProfileStatistics(tmp_path/'s.json')
    stats.start_session('p','s',{},at='2026-01-01T10:00:00Z')
    stats.observe_page('p','s','request_no_slots','doc',at='2026-01-01T10:00:00Z')
    stats.observe_page('p','s','server_error','bad',at='2026-01-01T10:10:00Z')
    stats.end_session('p','s','stopped',at='2026-01-01T10:20:00Z')
    raw=stats.history('p');total=stats.total('p')
    clips=stats.timeline_history('p',start='2026-01-01T10:05:00Z',end='2026-01-01T10:15:00Z')
    assert any(r.get('timeline_state')=='search' and r['start']<='2026-01-01T10:05:00+00:00' for r in clips)
    assert any(r.get('timeline_state')=='recovery' for r in clips)
    assert stats.history('p')==raw and stats.total('p')==total


def test_replayed_event_does_not_create_another_activity_day(tmp_path):
    stats = ProfileStatistics(tmp_path / 's.json')
    stats.event('a', 'check', 'same-document', session_id='s', at='2026-01-01T12:00:00Z')
    stats.event('a', 'check', 'same-document', session_id='s', at='2026-01-02T12:00:00Z')
    assert stats.total('a') == 1
    assert len(stats.history('a')) == 1
    assert stats.get('a')['days'] == 1
    assert stats.get('a', 'today', at='2026-01-02T12:00:00Z')['days'] == 0
    # A genuinely new event on the second day must still record that day.
    stats.event('a', 'check', 'next-document', session_id='s', at='2026-01-02T12:01:00Z')
    assert stats.total('a') == 2
    assert stats.get('a')['days'] == 2


def test_booking_details_enrich_original_event_without_recount_or_retiming(tmp_path):
    stats=ProfileStatistics(tmp_path/'s.json')
    first='2026-10-27T10:00:00Z'
    stats.event('p','booking','doc',at=first,session_id='s')
    details={'office':'CNP TEST, CALLE TEST, 1','offers':[{'date':'2026-10-28','time':'12:30','token':'SECRET'}],
             'captcha':'SECRET','confirmation':'SECRET','document_number':'SECRET'}
    for _ in range(2):stats.save_booking_details('p','s','doc',details)
    reopened=ProfileStatistics(stats.path);rows=[row for row in reopened.history('p') if row['kind']=='booking']
    assert len(rows)==1 and reopened.get('p')['booking_screens']==1
    assert rows[0]['start']=='2026-10-27T10:00:00+00:00'
    assert rows[0]['evidence']['booking_details']=={'office':'CNP TEST, CALLE TEST, 1','offers':[{'date':'2026-10-28','time':'12:30'}]}
    assert 'SECRET' not in str(rows)
    assert reopened.get('p','today',at='2026-10-28T15:00:00Z')['booking_screens']==0


def test_booking_details_never_create_missing_or_wrong_session_event(tmp_path):
    stats=ProfileStatistics(tmp_path/'s.json');stats.event('p','booking','doc',session_id='real')
    details={'office':'CNP TEST','offers':[{'date':'2026-10-28','time':'12:30'}]}
    assert not stats.save_booking_details('p','other','doc',details)
    assert not stats.save_booking_details('p','real','other-document',details)
    assert stats.get('p')['booking_screens']==1
    assert 'booking_details' not in stats.history('p')[0]['evidence']
