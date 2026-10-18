from app.config import Settings
import asyncio
import os
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock

import pytest

from app.site import (
    _browser_launch_error_code,
    _challenge_code,
    _configure_browser_path,
    _flow_endpoint,
    _http_500_page,
    _regular_chrome_user_agent,
    _support_id,
    _temporary_site_error_code,
    browser_profile_path,
    IcpPlusChecker,
    SiteCheckError,
)


def test_local_browser_path_is_configured(monkeypatch, tmp_path: Path):
    local_browsers = tmp_path / ".playwright"
    local_browsers.mkdir()
    monkeypatch.delenv("PLAYWRIGHT_BROWSERS_PATH", raising=False)

    _configure_browser_path(tmp_path)

    assert str(local_browsers) == os.environ["PLAYWRIGHT_BROWSERS_PATH"]


def test_explicit_browser_path_is_not_overwritten(monkeypatch, tmp_path: Path):
    (tmp_path / ".playwright").mkdir()
    monkeypatch.setenv("PLAYWRIGHT_BROWSERS_PATH", "/custom/browser/path")

    _configure_browser_path(tmp_path)

    assert os.environ["PLAYWRIGHT_BROWSERS_PATH"] == "/custom/browser/path"


def test_missing_browser_has_specific_error_code():
    exc = RuntimeError("Executable doesn't exist. Please run: playwright install")
    assert _browser_launch_error_code(exc) == "BROWSER_NOT_INSTALLED"


def test_other_launch_failure_has_specific_error_code():
    assert _browser_launch_error_code(RuntimeError("process exited")) == "BROWSER_LAUNCH_FAILED"


def test_fortigate_intrusion_page_is_a_block():
    text = "FortiGate Intrusion Prevention — Intrusion Prevention Triggered"
    assert _challenge_code(text) == "HTTP_BLOCKED"


def test_recorded_request_rejected_body_is_a_block():
    text = "The requested URL was rejected. Please consult with your administrador."
    assert _challenge_code(text) == "HTTP_BLOCKED"


def test_f5_support_id_is_extracted():
    assert _support_id("Your support ID is: <12147379153550661712>") == "12147379153550661712"


def test_request_rejected_records_flow_stage():
    settings = SimpleNamespace(navigation_timeout_seconds=5, browser_action_delay_seconds=(0, 0))
    checker = IcpPlusChecker(settings)

    class Locator:
        def __init__(self, selector):
            self.selector = selector
            self.first = self

        async def count(self):
            return 0

        async def inner_text(self):
            assert self.selector == "body"
            return "The requested URL was rejected."

    class Page:
        def locator(self, selector):
            return Locator(selector)

        async def title(self):
            return "Request Rejected"

    with pytest.raises(SiteCheckError) as raised:
        asyncio.run(checker._assert_no_challenge(Page(), "request.office"))

    assert raised.value.code == "HTTP_BLOCKED"
    assert raised.value.detail == "request.office"


def test_regular_chrome_user_agent_does_not_advertise_headless():
    user_agent = _regular_chrome_user_agent("151.0.7922.34", "Linux")
    assert "Chrome/151.0.0.0" in user_agent
    assert "HeadlessChrome" not in user_agent
    assert "X11; Linux x86_64" in user_agent


def test_recorded_system_error_is_retryable_site_error():
    visible_text = "Se ha producido un error en el sistema, por favor inténtelo de nuevo."
    assert _temporary_site_error_code(visible_text) == "SITE_TEMPORARY_ERROR"


def test_http_500_page_is_detected():
    assert _http_500_page("500 Internal Server Error")
    assert not _http_500_page("Información sobre cita previa")


def test_flow_endpoint_ignores_query_and_trailing_slash():
    assert _flow_endpoint("https://example.test/flow/acValidarEntrada/?x=1") == "acValidarEntrada"


