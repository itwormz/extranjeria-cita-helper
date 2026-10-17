import pytest
import asyncio
from types import SimpleNamespace
from unittest.mock import AsyncMock

from app.diagnostics import PageTrace


def test_trace_saves_new_documents_without_duplicate_poll_snapshots():
    async def scenario():
        listeners = {}
        document = ['one']
        saved = []
        ready = asyncio.Event()
        async def save(*args, **kwargs):
            assert kwargs == {'error_code': 'TRACE_SCREEN', 'include_controls': True}
            saved.append(document[0])
            ready.set()
            return True
        page = SimpleNamespace(url='https://example.test/acCitar', locator=lambda selector: SimpleNamespace(inner_text=AsyncMock(return_value='stable screen')),
            on=lambda event, callback: listeners.update({event: callback}),
            remove_listener=lambda event, callback: listeners.pop(event), is_closed=lambda: False)
        trace = PageTrace(page, SimpleNamespace(_save_debug=save), 'test', 'run',
                          AsyncMock(side_effect=lambda page: document[0]))
        trace.start()
        await asyncio.wait_for(ready.wait(), timeout=2)
        ready.clear()
        document[0] = 'two'
        listeners['domcontentloaded']()
        await asyncio.wait_for(ready.wait(), timeout=2)
        listeners['domcontentloaded']()
        await asyncio.sleep(0.05)
        await trace.stop()
        assert saved == ['one', 'two']
        assert not listeners
        assert trace.task.done()
    asyncio.run(scenario())


def test_navigation_response_retains_intermediate_html(tmp_path, monkeypatch):
    import json
    from app.site import IcpPlusChecker
    from app.config import Settings
    monkeypatch.setenv("DEBUG_DIR", str(tmp_path))
    checker = IcpPlusChecker(Settings(diagnostics_enabled=True))
    response = SimpleNamespace(url="https://icp.administracionelectronica.gob.es/icpplus/acEntrada",
        status=200, headers={"content-type":"text/html"}, body=AsyncMock(return_value=b"<p>Intermediate screen</p>"),
        request=SimpleNamespace(method="GET", is_navigation_request=lambda: True))
    asyncio.run(checker._save_response(response, "test", "run"))
    assert next(tmp_path.rglob("*.response.html")).read_text() == "<p>Intermediate screen</p>"
    meta=json.loads(next(tmp_path.rglob("*.response.json")).read_text())
    assert meta["status"]==200 and meta["method"]=="GET"


def test_auth_navigation_during_snapshot_never_writes_auth_content(tmp_path,monkeypatch):
    from app.site import IcpPlusChecker
    from app.config import Settings
    monkeypatch.setenv("DEBUG_DIR",str(tmp_path))
    checker=IcpPlusChecker(Settings(diagnostics_enabled=True))
    page=SimpleNamespace(url="https://icp.administracionelectronica.gob.es/icpplus/acEntrada",
        evaluate=AsyncMock(return_value="document-one"),content=AsyncMock(return_value="AUTH-PRIVATE-MATERIAL"))
    options=[]
    async def screenshot(**kwargs):
        options.append(kwargs)
        page.url="https://pasarela.clave.gob.es/Login"
        return b"private-image"
    page.screenshot=screenshot
    assert not asyncio.run(checker._save_debug(page,"test","run"))
    assert options[0]['full_page'] is False
    assert not list(tmp_path.rglob("*.html")) and not list(tmp_path.rglob("*.png"))


@pytest.mark.parametrize("mode", ["manual", "paused"])
def test_manual_capture_does_not_touch_browser_viewport(tmp_path, monkeypatch, mode):
    from app.site import IcpPlusChecker
    from app.config import Settings
    from app.assist_control import AssistControl
    monkeypatch.setenv("DEBUG_DIR", str(tmp_path))
    checker=IcpPlusChecker(Settings(diagnostics_enabled=True))
    control=AssistControl()
    control.publish(screen="Выбор ситы", evidence="", next_action="", mode=mode)
    checker._assist_control=control
    page=SimpleNamespace(url="https://icp.administracionelectronica.gob.es/icpplus/acOfertarCita",
        evaluate=AsyncMock(return_value="document-one"), content=AsyncMock(return_value="<p>Manual booking</p>"),
        screenshot=AsyncMock(return_value=b"image"))
    assert asyncio.run(checker._save_debug(page,"test","run"))
    page.screenshot.assert_not_awaited()
    assert next(tmp_path.rglob("*.html")).read_text()=="<p>Manual booking</p>"
    assert not list(tmp_path.rglob("*.png"))


@pytest.fixture(autouse=True)
def deterministic_unlocked_desktop(monkeypatch):
    # Unit mocks model an unlocked desktop; dedicated lock tests override this.
    from app import desktop_session
    monkeypatch.setattr(desktop_session,'screen_locked',lambda:False)
