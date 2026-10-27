from app.assist_control import AssistControl


def visit(control, stage, document='doc', **kw):
    control.observe_visit(document=document, stage=stage, page='https://site.test/same', title=stage, **kw)


def rows(control):
    return control.snapshot()['journal']


def test_sent_click_is_not_success_until_recognized_transition():
    c=AssistControl();visit(c,'province')
    c.record('action',stage='province',document='doc',detail='province')
    c.record('action_completed',stage='province',document='doc',detail='province')
    assert len(rows(c))==1 and rows(c)[0]['status']=='processing'
    visit(c,'procedure','new')
    assert rows(c)[0]['status']=='passed'
    assert rows(c)[0]['destination']=='procedure'
    assert rows(c)[0]['ended_at']


def test_reload_same_url_finishes_old_check_and_opens_new_visit():
    c=AssistControl();visit(c,'request_no_slots')
    c.publish(screen='',evidence='',next_action='',mode='waiting',remaining_seconds=45)
    c.record('check',document='doc',stage='request_no_slots')
    visit(c,'request_no_slots','next')
    assert len(rows(c))==2
    assert rows(c)[0]['status']=='checked'
    assert rows(c)[1]['status']=='processing'
    assert rows(c)[0]['page']==rows(c)[1]['page']


def test_qr_redirect_and_timer_ticks_do_not_add_rows():
    c=AssistControl();visit(c,'auth_wait')
    for doc in ('qr2','qr3'):
        visit(c,'auth_wait',doc)
        c.publish(screen='',evidence='',next_action='',mode='manual')
    assert len(rows(c))==1 and rows(c)[0]['status']=='login_required'


def test_embedded_expiry_is_a_separate_exception_not_passed_form():
    c=AssistControl();visit(c,'applicant')
    visit(c,'expired')
    assert len(rows(c))==2
    assert rows(c)[0]['status']=='interrupted'
    assert rows(c)[1]['status']=='recovering'


def test_repeated_429_never_claims_recovery():
    c=AssistControl();visit(c,'rate_limited')
    c.record('recovery',document='doc',stage='rate_limited',detail='probe')
    visit(c,'rate_limited','next')
    assert rows(c)[0]['status']!='passed'
    c.record('recovery',document='next',stage='rate_limited',detail='probe')
    visit(c,'province','normal')
    assert rows(c)[1]['status']=='passed'


def test_office_observation_selection_submission_are_one_visit():
    c=AssistControl();visit(c,'office')
    c.update_offices([{'text':'CNP Original NAME','value':'1'}])
    c.update_offices([{'text':'CNP Original NAME','value':'1'}],selected='CNP Original NAME')
    c.record('office_submitted',document='doc',stage='office')
    visit(c,'contact','next')
    assert len(rows(c))==2 and rows(c)[0]['status']=='offices_found'
    assert rows(c)[0]['offices']==['CNP Original NAME']
    assert rows(c)[0]['selected_office']=='CNP Original NAME'


def test_manual_office_choice_overrides_green_milestone():
    c=AssistControl();visit(c,'office')
    c.update_offices([{'text':'A'},{'text':'B'}])
    c.publish(screen='',evidence='',next_action='',mode='manual')
    assert rows(c)[0]['status']=='choice_required'


def test_pause_resume_and_stop_do_not_mark_visit_passed():
    c=AssistControl();visit(c,'applicant')
    c.set_paused(True)
    assert rows(c)[0]['status']=='paused'
    c.set_paused(False)
    assert rows(c)[0]['status']=='processing'
    c.request_stop()
    c.record('finish',detail='Поиск остановлен');c.finish('stopped')
    assert rows(c)[0]['status']=='cancelled'
    assert [r['event'] for r in rows(c)[1:]]==['pause','resume','finish']


def test_fatal_error_stays_red_after_finish():
    c=AssistControl();visit(c,'applicant')
    c.record('fatal_error',detail='error')
    c.record('finish',detail='stopped');c.finish('stopped')
    assert rows(c)[0]['status']=='failed'


def test_late_action_completion_does_not_overwrite_next_page():
    c=AssistControl();visit(c,'province')
    visit(c,'procedure','next')
    c.record('action_completed',stage='province',document='doc',detail='province')
    assert rows(c)[1]['stage']=='procedure' and rows(c)[1]['status']=='processing'


