"""Offline forms based on public controls in the private regreso capture."""
import asyncio
import os
from dataclasses import replace
from types import SimpleNamespace
from urllib.parse import parse_qs
import pytest
from tests.browser_support import launch_browser, save_failure_trace, start_failure_trace
from tests.test_local_assist_browser import runner_for, BASE
from app.site import SiteCheckError

pytestmark=pytest.mark.skipif(os.getenv('CITA_BROWSER_TESTS')!='1',reason='opt-in browser tests')
REGRESO='AUTORIZACIÓN DE REGRESO'

@pytest.mark.parametrize('kind,number,mode,response_delay',[('NIE','X0000000T','manual',0),('DNI','00000000T','manual',0),('PASSPORT','PA123456789012','manual',0),('DNI','00000000T','copy',0),('DNI','00000000T','manual',5.5)])
def test_regreso_applicant_submits_selected_document_without_nationality(kind,number,mode,response_delay):
    """Submit once, including a response later than the pointer action deadline."""
    async def scenario():
        from playwright.async_api import async_playwright, expect
        async with async_playwright() as pw:
            browser=await launch_browser(pw)
            traced=False
            try:
                ctx=await browser.new_context();page=await ctx.new_page();submitted=[]
                traced=await start_failure_trace(ctx)
                body='''<button id="btnCopiar" onclick="document.getElementById('rdbTipoDocDni').checked=true;for(const [id,v] of [['txtIdCitado','00000000T'],['txtDesCitado','AUTH EXAMPLE']]){const e=document.getElementById(id);e.value=v;e.readOnly=true}">Copiar a datos solicitante</button>
                <form action="acValidarEntrada" method="post">
                <input type="radio" id="rdbTipoDocNie" name="rdbTipoDoc" value="N.I.E." checked onclick="document.getElementById('txtIdCitado').maxLength=9">
                <input type="radio" id="rdbTipoDocDni" name="rdbTipoDoc" value="D.N.I." onclick="document.getElementById('txtIdCitado').maxLength=9">
                <input type="radio" id="rdbTipoDocPas" name="rdbTipoDoc" value="PASAPORTE" onclick="document.getElementById('txtIdCitado').maxLength=20">
                <input id="txtIdCitado" name="txtIdCitado" maxlength="9"><input id="txtDesCitado" name="txtDesCitado">
                <button id="btnEnviar">Aceptar</button></form>'''
                async def respond(route):
                    if route.request.method=='POST':
                        submitted.append(parse_qs(route.request.post_data))
                        if response_delay:await asyncio.sleep(response_delay)
                        html='<form><button id="btnEnviar">Solicitar cita</button></form>'
                    else:html=body
                    await route.fulfill(body=html,content_type='text/html; charset=utf-8')
                await page.route('**/*',respond)
                r=runner_for(page,ctx)
                effort=[]
                r.control.set_statistics_sink(lambda event,doc,**kwargs:effort.append(event),lambda seconds:None)
                r.task=SimpleNamespace(id='regreso_test',procedure=REGRESO,document_type=kind,document_number=number,full_name='ALEX EXAMPLE',nationality='',applicant_fill_mode=mode)
                await page.goto(BASE+'acEntrada')
                await expect(page.locator('#txtIdCitado')).to_be_visible(timeout=10_000)
                await expect(page.get_by_role('button',name='Aceptar',exact=True)).to_be_enabled(timeout=10_000)
                assert (await r.observe()).kind=='applicant'
                await r.step()
                await expect(page.get_by_role('button',name='Solicitar cita',exact=True)).to_be_visible(timeout=10_000)
                assert submitted==[{'rdbTipoDoc':[{'NIE':'N.I.E.','DNI':'D.N.I.','PASSPORT':'PASAPORTE'}[kind]],'txtIdCitado':[number],'txtDesCitado':['AUTH EXAMPLE' if mode=='copy' else 'ALEX EXAMPLE']}]
                assert effort.count('automation_action')==(2 if mode=='copy' else 3+(kind!='NIE'))
                assert (await r.observe()).kind=='request_menu'
            except Exception:
                if traced:await save_failure_trace(ctx)
                raise
            finally:await browser.close()
    asyncio.run(scenario())

@pytest.mark.parametrize('label',['AUTORIZACIÓN DE REGRESO','POLICIA-AUTORIZACIÓN DE REGRESO'])
def test_regreso_selects_only_exact_regreso_service(label):
    async def scenario():
        from playwright.async_api import async_playwright
        from app.site import IcpPlusChecker
        from app.config import Settings
        async with async_playwright() as pw:
            browser=await launch_browser(pw)
            try:
                page=await browser.new_page()
                await page.set_content('<select id="tramiteGrupo[0]"><option value="0">Seleccione</option><option value="wrong">POLICÍA-RECOGIDA CERTIFICADO Y AUTORIZACIÓN DE REGRESO</option><option value="right">'+label+'</option></select>')
                checker=IcpPlusChecker(Settings(browser_engine='camoufox' if os.getenv('CITA_TEST_ENGINE')=='camoufox' else 'webkit',browser_action_delay_seconds=(0,0)))
                await checker._select_procedure(page,REGRESO)
                assert await page.locator('select').input_value()=='right'
                await page.locator('option[value=right]').evaluate('(e)=>e.remove()')
                checker.settings=replace(checker.settings,navigation_timeout_seconds=5)
                with pytest.raises(SiteCheckError,match='CATALOG_VALUE_NOT_FOUND'):
                    await checker._select_procedure(page,REGRESO)
                assert await page.locator('select').input_value()!='wrong'
            finally:await browser.close()
    asyncio.run(scenario())


@pytest.mark.parametrize('has_radio,maximum',[(False,20),(True,9)])
def test_regreso_does_not_truncate_passport_or_guess_a_missing_radio(has_radio,maximum):
    async def scenario():
        from playwright.async_api import async_playwright
        from app.site import IcpPlusChecker
        from app.config import Settings
        async with async_playwright() as pw:
            browser=await launch_browser(pw)
            try:
                page=await browser.new_page()
                radio='<input type="radio" id="rdbTipoDocPas">' if has_radio else ''
                await page.set_content(radio+'<input id="txtIdCitado" maxlength="'+str(maximum)+'"><input id="txtDesCitado">')
                task=SimpleNamespace(procedure=REGRESO,document_type='PASSPORT',document_number='PA123456789012',full_name='ALEX EXAMPLE',nationality='')
                checker=IcpPlusChecker(Settings(browser_engine='camoufox' if os.getenv('CITA_TEST_ENGINE')=='camoufox' else 'webkit',browser_action_delay_seconds=(0,0)))
                with pytest.raises(SiteCheckError,match='FORM_NOT_READY'):
                    await checker._fill_applicant_manual(page,task,'test')
                assert await page.locator('#txtIdCitado').input_value()==''
                assert await page.locator('#txtDesCitado').input_value()==''
            finally:await browser.close()
    asyncio.run(scenario())
