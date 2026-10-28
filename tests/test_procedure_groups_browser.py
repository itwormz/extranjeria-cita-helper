"""Offline regression for province-specific service groups; no live requests."""
import asyncio
from html import escape
import os

import pytest
from tests.browser_support import launch_browser
from tests.test_local_assist_browser import runner_for, BASE
from app.config import TIE_PROCEDURE
from app.site import SiteCheckError

pytestmark = pytest.mark.skipif(os.getenv('CITA_BROWSER_TESTS') != '1', reason='opt-in browser tests')


@pytest.mark.parametrize('group,hidden_first', [(0, False), (1, False), (3, True)])
def test_procedure_selects_exact_service_in_visible_group(group, hidden_first):
    async def scenario():
        from playwright.async_api import async_playwright, expect
        async with async_playwright() as pw:
            browser = await launch_browser(pw)
            context = await browser.new_context()
            page = await context.new_page()
            groups = ''
            for number in range(group + 1):
                hidden = ' style="display:none"' if hidden_first and number == 0 else ''
                options = ('<option value="4010">' + escape(TIE_PROCEDURE) + '</option>'
                           if number == group else '<option value="4">SOLICITUD DE AUTORIZACIONES INICIALES</option>')
                groups += (f'<select id="tramiteGrupo[{number}]"{hidden} onchange="document.body.dataset.changed=this.id">'
                           '<option value="-1">Seleccione</option>' + options + '</select>')
            html = ('<select id="sede"><option value="99">Cualquier oficina</option>'
                    '<option value="2">CNP HUELLAS</option></select>' + groups +
                    '<select id="subtramite" style="display:none"></select>'
                    '<input id="btnAceptar" type="button" value="Aceptar" onclick="document.body.dataset.accepted=Number(document.body.dataset.accepted || 0)+1">')
            await page.route('**/*', lambda route: route.fulfill(body=html, content_type='text/html; charset=utf-8'))
            await page.goto(BASE + 'citar')
            await expect(page.locator('#sede')).to_be_visible(timeout=10_000)
            await expect(page.locator('#btnAceptar')).to_be_visible(timeout=10_000)
            runner = runner_for(page, context)
            runner.task.procedure = TIE_PROCEDURE
            observed = await runner.observe()
            assert observed.kind == 'procedure'
            await runner.act(observed)
            assert await page.locator(f'[id="tramiteGrupo[{group}]"]').input_value() == '4010'
            assert await page.locator('#sede').input_value() == '99'
            assert await page.locator('body').get_attribute('data-changed') == f'tramiteGrupo[{group}]'
            assert await page.locator('body').get_attribute('data-accepted') == '1'
            for number in range(group):
                assert await page.locator(f'[id="tramiteGrupo[{number}]"]').input_value() == '-1'
            await browser.close()
    asyncio.run(scenario())


def test_missing_service_never_accepts_or_selects_an_office():
    async def scenario():
        from playwright.async_api import async_playwright, expect
        async with async_playwright() as pw:
            browser = await launch_browser(pw)
            context = await browser.new_context()
            page = await context.new_page()
            html = ('<select id="sede"><option value="99">Cualquier oficina</option><option value="4010">' +
                    escape(TIE_PROCEDURE) + '</option></select>'
                    '<select id="tramiteGrupo[0]"><option value="-1">Seleccione</option><option value="4">Otra gestión</option></select>'
                    '<input id="btnAceptar" type="button" onclick="document.body.dataset.accepted=Number(document.body.dataset.accepted || 0)+1">')
            await page.route('**/*', lambda route: route.fulfill(body=html, content_type='text/html; charset=utf-8'))
            await page.goto(BASE + 'citar')
            await expect(page.locator('#sede')).to_be_visible(timeout=10_000)
            await expect(page.locator('#btnAceptar')).to_be_visible(timeout=10_000)
            runner = runner_for(page, context)
            runner.task.procedure = TIE_PROCEDURE
            with pytest.raises(SiteCheckError) as error:
                await runner.act(await runner.observe())
            assert error.value.code == 'CATALOG_VALUE_NOT_FOUND'
            assert await page.locator('#sede').input_value() == '99'
            assert await page.locator('body').get_attribute('data-accepted') is None
            await browser.close()
    asyncio.run(scenario())
