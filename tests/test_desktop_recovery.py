import asyncio
from types import SimpleNamespace
from unittest.mock import AsyncMock
import pytest
from playwright.async_api import TimeoutError as PlaywrightTimeoutError
from app.assist_control import AssistControl
from app.assist_flow import AssistRunner, Observed
from app.assist_runtime import AssistRestartRequested


def runner():
    control=AssistControl()
    page=SimpleNamespace(url='https://example.test/acCitar',wait_for_timeout=AsyncMock())
    r=AssistRunner(page,None,SimpleNamespace(settings=SimpleNamespace(diagnostics_enabled=True)),SimpleNamespace(id='test'),'run',control,SimpleNamespace(stop=AsyncMock(),start=AsyncMock()),None)
    r.countdown=AsyncMock(return_value=True);r.save=AsyncMock()
    return r


def test_pre_dispatch_retries_are_bounded_then_restart():
    r=runner();screen=Observed('applicant','doc','sig',None)
    r.action_phase='applicant.click';r.attempted.add(('doc','applicant'))
    error=PlaywrightTimeoutError('waiting for element to be visible, enabled and stable')
    async def scenario():
        await r.recover_timeout(screen,screen,error)
        await r.recover_timeout(screen,screen,error)
        with pytest.raises(AssistRestartRequested): await r.recover_timeout(screen,screen,error)
    asyncio.run(scenario())
    assert [c.args[1] for c in r.countdown.await_args_list]==[2,5]
    assert ('doc','applicant') not in r.attempted


def test_dispatched_post_with_new_document_is_not_repeated():
    r=runner();screen=Observed('request_menu','old','sig',None)
    r.observe=AsyncMock(return_value=Observed('request_no_slots','new','sig2',None))
    asyncio.run(r.recover_timeout(screen,screen,PlaywrightTimeoutError('performing click action')))
    r.countdown.assert_not_awaited()
    assert not r.control.restart_requested


def test_diagnostics_disabled_prevents_full_error_file(tmp_path):
    r=runner();r.control.set_diagnostics(False)
    r.checker._debug_directory=lambda *_:tmp_path
    r.save_error_details(RuntimeError('PRIVATE FORM CONTENT'))
    assert not list(tmp_path.iterdir())


def test_sound_disabled_does_not_start_attention():
    r=runner();r.control.set_sound(False)
    asyncio.run(r.signal(Observed('office','doc','sig',None),'Office selection',repeat=True))
    r.alerter.start.assert_not_awaited()


def test_rate_limit_waits_three_minutes_then_single_get_preserving_session():
    r=runner();r.page.goto=AsyncMock();r.last_method='POST';r.last_action='contact'
    observed=Observed('rate_limited','doc','sig',None)
    asyncio.run(r.recover(observed))
    assert r.countdown.await_args.args[1] == 180
    assert r.page.goto.await_count == 1
    assert not r.control.restart_requested and r.control.blocked_streak == 0
    assert r.last_action is None


def test_navigation_timeout_after_post_does_not_retry_as_unsent_click():
    r=runner();screen=Observed('request_no_slots','doc','sig',None)
    r.observe=AsyncMock(return_value=screen)
    r.attempted.add(('doc','request_reload'))
    with pytest.raises(AssistRestartRequested):
        asyncio.run(r.recover_timeout(screen,screen,PlaywrightTimeoutError('waiting for navigation to finish')))
    r.countdown.assert_not_awaited()


def test_captcha_rings_repeatedly_before_diagnostics():
    r=runner()
    r.page.evaluate=AsyncMock()
    r.page.locator=lambda _:SimpleNamespace(get_attribute=AsyncMock(return_value=None))
    events=[]
    async def signal(*args, **kwargs): events.append(('signal',kwargs['repeat']))
    async def save(*args, **kwargs): events.append(('save',None))
    r.signal=signal; r.save=save
    asyncio.run(r.booking_attention(Observed('captcha','doc','sig',None)))
    assert events[0]==('signal',True)


def test_stop_retains_browser_until_user_closes_it():
    from app.assist_runtime import retain_browser
    async def scenario():
        closed=False
        page=SimpleNamespace(is_closed=lambda:closed)
        control=AssistControl(); control.request_stop()
        pending=asyncio.create_task(retain_browser(page,control))
        await asyncio.sleep(.01)
        assert not pending.done()
        assert control.snapshot()['browser_retained']
        closed=True
        await asyncio.wait_for(pending,1)
    asyncio.run(scenario())


def test_block_keeps_captured_screen_during_cooldown_and_cancel_does_not_reset():
    r = runner()
    r.countdown = AsyncMock(return_value=False)
    screen = Observed('blocked', 'doc', 'sig', None, 'Request Rejected; Support ID: 123')
    asyncio.run(r.recover(screen))
    r.save.assert_awaited_once_with(screen, 'HTTP_BLOCKED')
    assert r.countdown.await_args.args[1] == 300
    assert not r.control.restart_requested
    assert any('Request Rejected' in row['detail'] for row in r.control.snapshot()['timeline'])


def test_shutdown_releases_retained_browser_without_waiting_for_manual_close():
    from app.assist_runtime import retain_browser
    from app.desktop.worker import apply_command
    async def scenario():
        page=SimpleNamespace(is_closed=lambda:False)
        control=AssistControl();control.request_stop()
        pending=asyncio.create_task(retain_browser(page,control))
        await asyncio.sleep(.01)
        assert control.browser_retained
        assert apply_command(control,{'command':'shutdown'})
        await asyncio.wait_for(pending,1)
        assert not control.browser_retained
        assert control.close_browser_requested
    asyncio.run(scenario())


def test_initial_navigation_error_preserves_open_document_for_flow():
    from playwright.async_api import Error
    from app.assist_runtime import open_initial_page
    r=runner();r.page.is_closed=lambda:False
    r.checker._open_entry=AsyncMock(side_effect=Error('navigation interrupted'))
    r.save_error_details=lambda error:None
    asyncio.run(open_initial_page(r.checker,r.page,r.task,r.run_id,r))
    assert r.checker._open_entry.await_count==1
    assert any(row['event']=='action_interrupted' for row in r.control.snapshot()['timeline'])


def test_initial_navigation_does_not_hide_closed_browser_or_python_bug():
    from playwright.async_api import Error
    from app.assist_runtime import open_initial_page
    r=runner();r.page.is_closed=lambda:True
    r.checker._open_entry=AsyncMock(side_effect=Error('closed'))
    with pytest.raises(Error):asyncio.run(open_initial_page(r.checker,r.page,r.task,r.run_id,r))
    r.page.is_closed=lambda:False
    r.checker._open_entry=AsyncMock(side_effect=ValueError('programming error'))
    with pytest.raises(ValueError):asyncio.run(open_initial_page(r.checker,r.page,r.task,r.run_id,r))