def test_http_500_is_reloaded_once_and_recovers():
    settings = SimpleNamespace(
        navigation_timeout_seconds=5,
        browser_action_delay_seconds=(0, 0),
        page_reload_attempts=1,
    )
    checker = IcpPlusChecker(settings)
    checker._settle = AsyncMock()

    class Locator:
        def __init__(self, page):
            self.page = page

        async def inner_text(self):
            return "500 Internal Server Error" if self.page.failed else "Formulario disponible"

    class Page:
        failed = True

        def locator(self, selector):
            assert selector == "body"
            return Locator(self)

        async def title(self):
            return "Error 500" if self.failed else "Cita previa"

        async def reload(self, **_kwargs):
            self.failed = False
            return SimpleNamespace(status=200)

    recovered = asyncio.run(
        checker._reload_on_server_error(Page(), "applicant.form", "client_01", "run123")
    )

    assert recovered is True
    checker._settle.assert_awaited_once()


def test_entry_bootstrap_waits_and_reloads_before_form_actions():
    settings = SimpleNamespace(
        browser_bootstrap_enabled=True,
        browser_bootstrap_wait_seconds=(0, 0),
    )
    checker = IcpPlusChecker(settings)
    checker._settle = AsyncMock()
    checker._assert_no_challenge = AsyncMock()
    checker._reload_on_server_error = AsyncMock(return_value=False)
    checker._accept_cookies = AsyncMock()
    response = SimpleNamespace(status=200)
    page = SimpleNamespace(
        goto=AsyncMock(return_value=response),
        reload=AsyncMock(return_value=response),
        wait_for_timeout=AsyncMock(),
        wait_for_load_state=AsyncMock(),
    )
    task = SimpleNamespace(id="client_01")

    asyncio.run(checker._open_entry(page, task, "run123"))

    page.goto.assert_awaited_once()
    page.reload.assert_awaited_once()
    assert page.wait_for_timeout.await_count == 2
    checker._accept_cookies.assert_awaited_once()






def test_browser_profile_is_task_specific_and_private(tmp_path):
    settings = SimpleNamespace(browser_profile_directory=str(tmp_path), browser_engine="webkit")
    profile = browser_profile_path(settings, "client_01")
    assert profile == tmp_path / "webkit" / "client_01"
    if os.name != "nt":
        assert profile.stat().st_mode & 0o777 == 0o700
        assert profile.parent.stat().st_mode & 0o777 == 0o700


def test_text_is_entered_as_sequential_keyboard_events():
    settings = Settings(browser_action_delay_seconds=(0, 0))
    checker = IcpPlusChecker(settings)
    locator = SimpleNamespace(
        click=AsyncMock(),
        fill=AsyncMock(),
        press_sequentially=AsyncMock(),
    )
    page = SimpleNamespace(locator=lambda _selector: locator)

    asyncio.run(checker._type_like_user(page, "#field", "ABC"))

    locator.click.assert_awaited_once()
    locator.fill.assert_awaited_once_with("")
    locator.press_sequentially.assert_awaited_once()
    assert locator.press_sequentially.await_args.args == ("ABC",)
    assert 70 <= locator.press_sequentially.await_args.kwargs["delay"] <= 140






def test_launch_context_uses_persistent_headed_task_profile(tmp_path):
    settings = SimpleNamespace(
        browser_profile_directory=str(tmp_path),
        browser_engine="chromium",
        browser_mode="headed",
        browser_session="persistent",
        browser_action_delay_seconds=(0, 0),
    )
    task = SimpleNamespace(
        id="client_01",
        schedule=SimpleNamespace(timezone="Europe/Madrid"),
    )

    class Chromium:
        def __init__(self):
            self.launch_persistent_context = AsyncMock(return_value=object())

    chromium = Chromium()
    playwright = SimpleNamespace(chromium=chromium, devices={})

    context = asyncio.run(IcpPlusChecker(settings).launch_context(playwright, task))

    assert context is chromium.launch_persistent_context.return_value
    chromium.launch_persistent_context.assert_awaited_once_with(
        user_data_dir=str(tmp_path / "chromium" / "client_01"),
        headless=False,
        channel="chromium",
        locale="es-ES",
        timezone_id="Europe/Madrid",
        viewport={"width": 1365, "height": 900},
    )


