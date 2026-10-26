import os
import asyncio
from types import SimpleNamespace
from unittest.mock import AsyncMock

import pytest
from app.assist_control import AssistControl
from app import assist_flow


def test_no_slots_states_need_stage_context():
    assert assist_flow.no_slots_kind(applicant=True, after_contacts=False) == "applicant_no_slots"
    assert assist_flow.no_slots_kind(applicant=False, after_contacts=True) == "post_contact_no_slots"
    assert assist_flow.no_slots_kind(applicant=False, after_contacts=False) == "request_no_slots"


def test_real_offices_ignore_placeholder_and_disabled():
    options = [dict(value="", text="Seleccione oficina", disabled=False),
               dict(value="-1", text="Seleccionar oficina", disabled=False),
               dict(value="1", text="Oficina de prueba", disabled=False),
               dict(value="2", text="Oficina cerrada", disabled=True)]
    assert [o["value"] for o in assist_flow.available_offices(options)] == ["1"]


def test_embedded_expired_page_wins_over_expected_form():
    from app.page_inspection import dead_document
    def frame(text):
        return SimpleNamespace(locator=lambda _: SimpleNamespace(inner_text=AsyncMock(return_value=text)),
            frame_element=AsyncMock(return_value=SimpleNamespace(is_visible=AsyncMock(return_value=True))))
    main = frame("Formulario de datos")
    child = frame("Su sesión ha caducado. Pulse Volver")
    page = SimpleNamespace(main_frame=main, frames=[main, child])
    found = asyncio.run(dead_document(page))
    assert found == (child, "session_expired")


def test_pause_before_action_survives_page_reload():
    control = AssistControl();control.set_paused(True)
    page = SimpleNamespace(wait_for_timeout=AsyncMock())
    runner = assist_flow.AssistRunner(page, SimpleNamespace(), SimpleNamespace(),
        SimpleNamespace(id="test"), "run", control, SimpleNamespace(), SimpleNamespace())
    async def resume(_):
        control.set_paused(False)
    page.wait_for_timeout.side_effect = resume
    asyncio.run(runner.wait_unpaused())
    page.wait_for_timeout.assert_awaited()
    assert not control.paused


def test_manual_revisit_keeps_manual_period_without_capture_or_actions():
    control = AssistControl()
    control.set_diagnostics(True)
    history = []
    control.set_history_sink(lambda kind, **fields: history.append((kind, fields)))
    control.publish(screen='', evidence='', next_action='', mode='manual')
    page = SimpleNamespace(url='about:blank', wait_for_timeout=AsyncMock())
    runner = assist_flow.AssistRunner(page, SimpleNamespace(), SimpleNamespace(),
        SimpleNamespace(id='test'), 'run', control, SimpleNamespace(), SimpleNamespace())
    runner.manual_signature = 'manual-form'
    runner.observe = AsyncMock(return_value=assist_flow.Observed('applicant', 'doc', 'manual-form', None))
    runner.save = AsyncMock()
    runner.guard = AsyncMock()
    asyncio.run(runner.step())
    runner.save.assert_not_awaited()
    runner.guard.assert_not_awaited()
    page.wait_for_timeout.assert_awaited_once_with(500)
    assert control.snapshot()['mode'] == 'manual'
    assert [fields['mode'] for kind, fields in history if kind == 'phase'] == ['manual']


def test_slow_manual_read_keeps_manual_period():
    async def scenario():
        control = AssistControl()
        control.publish(screen='', evidence='', next_action='', mode='manual')
        runner = assist_flow.AssistRunner(SimpleNamespace(), SimpleNamespace(), SimpleNamespace(),
            SimpleNamespace(id='test'), 'run', control, SimpleNamespace(), SimpleNamespace())
        runner.manual_signature = 'manual-form'
        event = asyncio.Event()
        runner._observe = AsyncMock(side_effect=event.wait)
        read = asyncio.create_task(runner.observe())
        try:
            await asyncio.sleep(.6)
            assert control.snapshot()['mode'] == 'manual'
        finally:
            read.cancel()
            with pytest.raises(asyncio.CancelledError):
                await read
    asyncio.run(scenario())


