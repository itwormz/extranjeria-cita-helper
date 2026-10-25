from concurrent.futures import ThreadPoolExecutor
from app.assist_control import AssistControl
from app.desktop.statistics import ProfileStatistics


def test_totals_survive_sessions_and_are_separate_per_profile(tmp_path):
    path = tmp_path / 'statistics.json'
    stats = ProfileStatistics(path)
    first = AssistControl()
    first.set_check_counter(stats.total('a'), lambda: stats.increment('a'))
    first.availability_checked()
    first.availability_checked()
    assert first.snapshot()['checks'] == 2
    second = AssistControl()
    second.set_check_counter(ProfileStatistics(path).total('a'), lambda: stats.increment('a'))
    second.availability_checked()
    assert second.snapshot()['checks'] == 1
    assert second.snapshot()['profile_checks'] == 3
    assert stats.total('b') == 0


def test_independent_writers_do_not_lose_checks(tmp_path):
    path = tmp_path / 'statistics.json'
    with ThreadPoolExecutor(max_workers=4) as pool:
        list(pool.map(lambda _: ProfileStatistics(path).increment('test'), range(20)))
    assert ProfileStatistics(path).total('test') == 20


def test_lifetime_migration_and_observation_dedup(tmp_path):
    import json
    path = tmp_path / 'statistics.json'
    path.write_text(json.dumps({'a': 7}))
    stats = ProfileStatistics(path)
    assert stats.get('a')['checks'] == 7
    stats.observe('a', 'province', 'start', at='2026-01-01T00:00:00+00:00')
    stats.add_active('a', 12)
    first = stats.observe('a', 'office', 'doc', offices=['Madrid: Office A'], at='2026-01-01T00:01:00+00:00')
    stats.observe('a', 'office', 'doc', offices=['Madrid: Office A'])
    stats.observe('a', 'office', 'doc2', offices=['Madrid: Office A', 'Toledo: Office A'])
    assert first['first_available_seconds'] == 60
    assert stats.get('a')['offices_seen'] == 2
    assert stats.get('a')['unique_offices'] == 2
    assert stats.get('b')['offices_seen'] == 0
    stats.observe('a', 'booking', 'booking')
    stats.observe('a', 'booking', 'booking')
    assert stats.get('a')['booking_screens'] == 1


def test_control_statistics_failures_are_nonfatal():
    control = AssistControl()
    def broken(*args, **kwargs):
        raise OSError('synthetic')
    control.set_statistics_sink(broken, broken)
    control.observe_statistics('booking', 'doc')
    control.statistics_tick(10)
    control.statistics_tick(10.5)


def test_active_ticker_excludes_manual_and_sleep():
    control = AssistControl()
    seconds = []
    control.set_statistics_sink(lambda *a, **k: None, seconds.append)
    control.statistics_tick(10)
    control.statistics_tick(10.5)
    control.statistics_tick(100)
    control.publish(screen='', evidence='', next_action='', mode='manual')
    control.statistics_tick(100.5)
    control.publish(screen='', evidence='', next_action='', mode='working')
    control.statistics_tick(101)
    control.statistics_tick(101.5)
    control.flush_statistics()
    assert sum(seconds) == 1


def test_independent_writers_deduplicate_observations(tmp_path):
    path = tmp_path / 'statistics.json'
    with ThreadPoolExecutor(max_workers=4) as pool:
        list(pool.map(lambda _: ProfileStatistics(path).observe('a', 'rate_limited', 'one'), range(20)))
    row = ProfileStatistics(path).get('a')
    assert row['rate_limited'] == 1
    assert row['blocked'] == row['server_errors'] == 0


def test_corrupt_legacy_statistics_are_not_overwritten(tmp_path):
    import pytest
    path = tmp_path / 'statistics.json'
    path.write_text('{"a": -1}')
    with pytest.raises(ValueError):
        ProfileStatistics(path).increment('a')
    assert path.read_text() == '{"a": -1}'


def test_observations_remain_deduplicated_beyond_old_cache_limit(tmp_path):
    path = tmp_path / 'statistics.json'
    stats = ProfileStatistics(path)
    stats.observe('a', 'blocked', 'one')
    for index in range(1030):
        stats.observe('a', 'blocked', str(index))
    stats.observe('a', 'blocked', 'one')
    assert stats.get('a')['blocked'] == 1031
    assert not path.exists()  # SQLite is the sole new-data writer.


def test_active_time_batches_and_flushes_on_pause():
    control = AssistControl()
    writes = []
    control.set_statistics_sink(lambda *a, **k: None, writes.append)
    for index in range(10):
        control.statistics_tick(index * .5)
    assert writes == []
    control.set_paused(True)
    assert writes == [4.5]
    control.statistics_tick(10)
    control.statistics_tick(10.5)
    control.flush_statistics()
    assert writes == [4.5]