def test_launch_context_can_use_supported_headless_descriptor(tmp_path):
    settings = SimpleNamespace(
        browser_profile_directory=str(tmp_path),
        browser_engine="chromium",
        browser_mode="headless",
        browser_session="persistent",
        browser_action_delay_seconds=(0, 0),
    )
    task = SimpleNamespace(
        id="client_01",
        schedule=SimpleNamespace(timezone="Europe/Madrid"),
    )
    chromium = SimpleNamespace(launch_persistent_context=AsyncMock(return_value=object()))
    playwright = SimpleNamespace(
        chromium=chromium,
        devices={"Desktop Chrome": {"user_agent": "supported desktop UA"}},
    )

    asyncio.run(IcpPlusChecker(settings).launch_context(playwright, task))

    assert chromium.launch_persistent_context.await_args.kwargs["headless"] is True
    assert chromium.launch_persistent_context.await_args.kwargs["user_agent"] == "supported desktop UA"


def test_launch_context_uses_webkit_without_chromium_channel(tmp_path):
    settings = SimpleNamespace(
        browser_profile_directory=str(tmp_path),
        browser_engine="webkit",
        browser_mode="headed",
        browser_session="persistent",
        browser_action_delay_seconds=(0, 0),
    )
    task = SimpleNamespace(
        id="client_01",
        schedule=SimpleNamespace(timezone="Europe/Madrid"),
    )
    webkit = SimpleNamespace(launch_persistent_context=AsyncMock(return_value=object()))
    playwright = SimpleNamespace(webkit=webkit, devices={})

    asyncio.run(IcpPlusChecker(settings).launch_context(playwright, task))

    kwargs = webkit.launch_persistent_context.await_args.kwargs
    assert kwargs["user_data_dir"] == str(tmp_path / "webkit" / "client_01")
    assert kwargs["headless"] is False
    assert "channel" not in kwargs


def test_launch_context_uses_fresh_session_without_profile(tmp_path):
    settings = SimpleNamespace(
        browser_profile_directory=str(tmp_path),
        browser_engine="webkit",
        browser_mode="headed",
        browser_session="fresh",
        browser_action_delay_seconds=(0, 0),
    )
    task = SimpleNamespace(
        id="client_01",
        schedule=SimpleNamespace(timezone="Europe/Madrid"),
    )
    context = object()
    browser = SimpleNamespace(new_context=AsyncMock(return_value=context), close=AsyncMock())
    webkit = SimpleNamespace(launch=AsyncMock(return_value=browser))
    playwright = SimpleNamespace(webkit=webkit, devices={})

    result = asyncio.run(IcpPlusChecker(settings).launch_context(playwright, task))

    assert result is context
    webkit.launch.assert_awaited_once_with(headless=False)
    browser.new_context.assert_awaited_once_with(
        locale="es-ES",
        timezone_id="Europe/Madrid",
        viewport={"width": 1365, "height": 900},
    )
    assert not (tmp_path / "webkit" / "client_01").exists()


def test_launch_context_vanilla_chromium_uses_browser_defaults(tmp_path):
    settings = SimpleNamespace(
        browser_preset="vanilla",
        browser_profile_directory=str(tmp_path),
        browser_engine="chromium",
        browser_mode="headed",
        browser_session="fresh",
        browser_action_delay_seconds=(0, 0),
    )
    task = SimpleNamespace(
        id="client_01",
        schedule=SimpleNamespace(timezone="Europe/Madrid"),
    )
    context = object()
    browser = SimpleNamespace(new_context=AsyncMock(return_value=context), close=AsyncMock())
    chromium = SimpleNamespace(launch=AsyncMock(return_value=browser))
    playwright = SimpleNamespace(chromium=chromium, devices={})

    result = asyncio.run(IcpPlusChecker(settings).launch_context(playwright, task))

    assert result is context
    chromium.launch.assert_awaited_once_with(headless=False)
    browser.new_context.assert_awaited_once_with()


def test_launch_context_can_use_branded_chromium_executable(tmp_path):
    executable = "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"
    settings = SimpleNamespace(
        browser_preset="vanilla",
        browser_profile_directory=str(tmp_path),
        browser_engine="chromium",
        browser_executable_path=executable,
        browser_mode="headed",
        browser_session="fresh",
        browser_action_delay_seconds=(0, 0),
    )
    task = SimpleNamespace(
        id="client_01",
        schedule=SimpleNamespace(timezone="Europe/Madrid"),
    )
    context = object()
    browser = SimpleNamespace(new_context=AsyncMock(return_value=context), close=AsyncMock())
    chromium = SimpleNamespace(launch=AsyncMock(return_value=browser))
    playwright = SimpleNamespace(chromium=chromium, devices={})

    result = asyncio.run(IcpPlusChecker(settings).launch_context(playwright, task))

    assert result is context
    chromium.launch.assert_awaited_once_with(headless=False, executable_path=executable)
    browser.new_context.assert_awaited_once_with()