@pytest.mark.parametrize("stage", ["province","procedure","auth","applicant","request_menu","office","contact"])
def test_embedded_expiry_blocks_actions_at_every_stage(stage):
    from app.site import SiteCheckError
    r=assist_flow.AssistRunner(SimpleNamespace(url="about:blank"),SimpleNamespace(),SimpleNamespace(),SimpleNamespace(id="test"),"run",AssistControl(),SimpleNamespace(),SimpleNamespace())
    r.guard_kind=stage;r.guard_document="same"
    r.observe=AsyncMock(return_value=assist_flow.Observed("expired","same","embedded",None))
    with pytest.raises(SiteCheckError) as raised:
        asyncio.run(r.action_guard())
    assert raised.value.code=="SESSION_EXPIRED"


def test_query_limit_never_closes_on_office_page():
    class EndStep(asyncio.CancelledError):pass
    c=AssistControl();c.request_sent()
    r=assist_flow.AssistRunner(SimpleNamespace(url="about:blank",is_closed=lambda:False),SimpleNamespace(),SimpleNamespace(),SimpleNamespace(id="test"),"run",c,SimpleNamespace(),SimpleNamespace())
    r.observe=AsyncMock(return_value=assist_flow.Observed("office","doc","sig",None))
    r.step=AsyncMock(side_effect=EndStep)
    with pytest.raises(EndStep):asyncio.run(r.run(max_cycles=1))


def test_recovery_timeout_does_not_escape_runner(monkeypatch):
    from app.site import SiteCheckError
    class End(asyncio.CancelledError):pass
    page=SimpleNamespace(url="about:blank",is_closed=lambda:False,wait_for_timeout=AsyncMock())
    r=assist_flow.AssistRunner(page,SimpleNamespace(),SimpleNamespace(),SimpleNamespace(id="test"),"run",AssistControl(),SimpleNamespace(),SimpleNamespace())
    r.step=AsyncMock(side_effect=[SiteCheckError("SESSION_EXPIRED"),End()])
    r.observe=AsyncMock(return_value=assist_flow.Observed("expired","doc","sig",None))
    r.recover=AsyncMock(side_effect=TimeoutError("recovering"))
    monkeypatch.setattr(assist_flow.asyncio,"sleep",AsyncMock())
    with pytest.raises(End):asyncio.run(r.run())
    assert r.step.await_count==2


def test_successful_terminal_result_resets_consecutive_recovery_counters():
    page=SimpleNamespace(url="about:blank")
    r=assist_flow.AssistRunner(page,SimpleNamespace(),SimpleNamespace(),SimpleNamespace(id="test"),"run",AssistControl(),SimpleNamespace(),SimpleNamespace())
    r.expired_attempts=3;r.server_attempts=3;r.server_button_attempts=3
    r.observe=AsyncMock(return_value=assist_flow.Observed("request_no_slots","doc","sig",None))
    r.save=AsyncMock();r.silence=AsyncMock();r.guard=AsyncMock(return_value=False)
    asyncio.run(r.step())
    assert r.expired_attempts==0 and r.server_attempts==0 and r.server_button_attempts==0


def test_slow_read_does_not_show_old_recognition_as_current():
    async def scenario():
        c=AssistControl();c.publish(screen="Старый экран",evidence="",next_action="",mode="working")
        r=assist_flow.AssistRunner(SimpleNamespace(url="about:blank"),SimpleNamespace(),SimpleNamespace(),SimpleNamespace(id="test"),"run",c,SimpleNamespace(),SimpleNamespace())
        event=asyncio.Event()
        r._observe=AsyncMock(side_effect=event.wait)
        read=asyncio.create_task(r.observe())
        await asyncio.sleep(.6)
        assert c.snapshot()["screen"]=="Проверяем страницу"
        read.cancel()
        with pytest.raises(asyncio.CancelledError):await read
    asyncio.run(scenario())