def test_journal_persists_updates_and_keeps_raw_diagnostics(tmp_path):
    import json
    c=AssistControl();c.set_timeline_path(tmp_path/'timeline.jsonl')
    visit(c,'province')
    c.record('capture',detail='TRACE_SCREEN')
    c.record('response',detail='HTTP 200')
    visit(c,'procedure','next')
    assert len(rows(c))==2
    assert [json.loads(s)['event'] for s in (tmp_path/'timeline.jsonl').read_text(encoding="utf-8").splitlines()]==['capture','response']
    persisted=[json.loads(s) for s in (tmp_path/'journal.jsonl').read_text(encoding="utf-8").splitlines()]
    assert any(r['status']=='passed' for r in persisted)


def test_unknown_after_dispatched_click_does_not_claim_success():
    c=AssistControl();visit(c,'applicant')
    c.record('action_completed',stage='applicant',document='doc',detail='applicant')
    visit(c,'unknown','new')
    assert rows(c)[0]['status']=='unconfirmed'
    assert rows(c)[1]['status']=='review_required'


def test_runtime_restart_and_recognition_add_one_boundary_and_keep_new_page_active():
    c=AssistControl();visit(c,'expired')
    c.record('flow_restart',detail='Новая сессия')
    visit(c,'province','next')
    c.record('flow_restart',document='next',stage='province',detail='Проход №2')
    assert len([r for r in rows(c) if r['event']=='flow_restart'])==1
    assert rows(c)[-1]['stage']=='province' and rows(c)[-1]['ended_at'] is None


def test_auth_entry_is_not_reported_as_completed_login():
    c=AssistControl();visit(c,'auth')
    visit(c,'auth_wait','next')
    assert 'Авторизация завершена' not in rows(c)[0]['message']


def test_recovery_button_and_its_actual_destination_are_kept_in_same_row():
    c=AssistControl();visit(c,'expired')
    c.record('recovery_button',document='doc',stage='expired',detail='Aceptar')
    assert rows(c)[0]['status']=='recovering'
    visit(c,'province','next')
    from app.assist_journal import description
    assert description(rows(c)[0])=='Кнопка Aceptar → province'


def test_runner_recognition_exposes_page_journal_with_chosen_clave_method():
    from types import SimpleNamespace
    from app.assist_flow import AssistRunner, Observed
    for method,title in [('movil','Cl@ve Móvil'),('permanente','Cl@ve Permanente')]:
        c=AssistControl()
        r=AssistRunner(SimpleNamespace(url='https://site.test/login'),None,SimpleNamespace(),SimpleNamespace(access_mode='clave',clave_method=method),'run',c,None,None)
        r.publish(Observed('auth_wait','doc','qr',None),mode='manual')
        assert rows(c)[0]['title']==title
        assert rows(c)[0]['status']=='login_required'


def test_diagnostic_noise_does_not_evict_human_history():
    c=AssistControl();visit(c,'applicant')
    for _ in range(300):
        c.record('capture',detail='TRACE_SCREEN')
    assert len(c.snapshot()['timeline'])==200
    assert len(rows(c))==1 and rows(c)[0]['stage']=='applicant'


def test_manual_hold_keeps_the_specific_reason():
    c=AssistControl();visit(c,'applicant')
    c.record('manual_required',document='doc',stage='applicant',detail='Контакты не заполнены. Проверь поля вручную')
    c.publish(screen='',evidence='',next_action='',mode='manual')
    assert rows(c)[0]['message']=='Контакты не заполнены. Проверь поля вручную'


def test_failure_before_browser_open_is_visible():
    c=AssistControl();c.record('fatal_error',detail='startup error');c.record('finish',detail='stopped')
    assert rows(c)[0]['status']=='failed'


def test_automatic_end_does_not_claim_user_interruption():
    c=AssistControl();visit(c,'applicant')
    c.record('finish',detail='finished')
    assert rows(c)[0]['status']=='interrupted'
    assert 'пользовател' not in rows(c)[0]['message']