def test_close_context_also_closes_ephemeral_browser():
    browser = SimpleNamespace(is_connected=lambda: True, close=AsyncMock())
    context = SimpleNamespace(browser=browser, close=AsyncMock())

    asyncio.run(IcpPlusChecker(SimpleNamespace()).close_context(context))

    context.close.assert_awaited_once()
    browser.close.assert_awaited_once()


def test_close_context_tolerates_window_closed_by_user():
    browser = SimpleNamespace(is_connected=lambda: False, close=AsyncMock())
    context = SimpleNamespace(
        browser=browser,
        close=AsyncMock(side_effect=RuntimeError("Target page, context or browser has been closed")),
    )

    asyncio.run(IcpPlusChecker(SimpleNamespace()).close_context(context))

    context.close.assert_awaited_once()
    browser.close.assert_not_awaited()




def test_clave_entry_accepts_silent_sso_return():
    settings = SimpleNamespace(
        browser_action_delay_seconds=(0, 0),
        navigation_timeout_seconds=1,
    )
    checker = IcpPlusChecker(settings)
    checker._settle = AsyncMock()
    checker._assert_no_challenge = AsyncMock()

    class Locator:
        def __init__(self, page):
            self.page = page
            self.first = self

        async def count(self):
            return 1

        async def is_visible(self):
            return True

        async def click(self):
            self.page.url = "https://icp.administracionelectronica.gob.es/icpplustieb//acEntrada"

    class Page:
        url = "https://icp.administracionelectronica.gob.es/icpplustieb/acInfo"

        def locator(self, _selector):
            return Locator(self)

        async def wait_for_timeout(self, _milliseconds):
            return None

    task = SimpleNamespace(id="client_01", access_mode="clave")

    asyncio.run(checker._enter_with_clave(Page(), task, "run123"))

    checker._settle.assert_awaited_once()
    checker._assert_no_challenge.assert_awaited_once()


def test_clave_entry_pauses_when_qr_authentication_is_required():
    settings = SimpleNamespace(
        browser_action_delay_seconds=(0, 0),
        navigation_timeout_seconds=1,
    )
    checker = IcpPlusChecker(settings)

    class Locator:
        def __init__(self, page):
            self.page = page
            self.first = self

        async def count(self):
            return 1

        async def is_visible(self):
            return True

        def nth(self, index):return self

        async def is_editable(self):return True

        async def click(self):
            self.page.url = (
                "https://www2.agenciatributaria.gob.es/wlpl/MOVI-P24H/"
                "ObtenerClaveMovilQR"
            )

    class Page:
        url = "https://icp.administracionelectronica.gob.es/icpplustieb/acInfo"

        def locator(self, _selector):
            return Locator(self)

        async def wait_for_timeout(self, _milliseconds):
            return None

    with pytest.raises(SiteCheckError) as raised:
        asyncio.run(
            checker._enter_with_clave(
                Page(), SimpleNamespace(id="client_01", access_mode="clave"), "run123"
            )
        )

    assert raised.value.code == "AUTH_REQUIRED"
    assert raised.value.state == "PAUSED_AUTH"
    assert raised.value.retryable is False