def test_pending_request_does_not_count_manual_get_or_redirect():
    frame=object();r=assist_flow.AssistRunner(SimpleNamespace(main_frame=frame),SimpleNamespace(),SimpleNamespace(),SimpleNamespace(id="test"),"run",AssistControl(),SimpleNamespace(),SimpleNamespace())
    r.pending_action="request"
    def request(method,path):return SimpleNamespace(method=method,url="https://icp.administracionelectronica.gob.es/icpplus/"+path,frame=frame,is_navigation_request=lambda:True)
    r._requested(request("GET","acEntrada"))
    assert r.control.snapshot()["requests"]==0
    r._requested(request("POST","acCitar"))
    r._requested(request("GET","acCitar"))
    assert r.control.snapshot()["requests"]==1


def test_timeline_urls_remove_auth_and_matrix_tokens():
    control=AssistControl()
    r=assist_flow.AssistRunner(SimpleNamespace(url="https://example.com/flow;sessionToken=PRIVATE?ticket=PRIVATE#PRIVATE"),SimpleNamespace(),SimpleNamespace(),SimpleNamespace(id="test"),"run",control,SimpleNamespace(),SimpleNamespace())
    r.record("action",assist_flow.Observed("procedure","doc","sig",None),"next")
    row=control.snapshot()["timeline"][-1]
    assert row["page"]=="https://example.com/flow" and "PRIVATE" not in str(row)



@pytest.mark.parametrize("lost_reply", [False, True])
def test_no_slots_waits_then_reloads_once(lost_reply):
    page = SimpleNamespace(url='https://example.test/acCitar', reload=AsyncMock(), go_back=AsyncMock())
    r = assist_flow.AssistRunner(page, SimpleNamespace(), SimpleNamespace(settings=SimpleNamespace(navigation_timeout_seconds=5)), SimpleNamespace(id='test'), 'run', AssistControl(), SimpleNamespace(), SimpleNamespace(next_delay=lambda **kw: 30))
    if lost_reply:
        page.reload.side_effect = RuntimeError("reply lost after send")
    observed = assist_flow.Observed('request_no_slots', 'doc', 'sig', None)
    r.countdown = AsyncMock(return_value=True)
    r.guard = AsyncMock(return_value=True)
    r.save = AsyncMock()
    r.hold = AsyncMock()
    async def scenario():
        if lost_reply:
            with pytest.raises(RuntimeError):
                await r.act(observed)
        else:
            await r.act(observed)
        await r.act(observed)
    asyncio.run(scenario())
    page.reload.assert_awaited_once()
    page.go_back.assert_not_awaited()
    r.countdown.assert_awaited_once()
    r.hold.assert_awaited_once()
    assert r.control.snapshot()['requests'] == 0  # no real POST observed


def test_no_slots_changed_during_timer_does_not_reload():
    page = SimpleNamespace(url='https://example.test/acCitar', reload=AsyncMock())
    r = assist_flow.AssistRunner(page, SimpleNamespace(), SimpleNamespace(settings=SimpleNamespace(navigation_timeout_seconds=5)), SimpleNamespace(id='test'), 'run', AssistControl(), SimpleNamespace(), SimpleNamespace(next_delay=lambda **kw: 30))
    r.countdown = AsyncMock(return_value=False)
    asyncio.run(r.act(assist_flow.Observed('request_no_slots', 'doc', 'sig', None)))
    page.reload.assert_not_awaited()


def test_single_office_already_selected_skips_reselection():
    select = SimpleNamespace(locator=lambda _: SimpleNamespace(evaluate_all=AsyncMock(return_value=[dict(value='16',text='Oficina',disabled=False)])), input_value=AsyncMock(return_value='16'), select_option=AsyncMock(side_effect=AssertionError('already selected')))
    page = SimpleNamespace(url='https://example.test/acCitar', evaluate=AsyncMock(return_value=dict(value='16',options=[dict(value='16',text='Oficina',disabled=False)])), locator=lambda _: SimpleNamespace(first=select), wait_for_timeout=AsyncMock())
    checker = SimpleNamespace(_activate_control=AsyncMock())
    r = assist_flow.AssistRunner(page, SimpleNamespace(), checker, SimpleNamespace(id='test'), 'run', AssistControl(), SimpleNamespace(), SimpleNamespace())
    observed = assist_flow.Observed('office', 'doc', 'sig', None)
    r.save=AsyncMock();r.silence=AsyncMock();r.guard=AsyncMock(return_value=True);r.observe=AsyncMock(return_value=observed)
    button = object()
    r.continuation=AsyncMock(return_value=button)
    asyncio.run(r.office(observed))
    select.select_option.assert_not_awaited()
    checker._activate_control.assert_awaited_once_with(button, timeout=5000)
    assert r.last_action=='office'


