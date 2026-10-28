import asyncio
import os
from types import SimpleNamespace

import pytest

from app.site import IcpPlusChecker
from app.config import Settings
from tests.browser_support import launch_browser

pytestmark = pytest.mark.skipif(os.getenv('CITA_BROWSER_TESTS') != '1', reason='opt-in local browser tests')


def test_fallback_contact_fields_are_distinct_dom_elements():
    async def scenario():
        from playwright.async_api import async_playwright, expect
        async with async_playwright() as pw:
            browser = await launch_browser(pw)
            page = await browser.new_page()
            checker = IcpPlusChecker(Settings())
            task = SimpleNamespace(id='test', contact_phone='600000000', contact_email='person@example.invalid')
            await page.set_content('<input type="tel"><input type="email" id="correoUno">')
            await expect(page.locator('input[type=tel]')).to_be_visible(timeout=10_000)
            assert await checker._fill_contact(page, task, 'test') is False
            await page.set_content('<input type="tel"><input type="email" id="correoUno">'
                                   '<input type="email" id="correoDos">')
            await expect(page.locator('#correoDos')).to_be_visible(timeout=10_000)
            assert await checker._fill_contact(page, task, 'test') is True
            assert await page.locator('#correoUno').input_value() == 'person@example.invalid'
            assert await page.locator('#correoDos').input_value() == 'person@example.invalid'
            await browser.close()
    asyncio.run(scenario())