def test_external_auth_diagnostics_do_not_capture_qr_or_html(tmp_path, monkeypatch):
    monkeypatch.setenv("DEBUG_DIR", str(tmp_path))
    settings = SimpleNamespace(
        diagnostics_enabled=True,
        diagnostics_directory=str(tmp_path),
        diagnostics_retention_days=3,
    )
    checker = IcpPlusChecker(settings)
    page = SimpleNamespace(
        url=(
            "https://www2.agenciatributaria.gob.es/wlpl/MOVI-P24H/"
            "ObtenerClaveMovilQR?token=secret"
        ),
        screenshot=AsyncMock(),
        content=AsyncMock(),
    )

    saved = asyncio.run(
        checker._save_debug(
            page,
            "client_01",
            "run123",
            error_code="AUTH_REQUIRED",
            error_field="auth.clave",
        )
    )

    assert saved is True
    page.screenshot.assert_not_awaited()
    page.content.assert_not_awaited()
    assert len(list(tmp_path.rglob("*.json"))) == 1
    assert not list(tmp_path.rglob("*.png"))
    assert not list(tmp_path.rglob("*.html"))


@pytest.mark.parametrize('entry_method', ['_enter_with_certificate', '_enter_with_clave'])
def test_auth_return_detects_expired_session_before_waiting(entry_method):
    checker = IcpPlusChecker(SimpleNamespace(navigation_timeout_seconds=45))
    checker._pace = AsyncMock()
    checker._assert_no_challenge = AsyncMock()
    page = SimpleNamespace(
        url='https://icp.administracionelectronica.gob.es/icpplustieb/acInfo',
        title=AsyncMock(return_value='ICP'),
        wait_for_timeout=AsyncMock(side_effect=AssertionError('must not wait on an expired auth return')),
    )
    async def enter():
        page.url = 'https://icp.administracionelectronica.gob.es/icpplustieb/infogenerica'
    button = SimpleNamespace(count=AsyncMock(return_value=1), is_visible=AsyncMock(return_value=True),
                             click=enter)
    button.first = button
    body = SimpleNamespace(inner_text=AsyncMock(return_value='Su sesión ha caducado por permanecer demasiado tiempo inactiva.'))
    missing = SimpleNamespace(count=AsyncMock(return_value=0))
    missing.first = missing
    page.locator = lambda selector, **kwargs: button if selector == '#btnAccesoClave' else body if selector == 'body' else missing
    with pytest.raises(SiteCheckError) as raised:
        asyncio.run(getattr(checker, entry_method)(page, SimpleNamespace(id='test'), 'run'))
    assert raised.value.code == 'SESSION_EXPIRED'
    page.wait_for_timeout.assert_not_awaited()




@pytest.mark.parametrize('native_prompt',[True,False])
def test_certificate_click_chimes_only_for_native_prompt(monkeypatch,native_prompt):
    from app import certificate_prompt
    calls=[0]
    def prompts():
        calls[0]+=1;return {1} if native_prompt and calls[0]>1 else set()
    monkeypatch.setattr(certificate_prompt,'certificate_prompt_ids',prompts)
    checker = IcpPlusChecker(SimpleNamespace(navigation_timeout_seconds=45))
    checker._pace = AsyncMock()
    page = SimpleNamespace(url='https://icp.administracionelectronica.gob.es/icpplus/acInfo',
                           title=AsyncMock(return_value='ICP'), wait_for_timeout=AsyncMock())
    async def enter():
        page.url = 'https://auth.example.test/certificate'
    async def certificate():
        page.url = 'https://icp.administracionelectronica.gob.es/icpplus/infogenerica'
    def button(click):
        loc = SimpleNamespace(count=AsyncMock(return_value=1), is_visible=AsyncMock(return_value=True), click=click)
        loc.first = loc
        return loc
    body = SimpleNamespace(inner_text=AsyncMock(side_effect=lambda: 'Su sesión ha caducado' if page.url.endswith('infogenerica') else ''))
    page.locator = lambda selector, **kwargs: body if selector == 'body' else button(enter if selector == '#btnAccesoClave' else certificate)
    attention = AsyncMock()
    with pytest.raises(SiteCheckError):
        asyncio.run(checker._enter_with_certificate(page, SimpleNamespace(id='test'), 'run', on_attention=attention))
    if native_prompt:attention.assert_awaited_once_with('certificate_approval', repeat=False)
    else:attention.assert_not_awaited()


