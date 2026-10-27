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


def test_booking_radio_details_are_passive_and_exclude_filters_hidden_and_disabled_offers(tmp_path):
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
            calls=[]
            async def respond(route):calls.append(route.request.url);await route.fulfill(body=html,content_type='text/html; charset=utf-8')
            await page.route('**/*',respond);await page.goto(BASE+'acCita')
            await expect(page.locator('#cita1')).to_be_visible(timeout=10_000)
            await page.evaluate('window.scrollTo(0, 150)')
            before=await page.evaluate('({scroll:window.scrollY,focus:document.activeElement.tagName})')
            r=runner_for(page,ctx);r.selected_office={'value':'20','text':'CNP TEST, CALLE TEST, 1'}
            from app.desktop.statistics import ProfileStatistics
            from app.desktop.statistics_session import StatisticsSession
            from app.assist_journal import description
            r.task=SimpleNamespace(id='test',province='Barcelona',procedure='huellas',access_mode='certificate')
            statistics=ProfileStatistics(tmp_path/'statistics.json');session=StatisticsSession(statistics,r.task,r.control)
            r.control.set_diagnostics(False);r.control.set_timeline_path(tmp_path/'timeline.jsonl')
            r.current_offices=[r.selected_office,{'value':'21','text':'OTHER OFFICE'}]
            await r.step()
            state=r.control.snapshot()
            assert state['booking_handoff'] and not state['booking_details_pending']
            assert state['booking_details']=={'office':'CNP TEST, CALLE TEST, 1','offers':[
                {'date':'2026-10-28','time':'12:30'},{'date':'2026-10-29','time':'12:06'}]}
            assert before==await page.evaluate('({scroll:window.scrollY,focus:document.activeElement.tagName})')
            assert await page.locator('#txtCaptcha').input_value()=='PRIVATE-CAPTCHA'
            assert await page.locator('input[type=radio]:checked').count()==0
            await r.step();assert len(calls)==1
            session.close('stopped')
            recorded=next(row for row in ProfileStatistics(statistics.path).history('test') if row['kind']=='booking')
            assert recorded['evidence']['booking_details']==state['booking_details']
            assert statistics.get('test')['booking_screens']==1
            journal=next(row for row in r.control.snapshot()['journal'] if row['stage']=='booking')
            assert '28.10.2026 — 12:30' in description(journal)
            log=(tmp_path/'timeline.jsonl').read_text()
            assert 'CNP TEST, CALLE TEST, 1' in log and '28.10.2026 — 12:30' in log
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


def test_human_attention_during_detail_read_is_not_lost(monkeypatch):
    async def scenario():
        from playwright.async_api import async_playwright, expect
        from app import booking_details
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            await page.route('**/*',lambda route:route.fulfill(body='<input name="fecha"><input id="captcha">',content_type='text/html'))
            await page.goto(BASE+'acCita')
            await expect(page.locator('input[name=fecha]')).to_be_visible(timeout=10_000)
            r=runner_for(page,ctx)
            async def read(*args,**kwargs):
                # Simulate actual user attention during the bounded read.
                await page.locator('#captcha').click()
                return {}
            monkeypatch.setattr(booking_details,'read_booking_details',read)
            await r.step();await r.step()
            assert r.booking_acknowledged
            assert r.attention is None
            await browser.close()
    asyncio.run(scenario())
