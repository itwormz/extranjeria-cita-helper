from types import SimpleNamespace
import asyncio
from dataclasses import replace
from pathlib import Path
from unittest.mock import AsyncMock
import pytest

from app.assist_control import AssistControl
from app.config import Settings
from app.desktop.worker import apply_command
from app.assist_runtime import run_assist, AssistRestartRequested, wait_restart


def test_worker_commands_and_eof_stop_are_process_owned():
    control = AssistControl()
    apply_command(control, {'command':'pause','paused':True})
    assert control.paused
    apply_command(control, {'command':'preferences','diagnostics':False,'sound':False})
    assert not control.diagnostics_enabled and not control.sound_enabled
    apply_command(control, {'command':'restart'})
    assert control.restart_requested
    apply_command(control, {'command':'stop'})
    assert control.stop_requested


def test_reset_removes_only_owned_profile_after_session_unwinds(monkeypatch, tmp_path):
    from app import assist_runtime
    from types import SimpleNamespace
    owned = tmp_path / 'camoufox' / 'test'; owned.mkdir(parents=True)
    (owned / 'cookie').write_text('old session')
    neighbor = tmp_path / 'camoufox' / 'neighbor'; neighbor.mkdir()
    control = AssistControl()
    async def session(*args, **kwargs):
        if owned.exists():
            raise AssistRestartRequested('manual', clear_profile=True, delay_seconds=0,user_requested=True)
        return 0
    monkeypatch.setattr(assist_runtime, '_run_session', session)
    settings = replace(Settings(), browser_engine='camoufox', browser_profile_directory=str(tmp_path))
    assert asyncio.run(run_assist(SimpleNamespace(id='test'), settings, control=control, open_panel=False)) == 0
    assert not owned.exists() and neighbor.exists()
    assert any(r['event']=='flow_restart' for r in control.snapshot()['timeline'])


def test_stop_cancels_restart_countdown():
    control = AssistControl(); control.request_stop()
    assert asyncio.run(wait_restart(control, 300)) is False


def test_blocked_requests_context_restart_instead_of_reload():
    from app.assist_flow import AssistRunner, Observed
    from types import SimpleNamespace
    control = AssistControl()
    page = SimpleNamespace(url='https://example.test/index', reload=AsyncMock())
    runner = AssistRunner(page, None, SimpleNamespace(), SimpleNamespace(id='test'), 'run', control, None, None)
    runner.save = AsyncMock()
    runner.countdown = AsyncMock(return_value=True)
    with pytest.raises(AssistRestartRequested) as error:
        asyncio.run(runner.recover(Observed('blocked','doc','sig',None)))
    assert error.value.clear_profile and error.value.delay_seconds == 0
    assert runner.countdown.await_args.args[1] == 300
    assert control.blocked_streak == 1
    page.reload.assert_not_awaited()


def test_auth_clave_does_not_enter_certificate():
    from app.assist_flow import AssistRunner, Observed
    from types import SimpleNamespace
    control = AssistControl()
    checker = SimpleNamespace(_enter_with_clave=AsyncMock(), _enter_with_certificate=AsyncMock())
    runner = AssistRunner(SimpleNamespace(url='https://example.test/index'), None, checker,
                         SimpleNamespace(id='test',access_mode='clave'), 'run', control, None, None)
    runner.silence = AsyncMock()
    asyncio.run(runner.act(Observed('auth','doc','sig',None)))
    checker._enter_with_clave.assert_awaited_once()
    checker._enter_with_certificate.assert_not_awaited()