def test_all_journal_templates_translate_and_keep_site_values_verbatim():
    from pathlib import Path
    import ast,json
    from app.assist_journal import description
    catalog=json.loads(Path('app/desktop/assets/translations.json').read_text(encoding="utf-8"))
    for node in ast.walk(ast.parse(Path('app/assist_journal.py').read_text(encoding="utf-8"))):
        if isinstance(node,ast.Constant) and isinstance(node.value,str) and node.value and 'А' <= node.value[0] <= 'Я':
            assert node.value in catalog['en'] and node.value in catalog['es'], node.value
    for locale in ('en','es'):
        row=dict(message='Выбран офис: {selected_office} → {destination}',selected_office='Нет сит {original}',destination='contact')
        result=description(row,lambda s:catalog[locale].get(s,s),lambda s:'Контактные данные')
        assert 'Нет сит {original}' in result


def test_date_availability_evidence_survives_user_close():
    c=AssistControl();visit(c,'booking')
    c.request_stop();c.record('finish',detail='closed')
    assert rows(c)[0]['status']=='date_available'
    assert rows(c)[0]['ended_at']
    assert rows(c)[-1]['event']=='finish'


def test_new_pass_marker_precedes_initial_page_in_panel_and_persistence(tmp_path):
    from types import SimpleNamespace
    from app.assist_flow import AssistRunner, Observed
    import json
    c=AssistControl();c.set_timeline_path(tmp_path/'timeline.jsonl')
    r=AssistRunner(SimpleNamespace(url='https://site.test/expired'),None,SimpleNamespace(),SimpleNamespace(id='test'),'run',c,None,None)
    r.publish(Observed('expired','one','old',None))
    r.record('recovery_button',Observed('expired','one','old',None),'Aceptar')
    r.page.url='https://site.test/start'
    r.publish(Observed('province','two','new',None))
    assert [row['event'] for row in rows(c)]==['visit','flow_restart','visit']
    assert rows(c)[0]['status']=='passed'
    assert rows(c)[1]['time']<=rows(c)[2]['time']
    saved=[json.loads(line) for line in (tmp_path/'journal.jsonl').read_text(encoding="utf-8").splitlines()]
    newest={}
    for row in saved: newest[row['id']]=row
    assert [row['event'] for row in newest.values()]==['visit','flow_restart','visit']


def test_booking_details_survive_journal_updates_and_persist_without_debug(tmp_path):
    import json
    from app.assist_journal import description
    c=AssistControl();c.set_diagnostics(False);c.set_timeline_path(tmp_path/'timeline.jsonl')
    visit(c,'booking');c.set_booking_details({'office':'CNP TEST, CALLE TEST, 1','offers':[{'date':'2026-10-28','time':'12:30'}]},document='doc')
    for _ in range(3):c.publish(screen='',evidence='',next_action='',mode='manual')
    booking=next(row for row in rows(c) if row['stage']=='booking')
    assert len(rows(c))==1
    text=description(booking)
    assert 'CNP TEST, CALLE TEST, 1' in text and '28.10.2026 — 12:30' in text
    c.request_stop();c.record('finish',detail='done')
    assert '28.10.2026 — 12:30' in description(rows(c)[0])
    saved=[json.loads(line) for line in (tmp_path/'journal.jsonl').read_text().splitlines()]
    assert any(row.get('booking_details',{}).get('offers') for row in saved)


def test_applicant_warning_continue_journals_fill_submit_and_confirmed_transition():
    from app.assist_journal import description
    c=AssistControl();visit(c,'applicant_no_slots',applicant_continue=True)
    c.record('action',stage='applicant_no_slots',document='doc',detail='Ввести NIE и имя из профиля')
    assert description(rows(c)[0])=='Заполняем данные заявителя'
    c.record('action_completed',stage='applicant_no_slots',document='doc',detail='applicant')
    assert description(rows(c)[0])=='Данные заявителя отправлены — ожидается ответ'
    visit(c,'request_menu','next')
    assert rows(c)[0]['status']=='passed'
    assert rows(c)[0]['message']=='Данные заявителя отправлены → {destination}'


def test_applicant_warning_wait_journal_stays_a_form_refresh():
    from app.assist_journal import description
    c=AssistControl();visit(c,'applicant_no_slots')
    c.record('action',stage='applicant_no_slots',document='doc',detail='Обновить форму без ввода данных')
    assert description(rows(c)[0])=='Обновляем форму без ввода данных'