def test_error_call_log_is_saved_only_in_private_session(tmp_path):
    import json
    checker=SimpleNamespace(settings=SimpleNamespace(diagnostics_enabled=True), _debug_directory=lambda *args: tmp_path)
    control=AssistControl()
    r=assist_flow.AssistRunner(SimpleNamespace(url='https://example.test/infogenerica'),SimpleNamespace(),checker,SimpleNamespace(id='test'),'run',control,SimpleNamespace(),SimpleNamespace())
    r.action_phase='expired.continue'
    r.last_observed=assist_flow.Observed('expired','doc','sig',None)
    r.save_error_details(TimeoutError('Locator.click: timeout\nCall log: waiting for button'))
    path=tmp_path/'errors.jsonl'
    data=json.loads(path.read_text(encoding="utf-8"))
    assert data['phase']=='expired.continue'
    assert 'Call log' in data['message']
    if os.name != "nt":
        assert path.stat().st_mode & 0o777 == 0o600
    assert 'Call log' not in str(control.snapshot())


def test_expired_click_timeouts_are_spaced_instead_of_exhausting_retries():
    checker=SimpleNamespace(_activate_control=AsyncMock(side_effect=TimeoutError()))
    r=assist_flow.AssistRunner(SimpleNamespace(url='https://example.test/infogenerica'),SimpleNamespace(),checker,SimpleNamespace(id='test'),'run',AssistControl(),SimpleNamespace(),SimpleNamespace())
    observed=assist_flow.Observed('expired','doc','sig',None)
    r.silence=AsyncMock();r.save=AsyncMock();r.countdown=AsyncMock(return_value=True);r.expiry_scope=AsyncMock();r.continuation=AsyncMock(return_value=object());r.guard=AsyncMock(return_value=True)
    async def scenario():
        for _ in range(4):
            with pytest.raises(TimeoutError):await r.recover(observed)
    asyncio.run(scenario())
    assert [c.args[1] for c in r.countdown.await_args_list]==[0,30,60,120]


@pytest.mark.parametrize('endpoint,action,expected', [('acCitar','request','reload'),('procesarRespuestaClave',None,'goto'),('acOfertarCita','contact','hold'),('index.html','no_slots_restart','goto'),('index.html','contact','hold'),('index.html','request','hold')])
def test_server_recovery_only_repeats_safe_actions(endpoint,action,expected):
    page=SimpleNamespace(url='https://icp.administracionelectronica.gob.es/icpplustieb/'+endpoint,reload=AsyncMock(),goto=AsyncMock())
    r=assist_flow.AssistRunner(page,SimpleNamespace(),SimpleNamespace(),SimpleNamespace(id='test'),'run',AssistControl(),SimpleNamespace(),SimpleNamespace())
    r.last_method='POST';r.last_action=action
    r.silence=AsyncMock();r.save=AsyncMock();r.countdown=AsyncMock(return_value=True);r.hold=AsyncMock()
    asyncio.run(r.recover(assist_flow.Observed('server_error','doc','sig',None)))
    assert r.countdown.await_args.args[1]==30
    assert page.reload.await_count==(expected=='reload')
    assert page.goto.await_count==(expected=='goto')
    assert r.hold.await_count==(expected=='hold')


