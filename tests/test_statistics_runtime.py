"""History is independent of optional page diagnostics."""
import pytest

from app.assist_control import AssistControl


def history_control():
    control = AssistControl()
    rows = []
    control.set_history_sink(lambda kind, **fields: rows.append((kind, fields)))
    control.set_diagnostics(False)
    return control, rows


def test_page_history_survives_disabled_diagnostics_without_raw_page_content():
    control, rows = history_control()
    control.observe_visit(document='ephemeral-id', stage='request_no_slots_restart',
                          page='https://example.test/?token=SECRET', title='PRIVATE TEXT')
    kind, fields = rows[-1]
    assert kind == 'page'
    assert fields['stage'] == 'request_no_slots_restart'
    assert fields['document'] == 'ephemeral-id'
    assert 'SECRET' not in str(rows)
    assert 'PRIVATE TEXT' not in str(rows)


def test_action_history_does_not_store_detail_strings_or_urls():
    control, rows = history_control()
    control.record('action', stage='applicant', document='document',
                   page='https://example.test/SECRET', detail='PERSONAL SECRET')
    assert rows[-1][0] == 'action'
    assert rows[-1][1]['stage'] == 'applicant'
    assert 'SECRET' not in str(rows)


def test_history_records_mode_changes_without_timer_spam():
    control, rows = history_control()
    for remaining in range(180, 170, -1):
        control.publish(screen='Private text', evidence='SECRET', next_action='SECRET',
                        mode='waiting', remaining_seconds=remaining)
    phases = [r for r in rows if r[0] == 'phase']
    assert len(phases) == 1
    assert phases[0][1]['mode'] == 'waiting'
    assert 'SECRET' not in str(rows)
    control.set_paused(True)
    assert rows[-1][0] == 'phase' and rows[-1][1]['mode'] == 'paused'


def test_storage_error_is_visible_and_does_not_abort_search():
    control = AssistControl()
    def broken(*args, **kwargs):
        raise OSError('contains private location')
    control.set_history_sink(broken)
    control.record('action', stage='province')
    assert control.snapshot()['statistics_error'] is True
    assert 'private location' not in str(control.snapshot())
    control.set_history_sink(lambda *a, **k: None)
    control.record('action', stage='province')
    assert control.snapshot()['statistics_error'] is True


def test_heartbeat_is_batched_and_sleep_creates_gap():
    control, rows = history_control()
    control.statistics_tick(10)
    for i in range(1, 40):
        control.statistics_tick(10 + i * .5)
    assert len([r for r in rows if r[0] == 'heartbeat']) == 1
    control.statistics_tick(100)
    assert any(kind == 'gap' for kind, _ in rows)
    assert control.snapshot()['statistics_error'] is False


def test_finish_and_manual_handoff_are_typed_events():
    control, rows = history_control()
    control.booking_handoff = True
    control.publish(screen='Booking', evidence='', next_action='', mode='booking_handoff')
    control.finish('SECRET reason')
    assert any(k == 'phase' and f['mode'] == 'booking_handoff' for k, f in rows)
    assert any(k == 'phase' and f['mode'] == 'finished' for k, f in rows)
    assert 'SECRET' not in str(rows)


def test_worker_session_writes_real_history_and_counts_once(tmp_path):
    from types import SimpleNamespace
    from app.desktop.statistics import ProfileStatistics
    from app.desktop.statistics_session import StatisticsSession
    stats = ProfileStatistics(tmp_path / 'statistics.json')
    task = SimpleNamespace(id='one', province='Barcelona', procedure='POLICIA - TOMA DE HUELLA',
                           access_mode='certificate', clave_method='movil',
                           document_number='PRIVATE-NIE', full_name='PRIVATE-NAME')
    control = AssistControl()
    session = StatisticsSession(stats, task, control)
    control.set_diagnostics(False)
    control.observe_visit(document='doc', stage='office', page='https://site.test/SECRET', title='SECRET')
    control.observe_statistics('office', 'doc', offices=['Barcelona: Office A, Street A'])
    control.observe_statistics('office', 'doc', offices=['Barcelona: Office A, Street A'])
    control.availability_checked()
    control.response_observed(503)
    control.publish(screen='', evidence='', next_action='', mode='waiting')
    session.close('stopped')
    assert stats.get('one')['offices_seen'] == 1
    assert stats.get('one')['checks'] == 1
    assert len(stats.office_history('one')[0]['appearances']) == 1
    rows = stats.history('one')
    assert any(r['stage'] == 'office' for r in rows)
    assert 'PRIVATE-' not in str(rows)
    assert 'SECRET' not in str(rows)