def test_waiting_for_certificate_auth_does_not_request_attention(monkeypatch):
    from app import site
    clock = [0.0]
    monkeypatch.setattr(site.time, 'monotonic', lambda: clock[0])
    checker = IcpPlusChecker(SimpleNamespace(navigation_timeout_seconds=45))
    checker._pace = AsyncMock()
    checker._raise_if_dead_page = AsyncMock(side_effect=[None, SiteCheckError('SITE_TEMPORARY_ERROR', True, 'ACTIVE')])
    async def waiting(*args):
        clock[0] = 10.0
    checker._assert_no_challenge = waiting
    page = SimpleNamespace(url='https://icp.administracionelectronica.gob.es/icpplus/acInfo', wait_for_timeout=AsyncMock())
    async def enter():
        page.url = 'https://icp.administracionelectronica.gob.es/icpplus/authReturn'
    button = SimpleNamespace(count=AsyncMock(return_value=1), is_visible=AsyncMock(return_value=True), click=enter)
    button.first = button
    page.locator = lambda selector, **kwargs: button
    attention = AsyncMock()
    with pytest.raises(SiteCheckError):
        asyncio.run(checker._enter_with_certificate(page, SimpleNamespace(id='test'), 'run', on_attention=attention))
    attention.assert_not_awaited()


@pytest.mark.parametrize('endpoint', ['acValidarEntrada', 'acEntrada', 'acInfo', 'acCitar'])
def test_flow_endpoint_accepts_url_session_parameter(endpoint):
    assert _flow_endpoint(f'https://icp.administracionelectronica.gob.es/icpplustieb//{endpoint};jsessionid=test-session?x=1') == endpoint






def test_diagnostics_separates_run_folders(tmp_path, monkeypatch):
    monkeypatch.setenv('DEBUG_DIR', str(tmp_path))
    checker = IcpPlusChecker(SimpleNamespace(diagnostics_enabled=True,
        diagnostics_directory=str(tmp_path), diagnostics_retention_days=3))
    page = SimpleNamespace(url='https://auth.example.test/login')
    for run in ('run-a', 'run-b'):
        assert asyncio.run(checker._save_debug(page, 'test', run))
        assert len(list((tmp_path / 'sessions' / f'test-{run}').glob('*.json'))) == 1
    assert not list(tmp_path.glob('*.json'))


def test_bootstrap_reload_checks_process_pause_guard():
    from app.config import Settings
    checker=IcpPlusChecker(Settings(browser_bootstrap_enabled=True,browser_bootstrap_wait_seconds=(0,0)))
    checker._settle=AsyncMock();checker._assert_no_challenge=AsyncMock();checker._reload_on_server_error=AsyncMock();checker._accept_cookies=AsyncMock()
    class Pause(Exception):pass
    checker._assist_guard=AsyncMock(side_effect=Pause())
    page=SimpleNamespace(goto=AsyncMock(return_value=SimpleNamespace(status=200)),wait_for_timeout=AsyncMock(),wait_for_load_state=AsyncMock(),reload=AsyncMock())
    with pytest.raises(Pause):asyncio.run(checker._open_entry(page,SimpleNamespace(id="test"),"run"))
    page.reload.assert_not_awaited()


@pytest.mark.parametrize('method,label',[('movil','Cl@ve Móvil'),('permanente','Cl@ve Permanente')])
def test_clave_method_selects_only_matching_visible_portal_control(method,label):
    checker=IcpPlusChecker(SimpleNamespace())
    checker._activate_control=AsyncMock()
    class Control:
        async def is_visible(self):return True
        async def inner_text(self):return label
        async def get_attribute(self,name):return None
    control=Control()
    class Candidates:
        async def count(self):return 1
        def nth(self,index):return control
    page=SimpleNamespace(url='https://pasarela.clave.gob.es/Proxy2/ServiceProvider',locator=lambda _:Candidates())
    assert asyncio.run(checker._select_clave_method(page,SimpleNamespace(clave_method=method)))
    checker._activate_control.assert_awaited_once_with(control)
    page.url='https://untrusted.example/ServiceProvider'
    assert not asyncio.run(checker._select_clave_method(page,SimpleNamespace(clave_method=method)))
    assert checker._activate_control.await_count==1