def test_applicant_warning_is_not_a_search_request():
    control = AssistControl()
    runner = assist_flow.AssistRunner(SimpleNamespace(url="about:blank"),
        SimpleNamespace(), SimpleNamespace(), SimpleNamespace(id="test"),
        "run", control, SimpleNamespace(), SimpleNamespace())
    for doc in ("first", "first", "reloaded", "reloaded"):
        runner.publish(assist_flow.Observed("applicant", doc, "same", None))
    assert control.snapshot()["checks"] == 0
    assert control.snapshot()["requests"] == 0
    runner.publish(assist_flow.Observed("expired", "error", "error", None))
    assert control.snapshot()["checks"] == 0


def test_availability_checks_include_manual_search_but_not_redirects():
    frame = object()
    control = AssistControl()
    runner = assist_flow.AssistRunner(SimpleNamespace(main_frame=frame, url="about:blank"),
        SimpleNamespace(), SimpleNamespace(), SimpleNamespace(id="test"),
        "run", control, SimpleNamespace(), SimpleNamespace())
    def request(method, target, request_frame=frame):
        return SimpleNamespace(method=method, url="https://example.com/icpplus/"+target,
            frame=request_frame, is_navigation_request=lambda: True)
    runner._requested(request("POST", "acCitar"))
    runner._requested(request("GET", "acCitar"))
    runner._requested(request("POST", "acCitar", object()))
    runner.pending_action = "request"
    runner._requested(request("POST", "acCitar"))
    assert control.snapshot()["checks"] == 2
    assert control.snapshot()["requests"] == 1


def test_server_error_uses_its_restart_button_before_repeating_post():
    frame = object()
    button = object()
    page = SimpleNamespace(url="https://icp.administracionelectronica.gob.es/icpplustieb/acCitar",
        main_frame=frame, reload=AsyncMock(), wait_for_timeout=AsyncMock())
    checker = SimpleNamespace(_activate_control=AsyncMock())
    runner = assist_flow.AssistRunner(page, SimpleNamespace(), checker,
        SimpleNamespace(id="test"), "run", AssistControl(), SimpleNamespace(), SimpleNamespace())
    runner.last_method = "POST"
    runner.last_action = "request"
    observed = assist_flow.Observed("server_error", "doc", "error", frame)
    runner.silence = AsyncMock(); runner.save = AsyncMock()
    runner.countdown = AsyncMock(return_value=True)
    runner.expiry_scope = AsyncMock(return_value=frame)
    runner.continuation = AsyncMock(return_value=button)
    runner.guard = AsyncMock(return_value=True)
    runner.observe = AsyncMock(return_value=assist_flow.Observed("province", "new", "new", frame))
    asyncio.run(runner.recover(observed))
    checker._activate_control.assert_awaited_once_with(button, timeout=5_000)
    page.reload.assert_not_awaited()
    assert runner.last_action is None and runner.pending_action is None
    assert runner.countdown.await_args.args[1] == 0


def test_office_submission_records_actual_manual_selection_without_personal_data():
    frame = object()
    control = AssistControl()
    runner = assist_flow.AssistRunner(SimpleNamespace(main_frame=frame, url="https://example.com/acCitar"),
        SimpleNamespace(), SimpleNamespace(), SimpleNamespace(id="test"), "run", control, SimpleNamespace(), SimpleNamespace())
    runner.current_offices = [{"value": "35", "text": "Office A"}, {"value": "36", "text": "Office B"}]
    runner.last_observed = assist_flow.Observed("office", "doc", "sig", frame)
    request = SimpleNamespace(method="POST", url="https://example.com/acVerFormulario?token=PRIVATE",
        frame=frame, is_navigation_request=lambda:True, post_data="idSede=36&email=PRIVATE&nie=PRIVATE")
    runner._requested(request)
    assert control.snapshot()["availability"] == []
    rows = [row for row in control.snapshot()["timeline"] if row["event"] in ("office_selected", "office_submitted")]
    assert [row["event"] for row in rows] == ["office_selected", "office_submitted"]
    assert "Office B" in rows[-1]["detail"]
    assert "PRIVATE" not in str(rows)