def test_storage_failure_at_start_is_visible_but_not_fatal(tmp_path):
    from types import SimpleNamespace
    from app.desktop.statistics_session import StatisticsSession
    class Broken:
        def start_session(self, *args, **kwargs):raise OSError('private source')
    control = AssistControl()
    task = SimpleNamespace(id='one', province='Barcelona', procedure='service', access_mode='certificate')
    session = StatisticsSession(Broken(), task, control)
    assert control.snapshot()['statistics_error']
    session.close('stopped')


def test_active_flush_after_sleep_keeps_original_day():
    control=AssistControl();writes=[]
    control.set_statistics_sink(lambda *a,**k:None,lambda s:None,
        add_active_at=lambda seconds,at:writes.append((seconds,at)))
    control.statistics_tick(10,wall_at='2026-01-01T22:59:58Z')
    control.statistics_tick(10.5,wall_at='2026-01-01T22:59:58.5Z')
    control.statistics_tick(100,wall_at='2026-01-02T00:30:00Z')
    control.flush_statistics()
    assert writes==[(.5,'2026-01-01T22:59:58.5Z')]


def test_first_province_is_not_a_repeat_pass(tmp_path):
    from types import SimpleNamespace
    from app.desktop.statistics import ProfileStatistics
    from app.desktop.statistics_session import StatisticsSession
    stats=ProfileStatistics(tmp_path/'s.json');control=AssistControl()
    task=SimpleNamespace(id='a',province='Barcelona',procedure='huellas',access_mode='certificate')
    session=StatisticsSession(stats,task,control)
    control.observe_statistics('pass','initial')
    assert stats.get('a')['flow_passes']==0
    control.observe_statistics('pass','second')
    control.observe_statistics('pass','second')
    assert stats.get('a')['flow_passes']==1
    session.close('stopped')


def test_heartbeat_confirms_bot_phase_without_extending_page(tmp_path):
    from types import SimpleNamespace
    from app.desktop.statistics import ProfileStatistics
    from app.desktop.statistics_session import StatisticsSession
    stats=ProfileStatistics(tmp_path/'s.json');control=AssistControl()
    task=SimpleNamespace(id='a',province='Barcelona',procedure='huellas',access_mode='certificate')
    session=StatisticsSession(stats,task,control)
    stats.phase('a',session.session_id,'paused',at='2026-01-01T10:00:00Z')
    stats.observe_page('a',session.session_id,'office','doc',at='2026-01-01T10:00:00Z')
    session.record('heartbeat',mode='paused',at='2026-01-01T10:00:30Z')
    rows=stats.history('a')
    assert next(r for r in rows if r['kind']=='phase')['last_seen']=='2026-01-01T10:00:30+00:00'
    assert next(r for r in rows if r['kind']=='page')['last_seen']=='2026-01-01T10:00:00+00:00'
    session.close('stopped')