def test_stop_cancels_hung_action_but_closes_browser_only_after_user(monkeypatch,tmp_path):
    from types import SimpleNamespace
    from app import assist_runtime, assist
    import playwright.async_api
    order=[]
    control=AssistControl();control.set_diagnostics(False)
    closed=False
    page=SimpleNamespace(set_default_timeout=lambda *_:None, is_closed=lambda:closed)
    class Checker:
        def __init__(self,settings):self.settings=settings
        def _debug_directory(self,*_):return tmp_path
        async def launch_context(self,*_,**kwargs):return SimpleNamespace()
        async def page_for_context(self,*_):return page
        async def _open_entry(self,*_):await asyncio.Event().wait()
        async def close_context(self,*_):order.append('browser_closed')
    class Driver:
        async def start(self):return self
        async def stop(self):order.append('driver_stopped')
    class Runner:
        def __init__(self,*_):pass
        def detach(self):pass
        async def wait_unpaused(self):pass
        async def run(self,**_):return 0
    monkeypatch.setattr(playwright.async_api,'async_playwright',Driver)
    from app import site
    monkeypatch.setattr(site,'IcpPlusChecker',Checker)
    monkeypatch.setattr(assist,'_install_lifecycle_logging',lambda *_:None)
    monkeypatch.setattr(assist_runtime,'AssistRunner',Runner)
    async def scenario():
        nonlocal closed
        running=asyncio.create_task(assist_runtime._run_session(SimpleNamespace(id='test'),Settings(),control))
        await asyncio.sleep(.05);control.request_stop()
        await asyncio.sleep(.3)
        assert control.snapshot()['browser_retained'] and not running.done()
        assert order==[]
        closed=True
        return await asyncio.wait_for(running,2)
    assert asyncio.run(scenario())==0
    assert order==['browser_closed','driver_stopped']


def test_paused_zero_delay_restart_waits_for_resume():
    async def scenario():
        control = AssistControl(); control.set_paused(True)
        waiting = asyncio.create_task(wait_restart(control, 0))
        await asyncio.sleep(.05)
        assert not waiting.done()
        control.set_paused(False)
        assert await asyncio.wait_for(waiting, 1)
    asyncio.run(scenario())


@pytest.mark.parametrize('mode,expected',[('manual','_fill_applicant_manual'),('copy','_fill_applicant_with_clave')])
def test_applicant_uses_profile_identity_mode(mode,expected):
    from app.assist_flow import AssistRunner, Observed
    from types import SimpleNamespace
    observed=Observed('applicant','doc','sig',None)
    checker=SimpleNamespace(_fill_applicant_manual=AsyncMock(),_fill_applicant_with_clave=AsyncMock())
    runner=AssistRunner(SimpleNamespace(url='https://example.test/index'),None,checker,
        SimpleNamespace(id='test',applicant_fill_mode=mode),'run',AssistControl(),None,None)
    runner.observe=AsyncMock(return_value=observed);runner.click=AsyncMock()
    asyncio.run(runner.act(observed))
    getattr(checker,expected).assert_awaited_once()
    other='_fill_applicant_manual' if mode=='copy' else '_fill_applicant_with_clave'
    getattr(checker,other).assert_not_awaited()


@pytest.mark.parametrize('continue_wait', [True,False])
def test_clave_server_error_waits_three_minutes_then_reopens_without_reset(continue_wait):
    from app.assist_flow import AssistRunner,Observed
    from types import SimpleNamespace
    page=SimpleNamespace(url='https://pasarela.clave.gob.es/Proxy2/ServiceRedirect',reload=AsyncMock(),goto=AsyncMock())
    runner=AssistRunner(page,None,SimpleNamespace(),SimpleNamespace(id='test'),'run',AssistControl(),None,None)
    runner.save=AsyncMock();runner.silence=AsyncMock();runner.countdown=AsyncMock(return_value=continue_wait)
    if continue_wait:
        with pytest.raises(AssistRestartRequested) as error:
            asyncio.run(runner.recover(Observed('server_error','doc','sig',None)))
        assert error.value.reopen_browser and not error.value.clear_profile
    else:
        asyncio.run(runner.recover(Observed('server_error','doc','sig',None)))
    assert runner.countdown.await_args.args[1]==180
    page.reload.assert_not_awaited();page.goto.assert_not_awaited()


def test_clave_null_error_is_recognized_without_http_status():
    from app.site import _temporary_site_error_code
    assert _temporary_site_error_code('Se ha producido un error... null Por favor, reinténtelo de nuevo más tarde')=='SITE_TEMPORARY_ERROR'
    assert _temporary_site_error_code('Por favor, reinténtelo de nuevo más tarde') is None


