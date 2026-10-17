"""Opt-in macOS window-order regression; synthetic page and disposable profile."""
import asyncio
import os
import subprocess
import sys
from types import SimpleNamespace

import pytest
from app.config import Settings
from app.site import IcpPlusChecker

pytestmark = pytest.mark.skipif(sys.platform != 'darwin' or os.getenv('CITA_FOCUS_TESTS') != '1', reason='opt-in native macOS focus test')


def test_province_selection_does_not_raise_background_browser(tmp_path):
    def js(code):
        return subprocess.check_output(['/usr/bin/osascript', '-l', 'JavaScript', '-e', 'ObjC.import("AppKit"); '+code],text=True).strip()
    original=int(js('$.NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier'))
    def top_window():
        return int(js('ObjC.import("CoreGraphics"); ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo(1,0))).filter(x=>x.kCGWindowLayer===0)[0].kCGWindowOwnerPID'))
    def restore():
        js('$.NSRunningApplication.runningApplicationWithProcessIdentifier('+str(original)+').activateWithOptions(2)')
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            checker=IcpPlusChecker(Settings(browser_engine='camoufox',browser_session='persistent',browser_profile_directory=str(tmp_path),browser_action_delay_seconds=(0,0)))
            task=SimpleNamespace(id='focus',schedule=SimpleNamespace(timezone='Europe/Madrid'))
            context=await checker.launch_context(pw,task,force_headed=True)
            try:
                page=await checker.page_for_context(context)
                await page.route('**/*',lambda route:route.fulfill(body='<input id="sentinel"><select id="form" onchange="document.body.dataset.changed=this.value"><option value="28">Madrid</option><option value="08">Barcelona</option></select>',content_type='text/html; charset=utf-8'))
                await page.goto('https://icp.administracionelectronica.gob.es/icpplustieb/index.html')
                await page.evaluate("document.getElementById('sentinel').focus()")
                restore();await asyncio.sleep(.5)
                assert top_window()==original, 'could not place test browser behind the original application'
                assert await page.evaluate('document.activeElement.id')=='sentinel', 'sentinel did not acquire focus before selection'
                await checker._select_label(page,'#form','Barcelona','request.province')
                await asyncio.sleep(.5)
                assert await page.locator('#form').input_value()=='08'
                assert await page.locator('body').get_attribute('data-changed')=='08'
                assert await page.evaluate('document.activeElement.id')=='sentinel', 'selecting a province moved keyboard focus'
                assert top_window()==original, 'selecting a province raised Camoufox above the working application'
            finally:
                await checker.close_context(context)
                restore()
    asyncio.run(scenario())
