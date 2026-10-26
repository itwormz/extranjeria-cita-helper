"""Passive script archival in an isolated browser with all website traffic intercepted."""
import asyncio,json,os
from types import SimpleNamespace
import pytest
from app.config import Settings
from app.site import IcpPlusChecker
from app.assist_control import AssistControl
from app.diagnostics import PageTrace
from app.assist import _document_key
from tests.browser_support import launch_browser

pytestmark=pytest.mark.skipif(os.getenv('CITA_BROWSER_TESTS')!='1',reason='opt-in browser test')

@pytest.mark.parametrize('manual',[False,True])
def test_browser_loaded_js_is_deduplicated_and_linked_without_extra_fetches(tmp_path,monkeypatch,manual):
    monkeypatch.setenv('DEBUG_DIR',str(tmp_path))
    async def run():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw)
            try:
                page=await browser.new_page();requests=[]
                async def respond(route):
                    requests.append(route.request.url)
                    if '/resources/js/' in route.request.url:
                        await route.fulfill(body='window.captureDemo=true;',content_type='application/javascript')
                    else:
                        await route.fulfill(body='<script src="/icpplustieb/resources/js/demo.js?v=1"></script><p>No hay citas disponibles</p>',content_type='text/html')
                await page.route('**/*',respond)
                checker=IcpPlusChecker(Settings(diagnostics_enabled=True))
                control=AssistControl();control.set_diagnostics(True)
                control.publish(screen='Test',evidence='',next_action='',mode='manual' if manual else 'working')
                checker._assist_control=control
                trace=PageTrace(page,checker,'demo','run',_document_key);trace.start()
                try:
                    url='https://icp.administracionelectronica.gob.es/icpplustieb/acCitar'
                    for _ in range(2):
                        await page.goto(url,wait_until='load')
                    async def ready():
                        for _ in range(100):
                            files=list(tmp_path.rglob('resources.json'))
                            if files and json.loads(files[0].read_text())[0]['load_count']==2:return
                            await asyncio.sleep(.05)
                        raise AssertionError('script archive not ready')
                    await ready()
                    assert await checker._save_debug(page,'demo','run')
                finally:await trace.stop()
                assert len(list((tmp_path/'resources').glob('*.js.gz')))==1
                assert len([url for url in requests if '/resources/js/' in url])==2
                captures=[json.loads(p.read_text()) for p in tmp_path.rglob('*.json') if p.name!='resources.json']
                assert any(record.get('resources') and record['resources'][0]['sha256'] for record in captures)
                if manual:assert not list(tmp_path.rglob('*.png'))
            finally:await browser.close()
    asyncio.run(run())
