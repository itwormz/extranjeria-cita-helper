"""Pure serial projection regressions; no real profiles or captures."""
from app.desktop.statistics_timeline import build_timeline


def at(second):
    return f"2026-01-01T10:{second//60:02}:{second%60:02}+00:00"


def page(second, stage, end=None, **extra):
    return dict(kind='page', stage=stage, start=at(second), end=at(second if end is None else end), last_seen=at(second if end is None else end), session_id='s', event=False, confirmed_end=False, evidence=[], **extra)


def phase(second, mode, end=None):
    return dict(kind='phase', mode=mode, start=at(second), end=at(second if end is None else end), last_seen=at(second if end is None else end), session_id='s', event=False)


def event(second, kind, **extra):
    return dict(kind=kind, start=at(second), end=at(second), last_seen=at(second), session_id='s', event=True, **extra)


def session(end=100, reason='stopped'):
    return [dict(session='s', start=at(0), end=at(end), last_seen=at(end), reason=reason)]


def clips(rows):
    return [r for r in rows if r['kind']=='clip']


def test_forms_and_reload_become_one_search_clip_with_restored_provenance():
    raw=[page(0,'province',2),page(10,'procedure',12),page(20,'applicant',22),page(30,'request_no_slots',32),page(40,'request_no_slots',42)]
    result=clips(build_timeline(raw,session()))
    assert len(result)==1
    assert (result[0]['start'],result[0]['end'],result[0]['timeline_state'])==(at(0),at(100),'search')
    assert result[0]['restored'] and result[0]['restored_ranges']
    assert result[0]['source_rows']==raw
    assert raw[0]['end']==at(2)


def test_later_state_supersedes_stale_overlap_and_cod_applies_to_all_no_slots():
    raw=[page(0,'request_no_slots',90),page(20,'expired',21),page(40,'loading',41),page(60,'post_contact_no_slots',61)]
    raw[-1]['evidence']=['operation_code','no_appointments']
    result=clips(build_timeline(raw,session()))
    assert [(r['start'],r['end'],r['timeline_state']) for r in result]==[(at(0),at(20),'search'),(at(20),at(60),'anomaly'),(at(60),at(100),'cod')]


def test_manual_override_and_explicit_gap_cannot_be_filled():
    raw=[page(0,'expired',5),phase(10,'paused',20),phase(20,'working',25),phase(30,'unobserved',40),phase(40,'working',45),page(45,'province',46)]
    result=clips(build_timeline(raw,session(60)))
    assert [(r['start'],r['end'],r['timeline_state']) for r in result]==[(at(0),at(10),'anomaly'),(at(10),at(20),'manual'),(at(20),at(30),'anomaly'),(at(30),at(40),'gap'),(at(40),at(60),'search')]
    assert all(not (r['start']<at(30) and r['end']>at(40)) for r in result if r['timeline_state']!='gap')


def test_no_session_keeps_supported_spans_and_unknown_end_has_no_tail():
    raw=[page(0,'province',2),page(20,'request_no_slots',22)]
    result=clips(build_timeline(raw,[]))
    assert [(r['start'],r['end']) for r in result]==[(at(0),at(2)),(at(20),at(22))]
    assert not any(r['restored'] for r in result)
    result=clips(build_timeline(raw,session(100,'last_observation')))
    assert result[-1]['end']==at(22)


def test_availability_combines_office_and_booking_at_first_office_and_resets():
    raw=[page(0,'province',1),event(10,'office',offices=['Fictitious office']),page(12,'office',13),page(15,'contact',16),event(20,'booking'),page(30,'province',31),event(40,'office',offices=['Another fictitious office'])]
    result=build_timeline(raw,session(50))
    available=[r for r in result if r['kind']=='availability']
    assert len(available)==2
    first=available[0]
    assert (first['start'],first['availability_kind'],first['office_seen_at'],first['booking_seen_at'])==(at(10),'booking',at(10),at(20))
    assert first['offices']==['Fictitious office']
    assert len(first['source_rows'])==2
    assert any(r['kind']=='clip' and r['end']==at(10) for r in result)
    assert not any(r['kind'] in {'office','booking'} for r in result)


def test_browser_generation_and_session_boundaries_do_not_bridge():
    raw=[page(0,'province',1),event(5,'browser_session_started',evidence={'browser_generation':1}),event(10,'browser_session_started',evidence={'browser_generation':2}),page(20,'request_menu',21)]
    result=build_timeline(raw,session(30))
    assert [r['start'] for r in result if r.get('timeline_state')=='restart']==[at(10)]
    assert any(r['kind']=='clip' and r['end']==at(10) for r in result)
    later=page(50,'expired',51);later['session_id']='later'
    result=clips(build_timeline(raw+[later],session(30)+[dict(session='later',start=at(40),end=at(60),last_seen=at(60),reason='stopped')]))
    assert all(not (r['start']<at(40) and r['end']>at(30)) for r in result)


