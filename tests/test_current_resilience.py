import asyncio
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock
import pytest


def test_tls_network_timeout_does_not_prevent_strict_browser_launch(monkeypatch, tmp_path):
    import camoufox.async_api
    from app import site
    from app.config import Settings
    from app.desktop import certificates
    def unavailable(*args):raise TimeoutError('offline')
    launch=AsyncMock(return_value=SimpleNamespace(on=lambda *args:None))
    monkeypatch.setattr(certificates,'prepare_intermediate_cache',unavailable);monkeypatch.setattr(camoufox.async_api,'AsyncNewBrowser',launch);monkeypatch.setattr(site,'bundled_browser',lambda:tmp_path/'camoufox');monkeypatch.setattr(site,'bundled_addons',lambda:[])
    (tmp_path/'application.ini').write_text('[App]\nVersion=156.0.1\n')
    checker=site.IcpPlusChecker(Settings(browser_session='fresh'));task=SimpleNamespace(id='test',schedule=SimpleNamespace(timezone='Europe/Madrid'))
    asyncio.run(checker._launch_camoufox(None,task,headless=True));assert launch.await_args.kwargs['ignore_https_errors'] is False


def test_empty_site_placeholder_is_loading_but_real_unknown_text_is_not():
    from app.assist_flow import loading_placeholder
    assert loading_placeholder('')
    assert loading_placeholder('Proceso automático para la solicitud de cita previa')
    assert not loading_placeholder('Una pantalla desconocida con contenido')


def test_stalled_loading_restarts_safely_without_replaying_post():
    from app.assist_flow import AssistRunner,Observed
    from app.assist_control import AssistControl
    from app.assist_runtime import AssistRestartRequested
    r=AssistRunner(SimpleNamespace(url='https://example.test',wait_for_timeout=AsyncMock()),None,SimpleNamespace(),SimpleNamespace(id='test'),'run',AssistControl(),SimpleNamespace(),None)
    r.loading_started=-10000;r.loading_document='doc';r.countdown=AsyncMock(return_value=True);r.save=AsyncMock();r.silence=AsyncMock();r.record=lambda *args,**kwargs:None
    with pytest.raises(AssistRestartRequested) as error:asyncio.run(r.wait_loading(Observed('loading','doc','sig',None)))
    assert not error.value.clear_profile


def test_manual_attention_repeats():
    from app.assist_flow import AssistRunner,Observed
    from app.assist_control import AssistControl
    r=AssistRunner(SimpleNamespace(url='https://example.test',wait_for_timeout=AsyncMock()),None,SimpleNamespace(),SimpleNamespace(id='test'),'run',AssistControl(),SimpleNamespace(),None)
    r.record=lambda *args,**kwargs:None;r.publish=lambda *args,**kwargs:None;r.save=AsyncMock();r.signal=AsyncMock()
    asyncio.run(r.hold(Observed('unknown','doc','sig',None),'manual'));r.signal.assert_awaited_once_with(Observed('unknown','doc','sig',None),'manual',repeat=True)


@pytest.mark.parametrize('preexisting',[False,True])
def test_native_prompt_detection_during_pending_activation(monkeypatch,preexisting):
    from app import site
    from app.config import Settings
    async def scenario():
        release=asyncio.Event();started=asyncio.Event();prompt=False
        async def activate(button):
            started.set()
            await release.wait()
        def windows():return {11} if preexisting or prompt else set()
        from app import certificate_prompt
        monkeypatch.setattr(certificate_prompt,'certificate_prompt_ids',windows)
        checker=site.IcpPlusChecker(Settings());checker._activate_control=activate
        attention=AsyncMock()
        action=asyncio.create_task(checker._activate_certificate_control(object(),attention))
        await started.wait()
        prompt=True
        await asyncio.sleep(.35)
        assert attention.await_count==(0 if preexisting else 1)
        assert not action.done()
        release.set();await action
        assert attention.await_count==(0 if preexisting else 1)
    asyncio.run(scenario())


def test_automatic_restart_has_default_cooldown_and_preserves_longer_waits(monkeypatch):
    from app import assist_runtime
    monkeypatch.setattr(assist_runtime.random,'uniform',lambda lower,upper:42)
    assert assist_runtime.AssistRestartRequested('unknown').delay_seconds==42
    assert assist_runtime.AssistRestartRequested('timeout',delay_seconds=5).delay_seconds==42
    assert assist_runtime.AssistRestartRequested('unknown',delay_seconds=0).delay_seconds==42
    assert assist_runtime.AssistRestartRequested('long',delay_seconds=180).delay_seconds==180
    assert assist_runtime.AssistRestartRequested('already waited',delay_seconds=0,cooldown_completed=True).delay_seconds==0
    assert assist_runtime.AssistRestartRequested('manual',delay_seconds=0,user_requested=True).delay_seconds==0


def test_detach_missing_listener_preserves_cancellation_and_cleans_remaining():
    from app.assist_flow import AssistRunner
    from app.assist_control import AssistControl
    calls=[]
    def remove(event,handler):
        calls.append(event)
        if event=='request':raise KeyError(handler)
    page=SimpleNamespace(on=lambda *args:None,remove_listener=remove)
    r=AssistRunner(page,None,SimpleNamespace(),SimpleNamespace(id='test'),'run',AssistControl(),SimpleNamespace(),None)
    async def cancelled(**kwargs):raise asyncio.CancelledError()
    r._run=cancelled
    with pytest.raises(asyncio.CancelledError):asyncio.run(r.run())
    assert calls==['request','response']
    r.detach()
    assert calls==['request','response']