def test_clave_method_ambiguity_does_not_click():
    checker=IcpPlusChecker(SimpleNamespace());checker._activate_control=AsyncMock()
    control=SimpleNamespace(is_visible=AsyncMock(return_value=True),inner_text=AsyncMock(return_value='Cl@ve Móvil'),get_attribute=AsyncMock(return_value=None))
    candidates=SimpleNamespace(count=AsyncMock(return_value=2),nth=lambda _:control)
    page=SimpleNamespace(url='https://pasarela.clave.gob.es/Proxy2/ServiceProvider',locator=lambda _:candidates)
    assert not asyncio.run(checker._select_clave_method(page,SimpleNamespace(clave_method='movil')))
    checker._activate_control.assert_not_awaited()


def test_sent_click_waits_for_late_navigation_without_resending():
    from playwright.async_api import TimeoutError as PlaywrightTimeoutError
    async def scenario():
        ready=asyncio.Event()
        frame=SimpleNamespace(wait_for_load_state=AsyncMock())
        async def navigation(event,**kwargs):
            await ready.wait()
            assert kwargs['predicate'](frame)
            return frame
        async def sent_click(**kwargs):
            asyncio.get_running_loop().call_later(.02,ready.set)
            raise PlaywrightTimeoutError('click action done; waiting for scheduled navigations to finish')
        handle=SimpleNamespace(owner_frame=AsyncMock(return_value=frame),dispose=AsyncMock())
        locator=SimpleNamespace(page=SimpleNamespace(main_frame=frame,wait_for_event=navigation),
            element_handle=AsyncMock(return_value=handle),is_visible=AsyncMock(return_value=True),
            is_enabled=AsyncMock(return_value=True),click=AsyncMock(side_effect=sent_click))
        checker=IcpPlusChecker(Settings());checker._assist_guard=object()
        await checker._activate_control(locator,timeout=5000)
        assert locator.click.await_count==1
        frame.wait_for_load_state.assert_awaited_once()
    asyncio.run(scenario())


def test_unsubmitted_click_timeout_is_not_hidden_by_navigation_wait():
    from playwright.async_api import TimeoutError as PlaywrightTimeoutError
    async def scenario():
        frame=SimpleNamespace()
        async def never(event,**kwargs):await asyncio.Event().wait()
        handle=SimpleNamespace(owner_frame=AsyncMock(return_value=frame),dispose=AsyncMock())
        locator=SimpleNamespace(page=SimpleNamespace(main_frame=frame,wait_for_event=never),
            element_handle=AsyncMock(return_value=handle),is_visible=AsyncMock(return_value=True),
            is_enabled=AsyncMock(return_value=True),click=AsyncMock(side_effect=PlaywrightTimeoutError('waiting for element to be visible, enabled and stable')))
        checker=IcpPlusChecker(Settings());checker._assist_guard=object()
        with pytest.raises(PlaywrightTimeoutError):await checker._activate_control(locator,timeout=5000)
        assert locator.click.await_count==1
    asyncio.run(scenario())


def test_locked_session_activates_button_with_trusted_keyboard(monkeypatch):
    from app import desktop_session
    monkeypatch.setattr(desktop_session,"screen_locked",lambda:True)
    frame=SimpleNamespace()
    async def never(*args,**kwargs):await asyncio.Event().wait()
    locator=SimpleNamespace(page=SimpleNamespace(main_frame=frame,wait_for_event=never),
        element_handle=AsyncMock(return_value=SimpleNamespace(owner_frame=AsyncMock(return_value=frame),dispose=AsyncMock())),
        is_visible=AsyncMock(return_value=True),is_enabled=AsyncMock(return_value=True),
        evaluate=AsyncMock(return_value=True),press=AsyncMock(),click=AsyncMock())
    checker=IcpPlusChecker(Settings());checker._assist_guard=object()
    asyncio.run(checker._activate_control(locator,timeout=5000))
    locator.press.assert_awaited_once_with("Enter",timeout=5000)
    locator.click.assert_not_awaited()