def test_response_only_http_event_does_not_become_rendered_error_state():
    raw=[page(0,'request_no_slots',1),event(10,'http_response',evidence={'http_status':500})]
    result=clips(build_timeline(raw,session(20)))
    assert len(result)==1 and result[0]['timeline_state']=='search'


def test_initial_phase_can_supply_coarse_search_without_invented_page():
    result=clips(build_timeline([phase(0,'working',10),page(20,'expired',21)],session(30)))
    assert [(r['start'],r['end'],r['timeline_state']) for r in result]==[(at(0),at(20),'search'),(at(20),at(30),'anomaly')]
    assert result[0]['source_rows'][0]['kind']=='phase'


def test_event_split_retains_actual_last_seen_and_live_id_is_stable():
    raw=[page(0,'request_no_slots',1),event(10,'office',offices=['Fictitious office'])]
    first=build_timeline(raw,session(20))
    second=build_timeline(raw+[event(15,'booking')],session(30))
    left=clips(first);right=clips(second)
    assert left[1]['last_seen']==at(1)
    assert [r['id'] for r in left]==[r['id'] for r in right]
    assert next(r for r in first if r['kind']=='availability')['id']==next(r for r in second if r['kind']=='availability')['id']


def test_isolated_pages_without_session_survive_as_points():
    result=clips(build_timeline([page(0,'unknown'),page(20,'province')],[]))
    assert [(r['start'],r['end'],r['timeline_state']) for r in result]==[(at(0),at(0),'recovery'),(at(20),at(20),'search')]


def test_checks_are_counted_once_at_clip_start_not_previous_clip_end():
    raw=[page(0,'province',1),page(10,'expired',11),event(5,'check'),event(10,'check')]
    result=clips(build_timeline(raw,session(20)))
    assert [r['checks'] for r in result]==[1,1]


def test_search_between_office_results_creates_independent_availability():
    raw=[event(0,'office'),page(10,'request_no_slots',11),event(20,'office'),event(30,'booking')]
    result=[r for r in build_timeline(raw,session(40)) if r['kind']=='availability']
    assert len(result)==2
    assert [r['availability_kind'] for r in result]==['office','booking']


def test_closed_manual_phase_cannot_hide_later_actual_page():
    raw=[page(0,'province',1),phase(5,'manual',10),page(20,'request_no_slots',21)]
    result=clips(build_timeline(raw,session(30)))
    assert [(r['start'],r['end'],r['timeline_state']) for r in result]==[(at(0),at(5),'search'),(at(5),at(20),'manual'),(at(20),at(30),'search')]


def test_overlapping_run_tails_are_capped_by_next_session_start():
    later=page(30,'expired',31);later['session_id']='later'
    result=clips(build_timeline([page(0,'province',1),later],session(100)+[dict(session='later',start=at(20),end=at(50),last_seen=at(50),reason='stopped')]))
    assert [(r['start'],r['end']) for r in result]==[(at(0),at(20)),(at(30),at(50))]
    assert result[0]['source_overlap'] and result[0]['restored']
    assert result[0]['original_source_end']==at(100)


def test_typed_cod_wait_phase_keeps_supported_purple_boundaries():
    wait=phase(10,'waiting',30);wait['stage']='cod_oper'
    result=clips(build_timeline([page(0,'request_no_slots',1),wait,page(25,'province',26)],session(40)))
    assert [(r['start'],r['end'],r['timeline_state']) for r in result]==[(at(0),at(10),'search'),(at(10),at(25),'cod'),(at(25),at(40),'search')]
    assert not result[1]['restored']


def test_explicit_gap_ends_availability_episode():
    raw=[event(0,'office'),phase(10,'unobserved',20),phase(20,'working',21),event(30,'office'),event(40,'booking')]
    markers=[r for r in build_timeline(raw,session(50)) if r['kind']=='availability']
    assert len(markers)==2
    assert [r['availability_kind'] for r in markers]==['office','booking']


def test_overlap_cap_does_not_promote_availability_from_discarded_tail():
    later=page(30,'province',31);later['session_id']='later'
    raw=[page(0,'province',1),event(10,'office'),event(40,'booking'),later]
    result=build_timeline(raw,session(60)+[dict(session='later',start=at(20),end=at(50),last_seen=at(50),reason='stopped')])
    marker=next(r for r in result if r['kind']=='availability')
    assert marker['availability_kind']=='office'
    assert marker['booking_seen_at'] is None