def test_office_polling_does_not_duplicate_sightings_but_records_manual_changes():
    options = [dict(value="35", text="Office A", disabled=False), dict(value="36", text="Office B", disabled=False)]
    select = SimpleNamespace(locator=lambda _:SimpleNamespace(evaluate_all=AsyncMock(return_value=options)),
        input_value=AsyncMock(side_effect=["35", "35", "36"]))
    page = SimpleNamespace(url="https://example.com/acCitar", evaluate=AsyncMock(side_effect=[dict(value=v,options=options) for v in ["35","35","36"]]), locator=lambda _:SimpleNamespace(first=select), wait_for_timeout=AsyncMock())
    control = AssistControl()
    runner = assist_flow.AssistRunner(page, SimpleNamespace(), SimpleNamespace(), SimpleNamespace(id="test"), "run", control, SimpleNamespace(), SimpleNamespace())
    runner.save = AsyncMock(); runner.signal = AsyncMock(); runner.guard = AsyncMock(return_value=True)
    observed = assist_flow.Observed("office", "doc", "sig", None)
    async def scenario():
        for _ in range(3):
            await runner.office(observed)
    asyncio.run(scenario())
    rows = control.snapshot()["availability"]
    assert [row["event"] for row in rows] == ["offices_seen"]
    selections = [row for row in control.snapshot()["timeline"] if row["event"] == "office_selected"]
    assert len(selections) == 2 and "Office B" in selections[-1]["detail"]


def test_booking_has_a_named_capture_and_timeline_event():
    page = SimpleNamespace(url="https://example.com/booking", evaluate=AsyncMock(),
        locator=lambda _:SimpleNamespace(get_attribute=AsyncMock(return_value=None)), wait_for_timeout=AsyncMock())
    control = AssistControl()
    runner = assist_flow.AssistRunner(page, SimpleNamespace(), SimpleNamespace(), SimpleNamespace(id="test"), "run", control, SimpleNamespace(), SimpleNamespace())
    runner.current_offices = [{"value": "35", "text": "Office A"}]
    runner.selected_office = runner.current_offices[0]
    runner.save = AsyncMock(); runner.signal = AsyncMock()
    observed = assist_flow.Observed("booking", "doc", "sig", None)
    async def scenario():
        await runner.booking_attention(observed)
        await runner.booking_attention(observed)
    asyncio.run(scenario())
    assert control.snapshot()["availability"] == []
    assert len([row for row in control.snapshot()["timeline"] if row["event"] == "booking_seen"]) == 1
    assert all(call.args[1] == "BOOKING_SELECTION_SCREEN" for call in runner.save.await_args_list)


def test_silent_return_to_province_logs_one_restart_across_loading_states():
    control = AssistControl()
    runner = assist_flow.AssistRunner(SimpleNamespace(url="https://example.com/acCitar"),
        SimpleNamespace(), SimpleNamespace(), SimpleNamespace(id="test"), "run", control, SimpleNamespace(), SimpleNamespace())
    for kind, doc in [("province","first"),("request_no_slots","result"),("loading","loading"),("province","second"),("province","second")]:
        runner.publish(assist_flow.Observed(kind, doc, doc, None))
    restarts = [r for r in control.snapshot()["timeline"] if r["event"] == "flow_restart"]
    assert len(restarts) == 1
    assert "Проход №2" in restarts[0]["detail"]
    assert "Сит нет после запроса" in restarts[0]["detail"]
    assert "причине не распознано" in restarts[0]["detail"]
    assert "сессия истекла" not in restarts[0]["detail"].lower()


def test_restart_remembers_explicit_recovery_reason():
    control = AssistControl()
    runner = assist_flow.AssistRunner(SimpleNamespace(url="https://example.com/index"),
        SimpleNamespace(), SimpleNamespace(), SimpleNamespace(id="test"), "run", control, SimpleNamespace(), SimpleNamespace())
    runner.publish(assist_flow.Observed("expired", "error", "error", None))
    runner.record("recovery", assist_flow.Observed("expired", "error", "error", None), "Кнопка сообщения об истёкшей сессии")
    runner.publish(assist_flow.Observed("loading", "loading", "loading", None))
    runner.publish(assist_flow.Observed("province", "new", "new", None))
    restart = next(r for r in control.snapshot()["timeline"] if r["event"] == "flow_restart")
    assert "истёкшей сессии" in restart["detail"]