def test_background_recovery_activates_button_with_trusted_keyboard(monkeypatch):
    from app import desktop_session
    monkeypatch.setattr(desktop_session, "screen_locked", lambda:False)
    frame=SimpleNamespace()
    async def never(*args,**kwargs):await asyncio.Event().wait()
    locator=SimpleNamespace(page=SimpleNamespace(main_frame=frame,wait_for_event=never),
        element_handle=AsyncMock(return_value=SimpleNamespace(owner_frame=AsyncMock(return_value=frame),dispose=AsyncMock())),
        is_visible=AsyncMock(return_value=True),is_enabled=AsyncMock(return_value=True),
        evaluate=AsyncMock(return_value=True),press=AsyncMock(),click=AsyncMock())
    checker=IcpPlusChecker(Settings());checker._assist_guard=object()
    asyncio.run(checker._activate_control(locator,timeout=5000,keyboard=True))
    locator.press.assert_awaited_once_with("Enter",timeout=5000)
    locator.click.assert_not_awaited()


@pytest.fixture(autouse=True)
def deterministic_unlocked_desktop(monkeypatch):
    # Unit mocks model an unlocked desktop; dedicated lock tests override this.
    from app import desktop_session
    monkeypatch.setattr(desktop_session,'screen_locked',lambda:False)


@pytest.mark.parametrize('outcome', ['quick', 'delayed', 'native'])
def test_certificate_attention_wait_is_single_and_cancelled(monkeypatch, outcome):
    checker = IcpPlusChecker(SimpleNamespace())
    async def scenario():
        original_sleep = asyncio.sleep
        timer_started = asyncio.Event()
        timer_release = asyncio.Event()
        notified = asyncio.Event()
        calls = []
        async def sleep(delay):
            assert delay == 5
            timer_started.set()
            await timer_release.wait()
        async def attention(reason, repeat=False):
            calls.append((reason, repeat));notified.set()
        async def entry(page, task, run_id, on_attention=None, on_certificate_started=None):
            # Time spent choosing an authentication method must not arm the reminder.
            await original_sleep(0)
            await original_sleep(0)
            assert not timer_started.is_set()
            on_certificate_started()
            await timer_started.wait()
            if outcome == 'native':
                await on_attention('certificate_approval', repeat=False)
            if outcome != 'quick':
                timer_release.set()
                await notified.wait()
                await original_sleep(0)
                await original_sleep(0)
            return 'interactive'
        monkeypatch.setattr(checker, '_enter_with_certificate_impl', entry, raising=False)
        monkeypatch.setattr(asyncio, 'sleep', sleep)
        result = await checker._enter_with_certificate(None, None, 'run', on_attention=attention)
        assert result == 'interactive'
        timer_release.set()
        await original_sleep(0)
        assert calls == ([] if outcome == 'quick' else [(('certificate_approval' if outcome == 'native' else 'certificate_wait'), False)])
    asyncio.run(scenario())


@pytest.mark.parametrize("navigated", [True, False])
def test_keyboard_timeout_observes_navigation_without_resending(monkeypatch, navigated):
    from app import desktop_session
    from playwright.async_api import TimeoutError as PlaywrightTimeoutError
    monkeypatch.setattr(desktop_session, 'screen_locked', lambda: True)
    async def scenario():
        ready = asyncio.Event()
        frame = SimpleNamespace(wait_for_load_state=AsyncMock())
        async def navigation(*args, **kwargs):
            await ready.wait()
            if not navigated:
                raise PlaywrightTimeoutError('no navigation')
            return frame
        async def sent_key(*args, **kwargs):
            ready.set()
            raise PlaywrightTimeoutError('Locator.press: Timeout 5000ms exceeded')
        locator = SimpleNamespace(
            page=SimpleNamespace(main_frame=frame, wait_for_event=navigation),
            element_handle=AsyncMock(return_value=SimpleNamespace(
                owner_frame=AsyncMock(return_value=frame), dispose=AsyncMock())),
            is_visible=AsyncMock(return_value=True), is_enabled=AsyncMock(return_value=True),
            evaluate=AsyncMock(return_value=True), press=AsyncMock(side_effect=sent_key), click=AsyncMock())
        checker = IcpPlusChecker(Settings()); checker._assist_guard = object()
        if navigated:
            await checker._activate_control(locator, timeout=5000)
            frame.wait_for_load_state.assert_awaited_once()
        else:
            with pytest.raises(PlaywrightTimeoutError, match='Locator.press'):
                await checker._activate_control(locator, timeout=5000)
        locator.press.assert_awaited_once()
        locator.click.assert_not_awaited()
    asyncio.run(scenario())