def test_optional_cache_preserves_facts_and_invalidates_changed_run_and_cap():
    from copy import deepcopy
    raw=[page(0,'province',1)];runs=session(30);before=deepcopy((raw,runs));cache={}
    first=build_timeline(raw,runs,cache=cache)
    second=build_timeline(raw,runs,cache=cache)
    assert first==second==build_timeline(raw,runs)
    assert first[0] is second[0]
    assert (raw,runs)==before
    changed=build_timeline(raw,session(40),cache=cache)
    assert clips(changed)[0]['end']==at(40)
    later=page(25,'expired',26);later['session_id']='later'
    capped=build_timeline(raw+[later],session(40)+[dict(session='later',start=at(20),end=at(30),last_seen=at(30),reason='stopped')],cache=cache)
    assert clips(capped)[0]['end']==at(20)
    changed_page=page(10,'expired',11)
    updated=clips(build_timeline(raw+[changed_page],runs,cache=cache))
    assert [r['timeline_state'] for r in updated]==['search','anomaly']


def test_historical_short_unknown_navigation_coalesces_without_hiding_facts():
    from copy import deepcopy
    raw=[page(0,'province',1),page(10,'unknown',11),page(12,'loading',13),page(14,'procedure',15)]
    for row in raw:row['session_id']='archive-test'
    runs=[dict(session='archive-test',start=at(0),end=at(30),last_seen=at(30),reason='stopped')]
    before=deepcopy(raw)
    result=clips(build_timeline(raw,runs))
    assert len(result)==1 and result[0]['timeline_state']=='search'
    assert result[0]['source_rows']==raw
    assert 'unknown' in result[0]['page_stages'] and 'loading' in result[0]['page_stages']
    assert result[0]['restored_reason']=='historical_navigation_transient'
    assert any(left<=at(10) and right>=at(14) for left,right in result[0]['restored_ranges'])
    assert raw==before
    live=[dict(row,session_id='s') for row in raw]
    assert any(r['timeline_state']=='recovery' for r in clips(build_timeline(live,session(30))))


def test_historical_unknown_is_not_suppressed_when_attention_or_long_or_open():
    base=[page(0,'province',1),page(10,'unknown',11),page(14,'procedure',15)]
    runs=session(30);runs[0]['context']={'app_version':'historical'}
    obstacles=[event(12,'timeout'),event(12,'network_error'),event(12,'recovery'),event(12,'manual_required'),phase(9,'manual',13),phase(9,'recovery',13),phase(12,'unobserved',13)]
    for obstacle in obstacles:
        result=clips(build_timeline(base+[obstacle],runs))
        assert any(r['timeline_state'] in {'recovery','gap'} for r in result), obstacle
    for tail in ([page(16,'procedure',17)],[]):
        assert any(r['timeline_state']=='recovery' for r in clips(build_timeline(base[:2]+tail,runs)))
    explicit=[page(0,'province',1),page(10,'server_error',11),page(14,'procedure',15)]
    assert any(r['timeline_state']=='recovery' for r in clips(build_timeline(explicit,runs)))


def test_error_page_keeps_its_colour_during_manual_attention_but_pause_wins():
    for stage,state in [('expired','anomaly'),('blocked','anomaly'),('server_error','recovery'),('rate_limited','recovery'),('unknown','recovery')]:
        raw=[page(0,stage,1),phase(2,'manual',10)]
        result=clips(build_timeline(raw,session(20)))
        assert all(r['timeline_state']==state for r in result), stage
        paused=clips(build_timeline(raw+[phase(12,'paused',15)],session(20)))
        assert paused[-1]['timeline_state']=='manual'


def test_open_run_shows_page_and_availability_newer_than_heartbeat():
    raw=[page(0,'province',1),page(45,'expired',46),event(50,'office')]
    runs=[dict(session='s',start=at(0),end=None,last_seen=at(30),reason='')]
    result=build_timeline(raw,runs)
    assert any(r['kind']=='clip' and r['stage']=='expired' and r['start']==at(45) for r in result)
    assert next(r for r in result if r['kind']=='availability')['start']==at(50)
    assert clips(result)[-1]['end']==at(50)


def test_live_working_mode_keeps_short_pending_navigation_normal():
    from copy import deepcopy
    raw=[phase(0,'working',20),page(0,'province',10),page(10,'unknown',12),page(14,'procedure',20)]
    before=deepcopy(raw)
    result=clips(build_timeline(raw,session(20)))
    assert len(result)==1 and result[0]['timeline_state']=='search'
    assert not result[0]['restored']
    assert 'restored_reason' not in result[0]
    assert result[0]['provisional_reason']=='working_navigation_pending'
    assert any(row.get('stage')=='unknown' for row in result[0]['source_rows'])
    assert raw==before
    for obstacle in [phase(10,'recovery',13),phase(10,'manual',13),event(11,'timeout'),event(11,'manual_required')]:
        assert any(row['timeline_state']=='recovery' for row in clips(build_timeline(raw+[obstacle],session(20))))
    assert any(row['timeline_state']=='recovery' for row in clips(build_timeline(raw[:3],session(20))))