@pytest.mark.parametrize('resume_monotonic', [3600, 2])
def test_sleep_gap_covers_missing_worker_time_in_projected_history(tmp_path, monkeypatch, resume_monotonic):
    from types import SimpleNamespace
    from app.desktop import statistics as module
    from app.desktop.statistics_session import StatisticsSession
    stamp = module._stamp
    clock = ['2026-01-01T10:00:00Z']
    monkeypatch.setattr(module, '_stamp', lambda value=None: stamp(clock[0] if value is None else value))
    stats = module.ProfileStatistics(tmp_path/'s.json')
    control = AssistControl()
    task = SimpleNamespace(id='a', province='Barcelona', procedure='huellas', access_mode='certificate')
    session = StatisticsSession(stats, task, control)
    control.publish(screen='', evidence='', next_action='', mode='working')
    control.observe_visit(document='doc', stage='expired', page='', title='')
    control.statistics_tick(now=0, wall_at=clock[0])
    clock[0] = '2026-01-01T10:00:01Z'
    control.statistics_tick(now=1, wall_at=clock[0])
    clock[0] = '2026-01-01T11:00:00Z'
    control.statistics_tick(now=resume_monotonic, wall_at=clock[0])
    control.flush_statistics()
    assert stats.get('a')['active_seconds'] == 1
    clips = [row for row in stats.timeline_history('a') if row['kind']=='clip']
    gaps = [row for row in clips if row['timeline_state']=='gap']
    assert [(row['start'], row['end']) for row in gaps] == [
        ('2026-01-01T10:00:01+00:00', '2026-01-01T11:00:00+00:00')]
    assert not any(row['timeline_state']!='gap' and row['start'] < gaps[0]['end']
                   and row['end'] > gaps[0]['start'] for row in clips)
    session.close('stopped')


def test_failed_active_write_keeps_separate_timestamps_after_pause():
    control=AssistControl();writes=[];broken=True
    def write(seconds,at):
        if broken:raise OSError('synthetic')
        writes.append((seconds,at))
    control.set_statistics_sink(lambda *a,**k:None,lambda s:None,add_active_at=write)
    control.statistics_tick(1,wall_at='2026-01-01T22:59:59Z')
    control.statistics_tick(1.5,wall_at='2026-01-01T22:59:59.5Z')
    control.set_paused(True)
    control.statistics_tick(10,wall_at='2026-01-02T00:00:00Z')
    control.set_paused(False)
    control.statistics_tick(10.5,wall_at='2026-01-02T00:00:00.5Z')
    broken=False
    control.statistics_tick(11,wall_at='2026-01-02T00:00:01Z');control.flush_statistics()
    assert writes==[(.5,'2026-01-01T22:59:59.5Z'),(.5,'2026-01-02T00:00:01Z')]


def test_restart_history_retains_safe_reason_and_initiator(tmp_path):
    from types import SimpleNamespace
    from app.desktop.statistics import ProfileStatistics
    from app.desktop.statistics_session import StatisticsSession
    stats=ProfileStatistics(tmp_path/'s.json');control=AssistControl()
    task=SimpleNamespace(id='a',province='Barcelona',procedure='huellas',access_mode='certificate')
    session=StatisticsSession(stats,task,control)
    control.observe_visit(document='old',stage='expired',page='',title='')
    control.record('flow_restart',stage='province',detail='SECRET')
    event=next(r for r in stats.history('a') if r['kind']=='flow_restart')
    assert event['evidence']['previous_stage']=='expired'
    assert event['evidence']['initiator']=='automatic'
    assert 'SECRET' not in str(event)
    session.close('stopped')


def test_same_error_across_retries_is_one_episode_until_healthy_page(tmp_path):
    from types import SimpleNamespace
    from app.desktop.statistics import ProfileStatistics
    from app.desktop.statistics_session import StatisticsSession
    stats=ProfileStatistics(tmp_path/'s.json');control=AssistControl()
    task=SimpleNamespace(id='a',province='Barcelona',procedure='huellas',access_mode='certificate')
    session=StatisticsSession(stats,task,control)
    control.observe_statistics('server_error','first-response')
    control.observe_visit(document='first-response',stage='server_error',page='',title='')
    control.observe_statistics('server_error','retried-response')
    assert stats.get('a')['server_errors']==1
    control.observe_visit(document='healthy',stage='province',page='',title='')
    control.observe_statistics('server_error','new-failure')
    assert stats.get('a')['server_errors']==2
    session.close('stopped')