def test_server_restart_button_retries_are_spaced_after_immediate_first_click():
    button = object()
    checker = SimpleNamespace(_activate_control=AsyncMock(side_effect=TimeoutError()))
    runner = assist_flow.AssistRunner(SimpleNamespace(url="https://icp.administracionelectronica.gob.es/acCitar"),
        SimpleNamespace(), checker, SimpleNamespace(id="test"), "run", AssistControl(), SimpleNamespace(), SimpleNamespace())
    runner.silence = AsyncMock(); runner.save = AsyncMock()
    runner.countdown = AsyncMock(return_value=True); runner.guard = AsyncMock(return_value=True)
    runner.expiry_scope = AsyncMock(); runner.continuation = AsyncMock(return_value=button)
    observed = assist_flow.Observed("server_error", "doc", "sig", object())
    async def scenario():
        for _ in range(4):
            with pytest.raises(TimeoutError):
                await runner.recover(observed)
    asyncio.run(scenario())
    assert [call.args[1] for call in runner.countdown.await_args_list] == [0,30,60,120]


@pytest.mark.parametrize("dispatched", [False, True])
def test_simple_navigation_retries_only_before_pointer_was_dispatched(dispatched):
    from playwright.async_api import TimeoutError as PlaywrightTimeoutError
    message = "Locator.click: Timeout 5000ms exceeded.\nwaiting for element to be visible, enabled and stable"
    if dispatched: message += "\nperforming click action"
    locator = SimpleNamespace(evaluate=AsyncMock(return_value=True))
    source = SimpleNamespace(locator=lambda _:SimpleNamespace(first=locator))
    observed = assist_flow.Observed("province", "doc", "sig", source)
    checker = SimpleNamespace(_activate_control=AsyncMock(side_effect=[PlaywrightTimeoutError(message), None]))
    page = SimpleNamespace(url="https://example.com/index", wait_for_load_state=AsyncMock(), wait_for_timeout=AsyncMock())
    runner = assist_flow.AssistRunner(page, SimpleNamespace(), checker, SimpleNamespace(id="test"), "run", AssistControl(), SimpleNamespace(), SimpleNamespace())
    runner.guard = AsyncMock(return_value=True); runner.save = AsyncMock(); runner.save_error_details = lambda _:None
    if dispatched:
        with pytest.raises(PlaywrightTimeoutError):asyncio.run(runner.click(observed, "#btnAceptar", action="province"))
        assert checker._activate_control.await_count == 1
    else:
        assert asyncio.run(runner.click(observed, "#btnAceptar", action="province"))
        assert checker._activate_control.await_count == 2
        assert checker._activate_control.await_args.kwargs["keyboard"] is True


@pytest.mark.parametrize("action,clear", [("province",False),("applicant",True),("contact",True)])
def test_stability_retry_does_not_repeat_covered_buttons_or_personal_submissions(action, clear):
    from playwright.async_api import TimeoutError as PlaywrightTimeoutError
    locator=SimpleNamespace(evaluate=AsyncMock(return_value=clear))
    source=SimpleNamespace(locator=lambda _:SimpleNamespace(first=locator))
    checker=SimpleNamespace(_activate_control=AsyncMock(side_effect=PlaywrightTimeoutError("waiting for element to be visible, enabled and stable")))
    runner=assist_flow.AssistRunner(SimpleNamespace(url="https://example.com/index"),SimpleNamespace(),checker,SimpleNamespace(id="test"),"run",AssistControl(),SimpleNamespace(),SimpleNamespace())
    runner.guard=AsyncMock(return_value=True);runner.save=AsyncMock()
    with pytest.raises(PlaywrightTimeoutError):
        asyncio.run(runner.click(assist_flow.Observed("province","doc","sig",source),"#btnAceptar",action=action))
    assert checker._activate_control.await_count==1


