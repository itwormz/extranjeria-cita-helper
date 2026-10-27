"""Offline browser acceptance for public notifications and applicant modes."""
import asyncio
import os
from types import SimpleNamespace
from unittest.mock import AsyncMock
from urllib.parse import parse_qs
import pytest
from tests.browser_support import launch_browser
from tests.test_local_assist_browser import runner_for, BASE

pytestmark=pytest.mark.skipif(os.getenv('CITA_BROWSER_TESTS')!='1',reason='opt-in browser tests')


@pytest.mark.parametrize('radio_name', ['rdbCita', 'rdbCITA'])
def test_booking_radio_details_are_passive_and_exclude_filters_hidden_and_disabled_offers(radio_name):
    async def scenario():
        from playwright.async_api import async_playwright, expect
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            html='''<label for="cita1">CITA 1 Día: 28/10/2026 Hora:12:30</label><input type="radio" name="rdbCita" id="cita1" value="PRIVATE-TOKEN">
                <label for="cita2">CITA 2 Día: 29/10/2026 Hora:12:06</label><input type="radio" name="rdbCita" id="cita2">
                <div hidden><label for="hidden">Día: 30/10/2026 Hora:11:00</label><input type="radio" name="rdbCita" id="hidden"></div>
                <label for="disabled">Día: 30/10/2026 Hora:11:00</label><input disabled type="radio" name="rdbCita" id="disabled">
                <select id="txtHora"><option>a partir de las 09:00</option></select>
                <input id="txtCaptcha" value="PRIVATE-CAPTCHA"><input type="hidden" name="txtIdCita" value="PRIVATE-CONFIRMATION">
                <div style="height:2000px"></div>'''
            html = html.replace('name="rdbCita"', f'name="{radio_name}"')
            calls=[]
            async def respond(route):calls.append(route.request.url);await route.fulfill(body=html,content_type='text/html; charset=utf-8')
            await page.route('**/*',respond);await page.goto(BASE+'acCita')
            await expect(page.locator('#cita1')).to_be_visible(timeout=10_000)
            await page.evaluate('window.scrollTo(0, 150)')
            before=await page.evaluate('({scroll:window.scrollY,focus:document.activeElement.tagName})')
            from app.booking_details import read_booking_details
            # This test verifies extraction correctness, not the production
            # handoff deadline. Slow extraction is an allowed generic fallback
            # in the runner, so exercise that contract separately without a VM.
            details=await read_booking_details(page,office='CNP TEST, CALLE TEST, 1',timeout=10)
            assert details=={'office':'CNP TEST, CALLE TEST, 1','offers':[
                {'date':'2026-10-28','time':'12:30'},{'date':'2026-10-29','time':'12:06'}]}
            assert before==await page.evaluate('({scroll:window.scrollY,focus:document.activeElement.tagName})')
            assert await page.locator('#txtCaptcha').input_value()=='PRIVATE-CAPTCHA'
            assert await page.locator('input[type=radio]:checked').count()==0
            assert len(calls)==1
            await browser.close()
    asyncio.run(scenario())


@pytest.mark.parametrize('fill_mode',['manual','copy'])
def test_continue_mode_fills_warning_form_and_submits_once(fill_mode):
    async def scenario():
        from playwright.async_api import async_playwright, expect
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page();submitted=[]
            async def respond(route):
                if route.request.url.endswith('acEntrada'):
                    body='''<form method="post" action="acValidarEntrada"><p>En este momento no hay citas disponibles</p>
                        <input id="txtIdCitado" name="document"><input id="txtDesCitado" name="name">
                        <select id="txtPaisNac" name="nationality"><option value="ES">España</option></select>
                        <button id="btnEnviar">Aceptar</button></form>'''
                    if fill_mode=='copy':body+='''<button id="btnCopiar" onclick="document.getElementById('txtIdCitado').value='X0000000T';document.getElementById('txtDesCitado').value='Test Applicant'">Copiar</button>'''
                else:
                    submitted.append(parse_qs(route.request.post_data));body='<form><button id="btnEnviar">Solicitar cita</button></form>'
                await route.fulfill(body=body,content_type='text/html; charset=utf-8')
            await page.route('**/*',respond)
            r=runner_for(page,ctx);r.task=SimpleNamespace(id='test',document_number='X0000000T',full_name='Test Applicant',nationality='España',applicant_fill_mode=fill_mode,applicant_no_slots_mode='continue',no_slots_mode='salir')
            r.countdown=AsyncMock()
            await page.goto(BASE+'acEntrada')
            await expect(page.locator('#txtIdCitado')).to_be_visible(timeout=10_000)
            await r.step()
            assert submitted==[{'document':['X0000000T'],'name':['Test Applicant'],'nationality':['ES']}]
            r.countdown.assert_not_awaited()
            assert not r.control.booking_handoff
            r.publish(await r.observe())
            journal=next(row for row in r.control.snapshot()['journal'] if row['stage']=='applicant_no_slots')
            assert journal['status']=='passed' and journal['message']=='Данные заявителя отправлены → {destination}'
            await browser.close()
    asyncio.run(scenario())


@pytest.mark.parametrize('human_delay', [0, 1.1])
def test_human_attention_during_detail_read_is_not_lost(monkeypatch,human_delay):
    async def scenario():
        from playwright.async_api import async_playwright, expect
        from app import booking_details
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            await page.route('**/*',lambda route:route.fulfill(body='<input name="fecha"><input id="captcha">',content_type='text/html'))
            await page.goto(BASE+'acCita')
            await expect(page.locator('input[name=fecha]')).to_be_visible(timeout=10_000)
            r=runner_for(page,ctx)
            human_actions=[]
            async def read(*args,**kwargs):
                # A real person's click is independent of the passive reader.
                # Cancelling that reader must not cancel the test's input action.
                async def human_input():
                    await asyncio.sleep(human_delay)
                    await page.locator('#captcha').click(timeout=10_000)
                action=asyncio.create_task(human_input())
                human_actions.append(action)
                await asyncio.shield(action)
                return {}
            monkeypatch.setattr(booking_details,'read_booking_details',read)
            try:
                await r.step()
                assert len(human_actions)==1
                await human_actions[0]
                await expect(page.locator('html')).to_have_attribute('data-cita-human-attention','1',timeout=10_000)
                # Poll the manual loop rather than assuming a single bounded DOM
                # probe always completes on a loaded Windows runner.
                deadline=asyncio.get_running_loop().time()+10
                while not r.booking_acknowledged and asyncio.get_running_loop().time()<deadline:
                    await r.step()
                assert r.booking_acknowledged
                assert r.attention is None
                assert r.control.booking_handoff and r.checker._assist_guard is None
            finally:
                for action in human_actions:
                    if not action.done():action.cancel()
                await asyncio.gather(*human_actions,return_exceptions=True)
                await browser.close()
    asyncio.run(scenario())