def test_clave_interactive_login_rings_and_does_not_immediately_silence():
    from app.assist_flow import AssistRunner,Observed
    from app.site import SiteCheckError
    control=AssistControl()
    checker=SimpleNamespace(_enter_with_clave=AsyncMock(side_effect=SiteCheckError('AUTH_REQUIRED',False,'PAUSED_AUTH','auth.clave')))
    runner=AssistRunner(SimpleNamespace(url='https://example.test/index'),None,checker,SimpleNamespace(id='test',access_mode='clave'),'run',control,None,None)
    runner.signal=AsyncMock();runner.silence=AsyncMock()
    observed=Observed('auth','doc','sig',None)
    asyncio.run(runner.act(observed))
    runner.signal.assert_awaited_once_with(observed,'clave_authentication',repeat=True)
    runner.silence.assert_not_awaited()

def test_clave_attention_does_not_restart_for_changing_qr_signature():
    from app.assist_flow import AssistRunner,Observed
    runner=AssistRunner(SimpleNamespace(url='https://example.test'),None,SimpleNamespace(),SimpleNamespace(id='test'),'run',AssistControl(),SimpleNamespace(start=AsyncMock(),stop=AsyncMock()),None)
    async def scenario():
        await runner.signal(Observed('auth_wait','doc','qr1',None),'clave_authentication',repeat=True)
        await runner.signal(Observed('auth_wait','doc','qr2',None),'clave_authentication',repeat=True)
    asyncio.run(scenario())
    runner.alerter.start.assert_awaited_once()


def test_clave_waiting_step_preserves_repeating_attention():
    from app.assist_flow import AssistRunner,Observed
    page=SimpleNamespace(url="https://www2.agenciatributaria.gob.es/wlpl/MOVI-P24H/ObtenerClaveMovilQR",wait_for_timeout=AsyncMock(),evaluate=AsyncMock(return_value=False))
    checker=SimpleNamespace(_select_clave_method=AsyncMock(return_value=False),_clave_requires_attention=AsyncMock(return_value=True))
    runner=AssistRunner(page,None,checker,SimpleNamespace(id="test",access_mode="clave"),"run",AssistControl(),None,None)
    runner.observe=AsyncMock(return_value=Observed("auth_wait","doc","qr",None))
    runner.save=AsyncMock();runner.signal=AsyncMock();runner.silence=AsyncMock()
    asyncio.run(runner.step())
    runner.signal.assert_awaited_once()
    assert runner.signal.call_args.kwargs["repeat"] is True
    runner.silence.assert_not_awaited()
    assert runner.control.snapshot()["mode"]=="manual"


def test_clave_empty_redirect_does_not_ring():
    from app.assist_flow import AssistRunner,Observed
    page=SimpleNamespace(url='https://pasarela.clave.gob.es/Proxy2/ResponseRedirect',wait_for_timeout=AsyncMock(),evaluate=AsyncMock(return_value=False))
    checker=SimpleNamespace(_select_clave_method=AsyncMock(return_value=False),_clave_requires_attention=AsyncMock(return_value=False))
    runner=AssistRunner(page,None,checker,SimpleNamespace(id='test',access_mode='clave'),'run',AssistControl(),None,None)
    runner.observe=AsyncMock(return_value=Observed('auth_wait','doc','blank',None))
    runner.save=AsyncMock();runner.signal=AsyncMock()
    asyncio.run(runner.step())
    runner.signal.assert_not_awaited()
    assert runner.control.snapshot()['mode']=='waiting'


def test_clave_human_attention_stops_sound_across_navigation():
    from app.assist_flow import AssistRunner, Observed
    page=SimpleNamespace(url='https://pasarela.clave.gob.es/login',wait_for_timeout=AsyncMock(),evaluate=AsyncMock(return_value=True))
    checker=SimpleNamespace(_select_clave_method=AsyncMock(return_value=False),_clave_requires_attention=AsyncMock(return_value=True))
    alerter=SimpleNamespace(start=AsyncMock(),stop=AsyncMock())
    runner=AssistRunner(page,None,checker,SimpleNamespace(id='test',access_mode='clave'),'run',AssistControl(),alerter,None)
    runner.save=AsyncMock()
    async def scenario():
        await runner.signal(Observed('auth_wait','doc','sig',None),'clave_authentication',repeat=True)
        runner.observe=AsyncMock(return_value=Observed('auth_wait','doc','sig',None))
        await runner.step()
        page.evaluate.return_value=False
        runner.observe.return_value=Observed('auth_wait','next','qr2',None)
        await runner.step()
    asyncio.run(scenario())
    alerter.start.assert_awaited_once()
    alerter.stop.assert_awaited_once()