def test_finished_runner_detaches_owned_network_listeners():
    listeners={"request":[],"response":[]}
    page=SimpleNamespace(url="about:blank",is_closed=lambda:True,
        on=lambda event,fn:listeners[event].append(fn),
        remove_listener=lambda event,fn:listeners[event].remove(fn))
    runner=assist_flow.AssistRunner(page,SimpleNamespace(),SimpleNamespace(),SimpleNamespace(id="test"),
        "run",AssistControl(),SimpleNamespace(),SimpleNamespace())
    asyncio.run(runner.run())
    assert listeners=={"request":[],"response":[]}


def test_booking_handoff_stays_manual_when_document_changes():
    page=SimpleNamespace(url='https://example.test/booking',evaluate=AsyncMock(),
        locator=lambda selector:SimpleNamespace(get_attribute=AsyncMock(return_value=None)),
        wait_for_timeout=AsyncMock())
    r=assist_flow.AssistRunner(page,SimpleNamespace(),SimpleNamespace(),SimpleNamespace(id='test'),'run',AssistControl(),
        SimpleNamespace(start=AsyncMock(),stop=AsyncMock()),SimpleNamespace())
    r.observe=AsyncMock(side_effect=[assist_flow.Observed('booking','first','first',None),
        assist_flow.Observed('server_error','next','next',None)])
    r.save=AsyncMock();r.act=AsyncMock();r.recover=AsyncMock()
    async def scenario():
        await r.step();await r.step()
    asyncio.run(scenario())
    assert r.control.snapshot()['booking_handoff']
    assert r.observe.await_count==1
    r.act.assert_not_awaited();r.recover.assert_not_awaited()
    assert r.checker._assist_guard is None


def test_passive_read_failure_does_not_restart_manual_booking():
    class End(asyncio.CancelledError):pass
    c=AssistControl();c.booking_handoff=True
    r=assist_flow.AssistRunner(SimpleNamespace(url='about:blank',is_closed=lambda:False),SimpleNamespace(),SimpleNamespace(),SimpleNamespace(id='test'),'run',c,SimpleNamespace(),SimpleNamespace())
    r.step=AsyncMock(side_effect=[RuntimeError('navigation during passive read'),End()])
    r.observe=AsyncMock();r.recover=AsyncMock()
    with pytest.raises(End):asyncio.run(r.run())
    r.observe.assert_not_awaited();r.recover.assert_not_awaited()


def test_cycle_limit_cannot_close_a_handed_over_browser():
    class End(asyncio.CancelledError):pass
    c=AssistControl();c.request_sent();c.booking_handoff=True
    r=assist_flow.AssistRunner(SimpleNamespace(url='about:blank',is_closed=lambda:False),SimpleNamespace(),SimpleNamespace(),SimpleNamespace(id='test'),'run',c,SimpleNamespace(),SimpleNamespace())
    r.step=AsyncMock(side_effect=End())
    r.observe=AsyncMock(return_value=assist_flow.Observed('request_no_slots','manual-next','manual-next',None))
    with pytest.raises(End):asyncio.run(r.run(max_cycles=1))
    r.observe.assert_not_awaited()


def test_explicit_restart_after_booking_clears_pause_but_not_old_page_latch():
    c=AssistControl();c.booking_handoff=True;c.set_paused(True)
    c.request_restart()
    assert c.restart_requested and not c.paused and c.booking_handoff
    c.set_session('new')
    assert not c.booking_handoff

@pytest.mark.parametrize('kind', ['province', 'procedure', 'applicant', 'request_menu'])
def test_cookie_banner_is_dismissed_before_automatic_form_action(kind):
    checker=SimpleNamespace(_accept_cookies=AsyncMock(return_value=True))
    r=assist_flow.AssistRunner(SimpleNamespace(url='about:blank'),SimpleNamespace(),checker,
        SimpleNamespace(id='test'),'run',AssistControl(),SimpleNamespace(),SimpleNamespace())
    observed=assist_flow.Observed(kind,'doc','sig',None)
    r.observe=AsyncMock(return_value=observed);r.publish=lambda *a,**kw:None
    r.save=AsyncMock();r.silence=AsyncMock();r.guard=AsyncMock(return_value=True);r.act=AsyncMock()
    asyncio.run(r.step())
    checker._accept_cookies.assert_awaited_once_with(r.page)
    r.act.assert_not_awaited()