def test_short_scheduler_stall_excludes_active_time_without_page_gap():
    control=AssistControl();history=[];active=[]
    control.set_history_sink(lambda kind,**fields:history.append((kind,fields)))
    control.set_statistics_sink(lambda *a,**k:None,active.append)
    control.statistics_tick(0)
    control.statistics_tick(.5)
    control.statistics_tick(4)
    control.flush_statistics()
    assert not any(kind=='gap' for kind,_ in history)
    assert active==[.5]
    control.statistics_tick(40)
    assert sum(kind=='gap' for kind,_ in history)==1


@pytest.mark.parametrize('before,after,expected', [
    ('2026-10-25T02:59:59+02:00', '2026-10-25T02:00:00+01:00', 1),
    ('2026-03-29T01:59:59+01:00', '2026-03-29T03:00:00+02:00', 1),
    ('2026-01-01T10:00:00Z', '2026-01-01T09:00:00Z', 0),
])
def test_wall_clock_offsets_and_backward_adjustment_are_safe(before, after, expected):
    control, history = history_control()
    active = []
    control.set_statistics_sink(lambda *a, **k: None, active.append)
    control.statistics_tick(now=0, wall_at=before)
    control.statistics_tick(now=1, wall_at=after)
    control.flush_statistics()
    assert sum(active) == expected
    assert not any(kind=='gap' for kind, _ in history)


def test_loading_history_has_loading_recognition():
    from app import assist_flow
    from types import SimpleNamespace
    control=AssistControl();history=[]
    control.set_history_sink(lambda kind,**fields:history.append((kind,fields)))
    runner=assist_flow.AssistRunner(SimpleNamespace(url='https://example.com/'),SimpleNamespace(),SimpleNamespace(),SimpleNamespace(id='test'),'run',control,SimpleNamespace(),SimpleNamespace())
    runner.publish(assist_flow.Observed('loading','loading','loading',None))
    assert next(fields for kind,fields in history if kind=='page')['recognition']=='loading'


def test_booking_details_link_to_counter_and_journal_without_diagnostics(tmp_path):
    from types import SimpleNamespace
    from app.desktop.statistics import ProfileStatistics
    from app.desktop.statistics_session import StatisticsSession
    from app.assist_journal import description
    stats=ProfileStatistics(tmp_path/'s.json');control=AssistControl();control.set_diagnostics(False)
    task=SimpleNamespace(id='p',province='Barcelona',procedure='huellas',access_mode='certificate')
    session=StatisticsSession(stats,task,control)
    control.observe_statistics('booking','doc')
    control.observe_visit(document='doc',stage='booking',page='https://example.test',title='booking',automated=False)
    details={'office':'CNP TEST','offers':[{'date':'2026-10-28','time':'12:30'}],'confirmation':'PRIVATE'}
    control.set_booking_details(details,document='doc');control.set_booking_details(details,document='doc')
    session.close('stopped')
    row=next(r for r in ProfileStatistics(stats.path).history('p') if r['kind']=='booking')
    assert row['evidence']['booking_details']=={'office':'CNP TEST','offers':[{'date':'2026-10-28','time':'12:30'}]}
    assert stats.get('p')['booking_screens']==1
    assert '28.10.2026 — 12:30' in description(control.snapshot()['journal'][0])
    assert 'PRIVATE' not in str(row)


def test_session_context_retains_both_independent_search_choices(tmp_path):
    import json
    from types import SimpleNamespace
    from app.desktop.statistics import ProfileStatistics
    from app.desktop.statistics_session import StatisticsSession
    stats=ProfileStatistics(tmp_path/'s.json');control=AssistControl()
    task=SimpleNamespace(id='p',province='Barcelona',procedure='huellas',access_mode='certificate',no_slots_mode='reload',applicant_no_slots_mode='continue')
    session=StatisticsSession(stats,task,control)
    with stats._db(write=False) as db:context=json.loads(db.execute('SELECT context FROM sessions').fetchone()[0])
    assert context['no_slots_mode']=='reload' and context['applicant_no_slots_mode']=='continue'
    session.close('stopped')
