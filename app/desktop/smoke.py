"""Offline smoke check for the packaged runtime; never visits the appointment site."""
import asyncio
import json
import os
from pathlib import Path
import tempfile
from dataclasses import replace
from types import SimpleNamespace

from .paths import bundled_browser, resource_root, runtime_root
from .i18n import translate


async def run():
    from playwright.async_api import async_playwright
    from ..config import Settings
    from ..site import IcpPlusChecker
    checker = IcpPlusChecker(Settings(browser_engine='camoufox', browser_mode='headless', browser_session='fresh'))
    task = SimpleNamespace(id='smoke', schedule=SimpleNamespace(timezone='Europe/Madrid'))
    async with async_playwright() as driver:
        context = await checker.launch_context(driver, task)
        try:
            page = await context.new_page()
            await page.route('**/*', lambda route: route.fulfill(
                body='<title>Cita offline check</title><h1>OK</h1>'
                     '<p style="font-family:sans-serif">España: cita, autorización, ñ, áéíóú — €</p>'
                     '<p style="font-family:serif">Кириллица: имя и фамилия</p>'
                     '<input id="demo">',
                content_type='text/html; charset=utf-8'))
            await page.goto('https://icp.administracionelectronica.gob.es/icpplus/smoke')
            assert await page.title() == 'Cita offline check'
            sample = 'TEST Ñ ÁÉÍÓÚ Кириллица X0000000T'
            await page.locator('#demo').fill(sample)
            assert await page.locator('#demo').input_value() == sample
            with tempfile.TemporaryDirectory(prefix='cita-diagnostics-') as directory:
                previous = os.environ.get('DEBUG_DIR')
                os.environ['DEBUG_DIR'] = directory
                checker.settings = replace(checker.settings, diagnostics_enabled=True)
                try:
                    from app.desktop_session import screen_locked
                    locked_before=screen_locked()
                    assert await checker._save_debug(page, 'smoke', 'offline', include_controls=True)
                    images=list(Path(directory).rglob('*.png'))
                    locked_after=screen_locked()
                    if not locked_before and not locked_after:assert images
                    elif locked_before and locked_after:assert not images
                    assert list(Path(directory).rglob('*.html'))
                finally:
                    if previous is None:
                        os.environ.pop('DEBUG_DIR', None)
                    else:
                        os.environ['DEBUG_DIR'] = previous
        finally:
            await checker.close_context(context)
    print(json.dumps(dict(ok=True, browser=bool(bundled_browser()),
                          resources=str(resource_root()), data=str(runtime_root()),
                          spanish=translate('Настройки','es')), ensure_ascii=False), flush=True)


def main():
    asyncio.run(run())
    # Exercise the actual Widgets UI after browser shutdown, without starting
    # a worker or accessing the appointment site.
    from PySide6.QtWidgets import QApplication
    from ..main import _paths
    from .profiles import ProfileStore
    from .process import DesktopController
    from .window import DesktopWindow
    app = QApplication.instance() or QApplication([])
    app.setStyle('Fusion')
    path, catalog, _ = _paths()
    store = ProfileStore(path, catalog)
    window = DesktopWindow(store, DesktopController(store))
    window.show()
    app.processEvents()
    assert window.tabs.count() == 3
    window.tabs.setCurrentIndex(1)
    assert window.statistics_panel.isVisible()
    window.tabs.setCurrentIndex(2)
    window.create_profile()
    app.processEvents()
    assert window.profile_editor.isVisible()
    from .notification_store import NotificationVault
    assert NotificationVault().backend() is not None
    window.profile_tabs.setCurrentWidget(window.notifications_panel.scroll)
    window.notifications_panel.telegram_toggle.setChecked(True)
    app.processEvents()
    assert window.notifications_panel.token_box.height()==48
    assert window.notifications_panel.token.isVisible()
    window.close()
    app.processEvents()
    print(json.dumps({'ok': True, 'gui': True}), flush=True)
    return 0