def test_old_closed_attention_phase_does_not_block_later_navigation_transient():
    raw=[phase(0,'manual',1),page(5,'province',6),page(10,'unknown',11),page(14,'procedure',15)]
    runs=session(20);runs[0]['context']={'app_version':'historical'}
    result=clips(build_timeline(raw,runs))
    assert not any(r['timeline_state']=='recovery' for r in result)
    assert any(r.get('restored_reason')=='historical_navigation_transient' for r in result)


def test_supported_recovery_phase_overrides_stale_normal_page_only():
    raw=[page(0,'province',1),phase(0,'working',10),phase(10,'recovery',20),phase(20,'working',30)]
    result=clips(build_timeline(raw,session(30)))
    assert [(r['start'],r['end'],r['timeline_state']) for r in result]==[(at(0),at(10),'search'),(at(10),at(20),'recovery'),(at(20),at(30),'search')]
    assert result[1]['source_rows'][0]['mode']=='recovery'
    for stage,evidence,state in [('blocked',[],'anomaly'),('request_no_slots',['operation_code'],'cod')]:
        special=page(0,stage,1);special['evidence']=evidence
        result=clips(build_timeline([special]+raw[1:],session(30)))
        assert all(r['timeline_state']==state for r in result)
    later=page(15,'procedure',16)
    result=clips(build_timeline(raw+[later],session(30)))
    assert any(r['start']==at(15) and r['timeline_state']=='search' for r in result)


def test_large_live_form_run_preserves_all_facts_as_one_clip():
    from datetime import datetime,timedelta,timezone
    origin=datetime(2026,1,1,tzinfo=timezone.utc)
    rows=[]
    for index in range(6000):
        left=(origin+timedelta(seconds=index)).isoformat()
        right=(origin+timedelta(seconds=index+1)).isoformat()
        rows.append(dict(kind='page',stage=['province','procedure','applicant','request_menu'][index%4],start=left,end=right,last_seen=right,session_id='live',event=False,evidence=[]))
    runs=[dict(session='live',start=rows[0]['start'],end=None,last_seen=rows[-1]['end'],reason='')]
    result=clips(build_timeline(rows,runs))
    assert len(result)==1
    assert len(result[0]['source_rows'])==6000
    assert result[0]['start']==rows[0]['start'] and result[0]['end']==rows[-1]['end']


def test_dense_navigation_block_retains_each_fact_once():
    from datetime import datetime,timedelta,timezone
    origin=datetime(2026,1,1,tzinfo=timezone.utc)
    raw=[page(0,'province',1)]
    for index in range(200):
        when=(origin+timedelta(hours=10,seconds=2,microseconds=index)).isoformat()
        raw.append(dict(kind='page',stage='unknown',start=when,end=when,last_seen=when,session_id='s',event=False,evidence=[]))
    raw.append(page(4,'procedure',5))
    runs=session(10);runs[0]['context']={'app_version':'historical'}
    result=clips(build_timeline(raw,runs))
    assert len(result)==1
    assert len(result[0]['source_rows'])==len(raw)
    assert len({id(row) for row in result[0]['source_rows']})==len(raw)
    assert result[0]['restoration_provenance'][0]['source_rows']==raw[1:-1]


def test_cod_survives_navigation_until_confirmed_normal_result():
    raw=[page(0,'request_no_slots_restart',1),page(10,'province',11),
         page(20,'applicant_no_slots',21),page(25,'applicant',26),page(30,'request_menu',31),
         page(40,'request_no_slots',41)]
    raw[0]['evidence']=['operation_code','no_appointments']
    result=clips(build_timeline(raw,session(50)))
    assert [(r['start'],r['end'],r['timeline_state']) for r in result]==[
        (at(0),at(40),'cod'),(at(40),at(50),'search')]


def test_cod_navigation_retains_errors_and_availability_ends_condition():
    raw=[page(0,'request_no_slots_restart',1),page(10,'server_error',11),
         page(20,'province',21),event(30,'office',offices=['Test office']),
         page(31,'office',32)]
    raw[0]['evidence']=['operation_code','no_appointments']
    result=clips(build_timeline(raw,session(40)))
    assert [(r['start'],r['end'],r['timeline_state']) for r in result]==[
        (at(0),at(10),'cod'),(at(10),at(20),'recovery'),
        (at(20),at(30),'cod'),(at(30),at(40),'search')]
