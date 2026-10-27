"""Offline acceptance of the new process-owned assist loop."""
import asyncio
import json
import os
import sys
from types import SimpleNamespace
from unittest.mock import AsyncMock

import pytest
from app.assist_control import AssistControl
from app.assist_flow import AssistRunner
from app.config import Settings
from app.site import IcpPlusChecker
from tests.browser_support import launch_browser

pytestmark = pytest.mark.skipif(os.getenv("CITA_BROWSER_TESTS") != "1", reason="opt-in browser tests")
BASE="https://icp.administracionelectronica.gob.es/icpplus/"
FORM='<form id="citadoForm"><input id="txtIdCitado"><input id="txtDesCitado"><button id="btnEnviar">Aceptar</button></form>'


@pytest.mark.parametrize('variant', ['information', 'province'])
@pytest.mark.parametrize('render_delay_ms', [0, 600])
def test_captured_expiry_variants_follow_own_control(variant, render_delay_ms):
    async def scenario():
        from playwright.async_api import async_playwright, expect
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            text='Su sesión ha caducado por permanecer demasiado tiempo inactiva. Deberás iniciar de nuevo la solicitud de la cita previa.'
            label='Aceptar' if variant=='information' else 'Volver'
            if variant=='province':
                text='La provincia seleccionada no ofrece el servicio de Cita Previa Internet para ningún trámite. '+text
            calls=[]
            async def respond(route):
                calls.append(route.request.url)
                body='<p>'+text+'</p><form action="salirInicio" method="post"><input type="button" value="'+label+'" onclick="this.form.submit()"></form><a href="#">Acepto</a>' if route.request.url.endswith('acInfo') else '<select id="form"><option>Barcelona</option></select>'
                await route.fulfill(body=body,content_type='text/html; charset=utf-8')
            await page.route('**/*',respond)
            r=runner_for(page,ctx);r.expired_attempts=1
            order=[]
            async def countdown(screen,delay,action):
                order.append(('wait',page.url,delay));return True
            r.countdown=countdown
            await page.goto(BASE+'acInfo')
            if render_delay_ms:
                await page.evaluate("""delay => {
                    document.body.style.visibility = 'hidden';
                    setTimeout(() => document.body.style.visibility = '', delay);
                }""", render_delay_ms)
            # load does not guarantee the first rendered body in a cold Firefox
            # process. observe() intentionally reports loading until it is visible.
            await expect(page.locator('body')).to_contain_text(text, use_inner_text=True, timeout=10_000)
            await expect(page.locator('input[type="button"]')).to_be_visible(timeout=10_000)
            screen=await r.observe();assert screen.kind=='expired', screen
            await r.step()
            assert page.url==BASE+'salirInicio'
            assert len(calls)==2
            if variant=='province':
                assert order==[('wait',BASE+'salirInicio',30)]
                assert 'провинция' in screen.evidence.lower()
            else:
                assert order==[('wait',BASE+'acInfo',30)]
            await browser.close()
    asyncio.run(scenario())


def test_request_rejected_is_saved_and_kept_visible_during_cooldown():
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            calls=[]
            async def respond(route):
                calls.append(route.request.url)
                await route.fulfill(body='<title>Request Rejected</title><p>The requested URL was rejected. Please consult with your administrador.</p><p>Your support ID is: &lt;123456&gt;</p><a href="javascript:history.back()">[Go Back]</a>',content_type='text/html')
            await page.route('**/*',respond);r=runner_for(page,ctx);r.save=AsyncMock()
            async def countdown(screen,delay,action):
                r.save.assert_awaited_with(screen,'HTTP_BLOCKED')
                assert delay==30 and await page.locator('body').is_visible()
                return False
            r.countdown=countdown
            await page.goto(BASE+'citar')
            screen=await r.observe()
            assert screen.kind=='blocked' and '123456' in screen.evidence
            await r.step()
            assert calls==[BASE+'citar']
            assert not r.control.restart_requested
            assert any('Request Rejected' in row['detail'] for row in r.control.snapshot()['timeline'])
            await browser.close()
    asyncio.run(scenario())


def runner_for(page, context):
    checker=IcpPlusChecker(Settings(browser_engine='camoufox' if os.getenv('CITA_TEST_ENGINE') == 'camoufox' else 'webkit',
                                   browser_action_delay_seconds=(0,0),diagnostics_enabled=False))
    return AssistRunner(page, context, checker, SimpleNamespace(id="test"), "run", AssistControl(),
                        SimpleNamespace(start=AsyncMock(),stop=AsyncMock()), SimpleNamespace(next_delay=lambda **kw: 0))


def test_http_429_overrides_block_page_and_probes_once_per_cooldown():
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            calls=[]
            async def respond(route):
                calls.append(route.request.method)
                status=429 if len(calls)<3 else 200
                body='Your access is blocked. Too many requests' if status==429 else '<select id="form"><option>Barcelona</option></select>'
                await route.fulfill(status=status,body=body,content_type='text/html')
            await page.route('**/*',respond)
            r=runner_for(page,ctx);r.countdown=AsyncMock(return_value=True)
            await page.goto(BASE+'index.html')
            observed=await r.observe()
            assert observed.kind=='rate_limited' and not observed.operation_code
            await r.step()
            assert calls==['GET','GET'] and (await r.observe()).kind=='rate_limited'
            await r.step()
            assert calls==['GET','GET','GET'] and (await r.observe()).kind=='province'
            assert [c.args[1] for c in r.countdown.await_args_list]==[180,180]
            assert not r.control.restart_requested and r.control.blocked_streak==0
            await browser.close()
    asyncio.run(scenario())


@pytest.mark.parametrize('fill_mode',['manual','copy'])
def test_applicant_warning_reloads_without_identity_until_warning_disappears(fill_mode):
    async def scenario():
        from playwright.async_api import async_playwright
        from urllib.parse import parse_qs
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            calls=[];applicant_data=[]
            async def respond(route):
                path=route.request.url.rsplit('/',1)[-1]
                calls.append((route.request.method,path))
                if path=='acEntrada':
                    body="""<form method="post" action="acValidarEntrada">
                        <input id="txtIdCitado" name="document"><input id="txtDesCitado" name="name">
                        <select id="txtPaisNac" name="nationality"><option value="ES">España</option></select>
                        <button id="btnEnviar">Aceptar</button></form>
                        """
                    if fill_mode=='copy':
                        body+="""<button type="button" id="btnCopiar" onclick="document.getElementById('txtIdCitado').value='X0000000T';document.getElementById('txtDesCitado').value='Test Applicant'">Copiar</button>"""
                    if sum(path==p for _,p in calls)<3:
                        body+='<p>En este momento no hay citas disponibles</p>'
                elif path=='acValidarEntrada':
                    applicant_data.append(parse_qs(route.request.post_data))
                    body='<form method="post" action="acCitar"><button id="btnEnviar">Solicitar cita</button></form>'
                else:
                    body='<p>En este momento no hay citas disponibles</p>'
                await route.fulfill(body=body,content_type='text/html; charset=utf-8')
            await page.route('**/*',respond)
            r=runner_for(page,ctx)
            r.task=SimpleNamespace(id="test",document_number="X0000000T",full_name="Test Applicant",nationality="España",applicant_fill_mode=fill_mode,no_slots_mode='reload')
            effort=[]
            r.control.set_statistics_sink(lambda kind,doc,**kwargs:effort.append(kind),lambda seconds:None)
            r.policy.next_delay=lambda **kwargs:47
            r.countdown=AsyncMock(return_value=True)
            await page.goto(BASE+'acEntrada')
            observed=await r.observe()
            assert observed.kind=='applicant_no_slots'
            for _ in range(2):
                assert await page.locator('#txtIdCitado').input_value()==''
                await r.step()
                assert not applicant_data
                assert r.control.snapshot()['checks']==0
            assert effort.count('automation_action')==2
            assert [call.args[1] for call in r.countdown.await_args_list]==[47,47]
            assert (await r.observe()).kind=='applicant'
            await r.step()
            assert applicant_data==[{'document':['X0000000T'],'name':['Test Applicant'],'nationality':['ES']}]
            assert effort.count('automation_action')==(6 if fill_mode=='manual' else 5)
            assert (await r.observe()).kind=='request_menu'
            await r.step()
            assert (await r.observe()).kind=='request_no_slots'
            await r.step()
            assert calls==[('GET','acEntrada')]*3+[('POST','acValidarEntrada'),('POST','acCitar'),('POST','acCitar')]
            assert r.control.snapshot()['checks']==2
            await browser.close()
    asyncio.run(scenario())


def test_embedded_expiry_uses_its_button_and_saves_frame(tmp_path,monkeypatch):
    monkeypatch.setenv('DEBUG_DIR',str(tmp_path))
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            child='<p>Su sesión ha caducado.</p><button id="recommended" onclick="parent.location.href=\''+BASE+'index\'">Reiniciar</button>'
            import html
            async def respond(route):
                body=FORM+'<iframe srcdoc="'+html.escape(child,quote=True)+'"></iframe>' if 'acEntrada' in route.request.url else '<select id="form"><option>Prueba</option></select>'
                await route.fulfill(body=body,content_type='text/html; charset=utf-8')
            await page.route('**/*',respond);r=runner_for(page,ctx)
            from dataclasses import replace
            r.checker.settings=replace(r.checker.settings,diagnostics_enabled=True)
            await page.goto(BASE+'acEntrada');await page.frames[1].locator('#recommended').wait_for()
            r.checker._fill_applicant_manual=AsyncMock()
            screen=await r.observe();assert screen.kind=='expired' and screen.source!=page.main_frame
            await r.step()
            await page.wait_for_url(BASE+'index')
            r.checker._fill_applicant_manual.assert_not_awaited()
            assert list(tmp_path.rglob('*.frame-1.html'))
            assert 'Su sesión ha caducado' in next(tmp_path.rglob('*.frame-1.html')).read_text()
            await browser.close()
    asyncio.run(scenario())


@pytest.mark.parametrize('count',[1,2])
def test_office_count_controls_auto_transition(count):
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            choices=''.join(f'<option value="{i}">Oficina {i}</option>' for i in range(1,count+1))
            async def respond(route):
                await route.fulfill(body='<select id="idSede"><option value="">Seleccione oficina</option>'+choices+'</select><button id="btnSiguiente" onclick="location.href=\''+BASE+'acVerFormulario\'">Siguiente</button>' if 'acOfertar' in route.request.url else '<input id="txtTelefonoCitado"><input id="emailUNO"><input id="emailDOS">',content_type='text/html; charset=utf-8')
            await page.route('**/*',respond);r=runner_for(page,ctx);await page.goto(BASE+'acOfertarCita')
            await r.step()
            if count==1: assert (await r.observe()).kind=='contact'
            else:
                assert (await r.observe()).kind=='office'
                r.alerter.start.assert_awaited_once()
                assert r.alerter.start.call_args.kwargs['repeat'] is True
            await browser.close()
    asyncio.run(scenario())




def test_live_dom_insertion_is_saved_without_navigation(tmp_path,monkeypatch):
    monkeypatch.setenv('DEBUG_DIR',str(tmp_path))
    async def scenario():
        from playwright.async_api import async_playwright
        from app.assist import _document_key
        from app.diagnostics import PageTrace
        async with async_playwright() as pw:
            browser=await launch_browser(pw);page=await browser.new_page()
            checker=IcpPlusChecker(Settings(browser_action_delay_seconds=(0,0),diagnostics_enabled=True))
            await page.route('**/*',lambda route:route.fulfill(body=FORM,content_type='text/html; charset=utf-8'))
            trace=PageTrace(page,checker,'test','run',_document_key);trace.start()
            await page.goto(BASE+'acEntrada')
            async def wait_capture(marker):
                for _ in range(60):
                    if any(marker in p.read_text(errors='replace') for p in tmp_path.rglob('*.html')):return
                    await asyncio.sleep(.1)
                raise AssertionError('screen not captured')
            await wait_capture('txtIdCitado')
            await page.evaluate("document.body.insertAdjacentHTML('afterbegin','<p>Su sesión ha caducado.</p>')")
            await wait_capture('Su sesión ha caducado')
            await trace.stop()
            assert list(tmp_path.rglob('*.response.html'))
            await browser.close()
    asyncio.run(scenario())


def test_no_slots_after_contacts_exits_instead_of_back():
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            async def respond(route):
                await route.fulfill(body='<p>No hay citas disponibles</p><button id="btnSalir" onclick="location.href=\''+BASE+'index\'">Salir</button>' if 'acVer' in route.request.url else '<select id="form"></select>',content_type='text/html; charset=utf-8')
            await page.route('**/*',respond);r=runner_for(page,ctx);r.last_action='contact'
            await page.goto(BASE+'acVerFormulario');assert (await r.observe()).kind=='post_contact_no_slots'
            await r.step();assert page.url==BASE+'index' and r.last_action is None
            await browser.close()
    asyncio.run(scenario())


def test_booking_sound_requires_real_attention_not_focus():
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            await page.route('**/*',lambda route:route.fulfill(body='<input name="fecha"><button id="pick">Elegir</button>',content_type='text/html; charset=utf-8'))
            r=runner_for(page,ctx);await page.goto(BASE+'acOfertarCita')
            await r.step();r.alerter.start.assert_awaited_once();assert r.alerter.start.call_args.kwargs['repeat']
            await page.bring_to_front();await r.step();r.alerter.stop.assert_not_awaited()
            await page.locator('#pick').click();await r.step();r.alerter.stop.assert_awaited_once()
            await browser.close()
    asyncio.run(scenario())


def test_stopped_browser_keeps_live_page_scrollable_until_user_closes():
    from app.assist_runtime import retain_browser
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            await page.goto('data:text/html,<div style="height:4000px">Appointment</div><input id="answer">')
            control=AssistControl();control.request_stop()
            waiting=asyncio.create_task(retain_browser(page,control))
            await asyncio.sleep(.05)
            await page.evaluate('window.scrollTo(0,1500)')
            assert await page.evaluate('window.scrollY') > 1000
            await page.locator('#answer').fill('user choice')
            assert await page.locator('#answer').input_value() == 'user choice'
            assert not waiting.done() and control.snapshot()['browser_retained']
            await page.close();await asyncio.wait_for(waiting,2)
            await browser.close()
    asyncio.run(scenario())


def test_contact_post_then_rpc_error_still_uses_exit_branch():
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            async def respond(route):
                await route.fulfill(body='<input id="txtTelefonoCitado"><input id="emailUNO"><input id="emailDOS"><button id="btnSiguiente">Siguiente</button>' if 'acVer' in route.request.url else '<p>No hay citas disponibles</p><button>Salir</button>',content_type='text/html; charset=utf-8')
            await page.route('**/*',respond);r=runner_for(page,ctx);r.last_action='office';await page.goto(BASE+'acVerFormulario')
            before=await r.observe();assert before.kind=='contact'
            async def lost_reply(*args,**kw):
                await page.goto(BASE+'acResultado')
                raise RuntimeError('reply lost after commit')
            fake=SimpleNamespace(locator=lambda selector:SimpleNamespace(first=SimpleNamespace(page=page,element_handle=page.locator('#btnSiguiente').element_handle,click=lost_reply,is_visible=AsyncMock(return_value=True),is_enabled=AsyncMock(return_value=True))))
            with pytest.raises(RuntimeError):await r.click(before,'#btnSiguiente',source=fake,action='contact')
            assert (await r.observe()).kind=='post_contact_no_slots'
            await browser.close()
    asyncio.run(scenario())


def test_external_iframe_does_not_break_or_leak_snapshot(tmp_path,monkeypatch):
    monkeypatch.setenv('DEBUG_DIR',str(tmp_path))
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);page=await browser.new_page()
            async def respond(route):
                await route.fulfill(body=FORM+'<iframe src="https://pasarela.clave.gob.es/Login"></iframe>' if route.request.url.startswith(BASE) else '<h1>EXTERNAL_AUTH_PRIVATE</h1>',content_type='text/html; charset=utf-8')
            await page.route('**/*',respond);await page.goto(BASE+'acEntrada');await page.frames[1].locator('h1').wait_for()
            checker=IcpPlusChecker(Settings(diagnostics_enabled=True))
            assert await checker._save_debug(page,'test','run',include_controls=True)
            assert list(tmp_path.rglob('*.png')) and list(tmp_path.rglob('*.html'))
            assert not any('EXTERNAL_AUTH_PRIVATE' in p.read_text() for p in tmp_path.rglob('*.html'))
            await browser.close()
    asyncio.run(scenario())


def test_neutral_http_503_is_recovered_without_form_action(monkeypatch):
    from app import assist_flow
    monkeypatch.setattr(assist_flow,'SERVER_ERROR_PAUSES',(0,0,0,0))
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page();seen=[]
            async def respond(route):
                seen.append(route.request.method)
                await route.fulfill(status=503 if len(seen)==1 else 200,body=FORM if len(seen)==1 else '<select id="form"></select>',content_type='text/html; charset=utf-8')
            await page.route('**/*',respond);r=runner_for(page,ctx);r.checker._fill_applicant_manual=AsyncMock();await page.goto(BASE+'acEntrada')
            assert (await r.observe()).kind=='server_error'
            await r.step();assert (await r.observe()).kind=='province'
            r.checker._fill_applicant_manual.assert_not_awaited();assert seen==['GET','GET']
            await browser.close()
    asyncio.run(scenario())


def test_popup_screens_are_traced_in_same_session(tmp_path,monkeypatch):
    monkeypatch.setenv('DEBUG_DIR',str(tmp_path))
    async def scenario():
        from playwright.async_api import async_playwright
        from app.assist import _document_key
        from app.diagnostics import PageTrace
        async with async_playwright() as pw:
            browser=await launch_browser(pw);page=await browser.new_page()
            async def respond(route):
                body='<button onclick="window.open(\''+BASE+'popup\')">Open</button>' if route.request.url.endswith('index') else '<h1>CHILD_STAGE_CAPTURED</h1>'
                await route.fulfill(body=body,content_type='text/html; charset=utf-8')
            await page.route('**/*',respond)
            # Routes on context also apply to the popup's first request.
            await page.context.route('**/*',respond)
            trace=PageTrace(page,IcpPlusChecker(Settings(diagnostics_enabled=True)),'test','run',_document_key);trace.start()
            await page.goto(BASE+'index')
            async with page.expect_popup() as popup_info:await page.locator('button').click()
            popup=await popup_info.value;await popup.locator('h1').wait_for()
            for _ in range(50):
                if any('CHILD_STAGE_CAPTURED' in p.read_text(errors='replace') for p in tmp_path.rglob('*.html')):break
                await asyncio.sleep(.1)
            assert any('CHILD_STAGE_CAPTURED' in p.read_text(errors='replace') for p in tmp_path.rglob('*.html'))
            await trace.stop();await browser.close()
    asyncio.run(scenario())


def test_inline_expiry_uses_message_button_not_surrounding_form(tmp_path,monkeypatch):
    monkeypatch.setenv('DEBUG_DIR',str(tmp_path))
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            body='<form id="citadoForm"><input id="txtIdCitado"><button id="btnEnviar">Aceptar</button><div id="error-block"><p>Su sesión ha caducado.</p><button type="button" onclick="location.href=\''+BASE+'index\'">Reiniciar</button></div></form>'
            async def respond(route):await route.fulfill(body=body if 'acEntrada' in route.request.url else '<select id="form"></select>',content_type='text/html; charset=utf-8')
            await page.route('**/*',respond);r=runner_for(page,ctx);await page.goto(BASE+'acEntrada')
            assert (await r.observe()).kind=='expired'
            await r.step()
            assert page.url==BASE+'index'
            await browser.close()
    asyncio.run(scenario())


def test_procedure_warning_is_not_a_request_result():
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            await page.route('**/*',lambda route:route.fulfill(body='<select id="tramiteGrupo[0]"><option>Toma de huellas</option></select><p>En este momento no hay citas disponibles en esta sede.</p><input id="btnAceptar" type="button" value="Aceptar">',content_type='text/html'))
            r=runner_for(page,ctx);await page.goto(BASE+'citar')
            assert (await r.observe()).kind=='procedure'
            assert r.last_action is None
            await browser.close()
    asyncio.run(scenario())


def test_startup_uses_trusted_mouse_click():
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            # Require a real pointer click rather than Enter or a DOM click.
            async def respond(route):
                body='<select id="form"><option>Barcelona</option></select><input id="btnAceptar" type="button" value="Aceptar" onclick="if(event.isTrusted &amp;&amp; event.detail===1) location.href=\''+BASE+'citar\'">' if route.request.url.endswith('index') else '<select id="tramiteGrupo[0]"></select>'
                await route.fulfill(body=body,content_type='text/html; charset=utf-8')
            await page.route('**/*',respond);r=runner_for(page,ctx);r.task.province='Barcelona'
            await page.goto(BASE+'index');await r.step()
            assert page.url==BASE+'citar'
            await browser.close()
    asyncio.run(scenario())


def test_clave_card_activates_in_background_site_tab():
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page();panel=await ctx.new_page()
            async def respond(route):
                await route.fulfill(body='<div id="btnAccesoClave"><p>Presentación con Cl@ve</p></div><script>document.getElementById("btnAccesoClave").addEventListener("click",event=>{if(event.isTrusted && event.detail===1) location.href="'+BASE+'acEntrada"})</script>' if route.request.url.endswith('acInfo') else FORM+'<button id="btnCopiar">Copiar</button>',content_type='text/html; charset=utf-8')
            await page.route('**/*',respond);r=runner_for(page,ctx);await page.goto(BASE+'acInfo')
            await panel.goto('data:text/html,<h1>Panel</h1>');await panel.bring_to_front()
            observed=await r.observe();assert observed.kind=='auth'
            assert await r.checker._enter_with_certificate(page,r.task,r.run_id)=='silent'
            await page.wait_for_url(BASE+'acEntrada',timeout=5_000)
            assert page.url==BASE+'acEntrada'
            await browser.close()
    asyncio.run(scenario())



@pytest.mark.parametrize('render_delay_ms', [0, 1500])
@pytest.mark.parametrize('next_kind', ['request_no_slots', 'expired', 'office'])
def test_no_slots_reload_post_recognizes_next_screen(next_kind, render_delay_ms):
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser = await launch_browser(pw)
            ctx = await browser.new_context()
            page = await ctx.new_page()
            menu, result = BASE + 'acValidarEntrada', BASE + 'acCitar'
            posts = []
            bodies = {'request_no_slots': '<p>No hay citas disponibles</p>', 'expired': '<p>Su sesión ha caducado.</p><button>Reiniciar</button>', 'office': '<select id="idSede"><option value="1">Oficina</option></select>'}
            async def respond(route):
                if route.request.url == menu:
                    body = '<form action="' + result + '" method="post"><button id="btnEnviar">Solicitar cita</button></form>'
                else:
                    if route.request.method == 'POST':
                        posts.append(route.request.url)
                    body = bodies['request_no_slots'] if len(posts) == 1 else bodies[next_kind]
                await route.fulfill(body=body, content_type='text/html; charset=utf-8')
            await page.route('**/*', respond)
            r = runner_for(page, ctx)
            r.task.no_slots_mode = 'reload'
            await page.goto(menu)
            if render_delay_ms:
                await page.evaluate("""delay => {
                    document.body.style.visibility = 'hidden';
                    setTimeout(() => document.body.style.visibility = '', delay);
                }""", render_delay_ms)
            # A cold renderer may report loading on the first engine tick.
            # Drive the loop until the first request is dispatched, then stop:
            # another tick on the result would already start the reload.
            for _ in range(10):
                await r.step()
                if r.last_action == 'request':
                    break
            assert r.last_action == 'request'
            await page.wait_for_url(result)
            await page.locator('p').wait_for(state='visible')
            assert (await r.observe()).kind == 'request_no_slots'
            await r.step()
            assert posts == [result, result]
            assert r.control.snapshot()['requests'] == 2
            await page.locator('#idSede' if next_kind == 'office' else 'p').wait_for(state='visible')
            assert (await r.observe()).kind == next_kind
            await browser.close()
    asyncio.run(scenario())


def test_post_contact_no_slots_acceptar_restarts():
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            async def respond(route):
                body='<p>No hay citas disponibles</p><form action="'+BASE+'index.html" method="post"><input id="btnSubmit" type="button" value="Aceptar" onclick="this.form.submit()"></form>' if route.request.url.endswith('acOfertarCita') else '<select id="form"><option>Barcelona</option></select>'
                await route.fulfill(body=body,content_type='text/html; charset=utf-8')
            await page.route('**/*',respond);r=runner_for(page,ctx);r.last_action='contact'
            await page.goto(BASE+'acOfertarCita');await r.step()
            await page.wait_for_url(BASE+'index.html',timeout=5000)
            assert (await r.observe()).kind=='province'
            await browser.close()
    asyncio.run(scenario())


def test_no_slots_reload_returns_province_at_same_url_and_logs_restart():
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            searches=0
            async def respond(route):
                nonlocal searches
                if route.request.url.endswith('acValidarEntrada'):
                    body='<form action="acCitar" method="post"><button id="btnEnviar">Solicitar cita</button></form>'
                else:
                    searches+=1
                    body='<p>En este momento no hay citas disponibles</p>' if searches==1 else '<select id="form"><option>Barcelona</option></select>'
                await route.fulfill(body=body,content_type='text/html; charset=utf-8')
            await page.route('**/*',respond);r=runner_for(page,ctx);r.task.no_slots_mode='reload'
            await page.goto(BASE+'acValidarEntrada')
            # A renderer may still be loading on the first engine tick.
            # Stop driving once the request was sent, then await its document.
            for _ in range(10):
                await r.step()
                if r.last_action == 'request':
                    break
            assert r.last_action == 'request'
            await page.wait_for_url(BASE+'acCitar')
            await page.locator('p').wait_for(state='visible')
            assert searches == 1
            assert (await r.observe()).kind=='request_no_slots'
            await r.step()
            await page.locator('#form').wait_for(state='visible')
            observed=await r.observe()
            assert searches == 2
            assert observed.kind=='province' and page.url==BASE+'acCitar'
            r.publish(observed);r.publish(observed)
            rows=r.control.snapshot()['timeline']
            restarts=[row for row in rows if row['event']=='flow_restart']
            assert len(restarts)==1
            assert 'HTTP 200' in restarts[0]['detail']
            assert 'причине не распознано' in restarts[0]['detail']
            responses=[row for row in rows if row['event']=='response']
            assert 'POST → HTTP 200' in responses[-1]['detail']
            assert not any(row['stage'] in {'expired','server_error'} for row in rows)
            await browser.close()
    asyncio.run(scenario())


def test_stability_fallback_uses_trusted_keyboard_activation():
    async def scenario():
        from playwright.async_api import async_playwright, TimeoutError as PlaywrightTimeoutError
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            async def respond(route):
                body='<select id="form"><option>Barcelona</option></select><input type="button" id="btnAceptar" value="Aceptar" onclick="if(event.isTrusted)location.href=\'citar?trusted=1\'">' if route.request.url.endswith('index') else '<select id="tramiteGrupo[0]"><option>Prueba</option></select>'
                await route.fulfill(body=body,content_type='text/html; charset=utf-8')
            await page.route('**/*',respond);r=runner_for(page,ctx)
            original=r.checker._activate_control;calls=0
            async def delayed(locator,**kwargs):
                nonlocal calls
                calls+=1
                if calls==1:raise PlaywrightTimeoutError('waiting for element to be visible, enabled and stable')
                assert kwargs.get('keyboard') is True
                await original(locator,**kwargs)
            r.checker._activate_control=delayed
            await page.goto(BASE+'index')
            assert await r.click(await r.observe(),'#btnAceptar',action='province')
            assert page.url==BASE+'citar?trusted=1' and calls==2
            assert any(row['event']=='interaction_retry' for row in r.control.snapshot()['timeline'])
            await browser.close()
    asyncio.run(scenario())


def test_no_slots_accept_restart_does_not_replay_search_post():
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            calls=[]
            async def respond(route):
                calls.append((route.request.method, route.request.url))
                body=('<p>En este momento no hay citas disponibles.</p><p>Cod. Oper.: TEST</p>'
                      '<form action="salirInicio" method="post"><input id="btnSubmit" type="button" value="Aceptar" onclick="this.form.submit()"></form>') if route.request.url.endswith('acCitar') else '<select id="form"><option>Barcelona</option></select>'
                await route.fulfill(body=body,content_type='text/html; charset=utf-8')
            await page.route('**/*',respond);r=runner_for(page,ctx)
            r.countdown=AsyncMock(return_value=True)
            await page.goto(BASE+'acCitar')
            observed=await r.observe()
            assert observed.kind == 'request_no_slots_restart'
            await r.step()
            assert r.countdown.await_args.args[1] == 180
            assert calls == [('GET',BASE+'acCitar'),('POST',BASE+'salirInicio')]
            assert r.control.snapshot()['checks'] == 0
            await browser.close()
    asyncio.run(scenario())


def test_slow_navigation_after_sent_click_does_not_resubmit():
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            posts=[]
            async def respond(route):
                if route.request.method=='POST':
                    posts.append(route.request.url)
                    await asyncio.sleep(5.5)
                    body='<p>No hay citas disponibles</p>'
                else:
                    body='<form action="'+BASE+'acCitar" method="post"><button id="btnEnviar">Solicitar cita</button></form>'
                await route.fulfill(body=body,content_type='text/html; charset=utf-8')
            await page.route('**/*',respond);r=runner_for(page,ctx)
            await page.goto(BASE+'acValidarEntrada')
            await r.step()
            assert page.url==BASE+'acCitar'
            assert (await r.observe()).kind=='request_no_slots'
            assert posts==[BASE+'acCitar']
            assert r.control.snapshot()['requests']==1
            await browser.close()
    asyncio.run(scenario())


@pytest.mark.parametrize("initial_loading", [False, True])
def test_manual_booking_with_diagnostics_retains_scroll_and_input(tmp_path, monkeypatch, initial_loading):
    from dataclasses import replace
    from app.diagnostics import PageTrace
    from app.assist import _document_key
    monkeypatch.setenv("DEBUG_DIR", str(tmp_path))
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw)
            ctx=await browser.new_context(viewport={"width":1000,"height":700})
            page=await ctx.new_page()
            body='<h1>Selecciona una cita</h1><input type="radio" name="cita" value="demo"><div style="height:3000px"></div><input id="captcha" name="txtCaptcha"><p id="status">Pending</p>'
            await page.route('**/*',lambda route:route.fulfill(body=body,content_type='text/html; charset=utf-8'))
            r=runner_for(page,ctx)
            r.checker.settings=replace(r.checker.settings,diagnostics_enabled=True)
            r.control.set_diagnostics(True)
            r.checker._assist_control=r.control
            await page.goto(BASE+'acOfertarCita')
            if initial_loading:
                observe = r.observe
                first_tick = True
                async def slow_first_tick():
                    nonlocal first_tick
                    observed = await observe()
                    if first_tick:
                        first_tick = False
                        return replace(observed, kind='loading')
                    return observed
                monkeypatch.setattr(r, 'observe', slow_first_tick)
            # A slow renderer can report loading on the first tick. The
            # guarantee starts when control is actually handed to the user.
            for _ in range(10):
                await r.step()
                if r.control.booking_handoff:
                    break
            assert r.control.booking_handoff
            images_before = set(tmp_path.rglob('*.png'))
            screenshot_calls = []
            async def forbidden_screenshot(*args, **kwargs):
                screenshot_calls.append(True)
                raise AssertionError('Manual handoff must not request screenshots')
            monkeypatch.setattr(page, 'screenshot', forbidden_screenshot)
            trace=PageTrace(page,r.checker,"test","run",_document_key)
            trace.start()
            try:
                await page.evaluate('window.scrollTo(0,1600)')
                for number in range(3):
                    await page.evaluate('(n)=>document.querySelector("#status").textContent="Pending "+n',number)
                    await r.step()
                    assert await page.evaluate('window.scrollY')==1600
                r.control.set_paused(True)
                await page.locator('#captcha').fill('manual input')
                await page.locator('#captcha').press('Tab')
                await page.wait_for_timeout(250)
                await page.evaluate('window.scrollTo(0,1600)')
                position=await page.evaluate('window.scrollY')
                await asyncio.sleep(1.2)
                assert await page.evaluate('window.scrollY')==position
                assert await page.locator('#captcha').input_value()=='manual input'
                assert list(tmp_path.rglob('*.html'))
                assert set(tmp_path.rglob('*.png')) == images_before
                assert not screenshot_calls
            finally:
                await trace.stop()
                r.detach()
                await browser.close()
    asyncio.run(scenario())


@pytest.mark.parametrize("control", [
    '<button id="btnEnviar" onclick="if(!event.isTrusted)event.preventDefault()">Solicitar cita</button>',
    '<input id="btnEnviar" type="button" value="Solicitar cita" onclick="if(event.isTrusted)this.form.submit()">',
    '<input id="btnEnviar" type="submit" value="Solicitar cita" onclick="if(!event.isTrusted)event.preventDefault()">',
])
@pytest.mark.skipif(sys.platform != "darwin", reason="Locked-session handling is implemented only on macOS")
@pytest.mark.parametrize("lost_ack", [False, True])
def test_locked_desktop_uses_keyboard_for_one_trusted_post(monkeypatch, control, lost_ack):
    from app import desktop_session
    monkeypatch.setattr(desktop_session,'screen_locked',lambda:True)
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            posts=[]
            async def route(request):
                if request.request.method=='POST':posts.append(request.request.url)
                body='<p>No hay citas disponibles</p>' if request.request.method=='POST' else '<form action="acCitar" method="post">'+control+'</form>'
                await request.fulfill(body=body,content_type='text/html; charset=utf-8')
            await page.route('**/*',route)
            r=runner_for(page,ctx)
            if lost_ack:
                from playwright.async_api import TimeoutError as PlaywrightTimeoutError
                activate = r.checker._activate_control
                async def lose_keyboard_reply(locator, **kwargs):
                    press = locator.press
                    async def sent_key(*args, **options):
                        await press(*args, **options)
                        raise PlaywrightTimeoutError('Locator.press: acknowledgement lost')
                    monkeypatch.setattr(locator, 'press', sent_key)
                    await activate(locator, **kwargs)
                monkeypatch.setattr(r.checker, '_activate_control', lose_keyboard_reply)
            await page.goto(BASE+'acValidarEntrada')
            async def forbidden(*args,**kwargs):raise AssertionError('Locked desktop must not use mouse dispatch')
            page.mouse.click=forbidden
            page.mouse.move=forbidden
            await r.step()
            await page.wait_for_url(BASE+'acCitar')
            assert posts==[BASE+'acCitar']
            assert (await r.observe()).kind=='request_no_slots'
            r.detach();await browser.close()
    asyncio.run(scenario())


def test_http_500_after_no_slots_exit_loads_entry_without_replaying_post():
    async def scenario():
        from playwright.async_api import async_playwright
        from app.config import DEFAULT_START_URL
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            calls=[]
            async def respond(route):
                calls.append((route.request.method,route.request.url))
                if route.request.method=='POST':
                    await route.fulfill(status=500,content_type='text/html',
                        body='<title>500 Internal Server Error</title><h1>Internal Server Error</h1>')
                elif route.request.url.endswith('acCitar'):
                    await route.fulfill(content_type='text/html',body='<form method="post" action="index.html"><button>Aceptar</button></form>')
                else:
                    await route.fulfill(content_type='text/html',body='<select id="form"><option>Barcelona</option></select>')
            await page.route('**/*',respond)
            r=runner_for(page,ctx);r.countdown=AsyncMock(return_value=True)
            await page.goto(BASE+'acCitar')
            r.last_action='no_slots_restart'
            await page.get_by_role('button',name='Aceptar').click()
            await page.wait_for_url('**/index.html')
            observed=await r.observe()
            assert observed.kind=='server_error' and r.last_method=='POST'
            await r.recover(observed)
            assert calls==[('GET',BASE+'acCitar'),('POST',BASE+'index.html'),('GET',DEFAULT_START_URL)]
            assert (await r.observe()).kind=='province'
            assert r.manual_signature is None and r.pending_action is None and r.last_action is None
            await browser.close()
    asyncio.run(scenario())


@pytest.mark.parametrize('readonly,delay', [(False,0),(True,200)])
@pytest.mark.parametrize('render_delay_ms', [0, 600])
def test_copy_from_auth_continues_and_uses_fresh_identity(readonly,delay,render_delay_ms):
    async def scenario():
        from playwright.async_api import async_playwright, expect
        from urllib.parse import parse_qs
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            posts=[];generation=0
            async def respond(route):
                nonlocal generation
                if route.request.url.endswith('acEntrada'):
                    generation+=1
                    doc,name=('X0000000T','AUTH ONE') if generation==1 else ('X0000001R','AUTH TWO')
                    lock='window.document.querySelectorAll("input").forEach(field=>field.readOnly=true);' if readonly else ''
                    body=f'''<form method="post" action="acValidarEntrada">
                        <input id="txtIdCitado" name="document"><input id="txtDesCitado" name="name">
                        <select id="txtPaisNac" name="nationality"><option value="ES">España</option></select>
                        <button type="button" id="btnCopiar" onclick='setTimeout(()=>{{window.document.getElementById("txtIdCitado").value="{doc}";window.document.getElementById("txtDesCitado").value="{name}";{lock}}},{delay})'>Copiar datos</button>
                        <button id="btnEnviar">Aceptar</button></form>'''
                else:
                    posts.append(parse_qs(route.request.post_data))
                    body='<button id="btnEnviar">Solicitar cita</button>'
                await route.fulfill(body=body,content_type='text/html; charset=utf-8')
            await page.route('**/*',respond);r=runner_for(page,ctx)
            r.task=SimpleNamespace(id='test',applicant_fill_mode='copy',document_number='DO NOT USE',full_name='DO NOT USE',nationality='España')
            for _ in range(2):
                await page.goto(BASE+'acEntrada')
                if render_delay_ms:
                    await page.evaluate("delay => {document.body.style.visibility='hidden'; setTimeout(()=>document.body.style.visibility='',delay)}",render_delay_ms)
                # Navigation load does not imply the fixture is rendered.
                # One runner tick may correctly yield while the body is hidden.
                await expect(page.locator('#txtIdCitado')).to_be_visible(timeout=10_000)
                await expect(page.locator('#btnCopiar')).to_be_visible(timeout=10_000)
                await r.step()
                assert (await r.observe()).kind=='request_menu'
                assert r.manual_signature is None
            assert posts==[{'document':['X0000000T'],'name':['AUTH ONE'],'nationality':['ES']},
                           {'document':['X0000001R'],'name':['AUTH TWO'],'nationality':['ES']}]
            await browser.close()
    asyncio.run(scenario())


def test_copy_button_without_data_is_not_submitted():
    async def scenario():
        from playwright.async_api import async_playwright
        from app.site import SiteCheckError
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            await page.route('**/*',lambda route:route.fulfill(content_type='text/html; charset=utf-8',body='<input id="txtIdCitado"><input id="txtDesCitado"><button id="btnCopiar">Copiar datos</button><select id="txtPaisNac"><option>España</option></select>'))
            await page.goto(BASE+'acEntrada');r=runner_for(page,ctx)
            with pytest.raises(SiteCheckError) as error:
                await r.checker._fill_applicant_with_clave(page,SimpleNamespace(id='test',nationality='España'),'run')
            assert error.value.detail=='applicant.copy_from_clave'
            await browser.close()
    asyncio.run(scenario())


@pytest.mark.parametrize('variant,expected', [('booking','booking'),('server','server_error'),('expired','expired'),('empty','request_no_slots')])
def test_booking_with_captcha_is_distinct_from_site_errors(variant,expected):
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            text={'booking':'Selecciona una cita','server':'Internal Server Error','expired':'Su sesión ha caducado','empty':'En este momento no hay citas disponibles'}[variant]
            body=f'<h1>{text}</h1><input type="radio" name="cita" value="demo">'
            if variant=='booking':body+='<input id="captcha">'
            await page.route('**/*',lambda route:route.fulfill(status=500 if variant=='server' else 200,body=body,content_type='text/html; charset=utf-8'))
            await page.goto(BASE+'acOfertarCita');r=runner_for(page,ctx)
            observed=await r.observe();assert observed.kind==expected
            if expected=='booking':
                await r.step();assert r.control.booking_handoff
                r.alerter.start.assert_awaited_once()
            else:
                assert not r.control.booking_handoff
                r.alerter.start.assert_not_awaited()
            r.detach();await browser.close()
    asyncio.run(scenario())


def test_booking_handoff_never_acts_after_manual_navigation(tmp_path,monkeypatch):
    from dataclasses import replace
    from app.diagnostics import PageTrace
    from app.assist import _document_key
    monkeypatch.setenv('DEBUG_DIR',str(tmp_path))
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context(viewport={'width':1000,'height':700});page=await ctx.new_page()
            requests=[]
            async def respond(route):
                requests.append((route.request.method,route.request.url))
                body='<input type="radio" name="cita" value="demo"><div style="height:2500px"></div><input id="captcha"><a href="next">Continuar</a>' if route.request.url.endswith('acOfertarCita') else '<h1>Internal Server Error</h1><div style="height:2500px"></div><input id="notes"><button>Reiniciar</button>'
                await route.fulfill(status=200 if route.request.url.endswith('acOfertarCita') else 500,body=body,content_type='text/html; charset=utf-8')
            await page.route('**/*',respond);r=runner_for(page,ctx)
            r.checker.settings=replace(r.checker.settings,diagnostics_enabled=True);r.control.set_diagnostics(True);r.checker._assist_control=r.control
            await page.goto(BASE+'acOfertarCita');await r.step()
            assert r.control.booking_handoff and r.checker._assist_guard is None
            r.act=AsyncMock(side_effect=AssertionError('Automatic action after handoff'))
            r.recover=AsyncMock(side_effect=AssertionError('Recovery after handoff'))
            page.screenshot=AsyncMock(side_effect=AssertionError('Screenshot after handoff'))
            trace=PageTrace(page,r.checker,'test','run',_document_key);trace.start()
            try:
                await page.locator('#captcha').fill('manual text');await page.locator('#captcha').press('Tab')
                await r.step();assert r.booking_acknowledged
                await page.get_by_role('link',name='Continuar').click()
                await page.wait_for_url('**/next')
                await page.locator('#notes').fill('human continues')
                await page.locator('#notes').press('Tab')
                await page.wait_for_timeout(250)
                await page.evaluate('window.scrollTo(0,1500)')
                await page.wait_for_timeout(250)
                assert await page.evaluate('window.scrollY')==1500  # Stable before helper ticks.
                for _ in range(3):await r.step()
                page.screenshot.assert_not_awaited()
                assert await page.evaluate('window.scrollY')==1500
                assert await page.locator('#notes').input_value()=='human continues'
                assert len(requests)==2 and all(method=='GET' for method,_ in requests)
                r.act.assert_not_awaited();r.recover.assert_not_awaited();page.screenshot.assert_not_awaited()
                assert r.control.snapshot()['mode']=='manual' and not page.is_closed()
                assert list(tmp_path.rglob('*.html'))
            finally:
                await trace.stop();r.detach();await browser.close()
    asyncio.run(scenario())

@pytest.mark.parametrize('stage', ['province', 'procedure'])
def test_cookie_banner_is_accepted_before_actions_on_any_automatic_page(stage):
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            form=('<select id="form"><option>Barcelona</option></select>' if stage=='province'
                  else '<select id="tramiteGrupo[0]"><option>Toma de huellas</option></select>')
            banner='''<div id="cookie-law-info-bar" style="position:fixed;bottom:0">
                Política de uso de cookies
                <a href="#" id="cookie_action_close_header" onclick="event.preventDefault();document.body.dataset.accepts=String(Number(document.body.dataset.accepts||0)+1);this.parentElement.style.display='none'">Acepto</a>
                </div><button id="btnAceptar">Aceptar</button>'''
            await page.route('**/*',lambda route:route.fulfill(body=form+banner,content_type='text/html'))
            await page.goto(BASE+'index');r=runner_for(page,ctx);r.act=AsyncMock()
            assert (await r.observe()).kind==stage
            await r.step()
            assert await page.locator('body').get_attribute('data-accepts')=='1'
            assert not await page.locator('#cookie-law-info-bar').is_visible()
            r.act.assert_not_awaited()
            await r.step()
            r.act.assert_awaited_once()
            assert await page.locator('body').get_attribute('data-accepts')=='1'
            await browser.close()
    asyncio.run(scenario())


@pytest.mark.parametrize('url,body,ready',[
    ('https://www2.agenciatributaria.gob.es/wlpl/MOVI-P24H/ObtenerClaveMovilQR','<body>Cl@ve Móvil</body>',False),
    ('https://www2.agenciatributaria.gob.es/wlpl/MOVI-P24H/ObtenerClaveMovilQR','<canvas id="qr" width="160" height="160"></canvas>',True),
    ('https://pasarela.clave.gob.es/Proxy2/ServiceProvider','<button>Cl@ve Permanente</button>',False),
    ('https://clave.gob.es/login','<input name="usuario"><input type="password">',True),
    ('https://clave.gob.es/login','<input name="usuario" style="display:none"><input type="password" disabled>',False),
])
def test_clave_attention_requires_ready_visible_controls(url,body,ready):
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw)
            try:
                page=await browser.new_page()
                await page.route('**/*',lambda route:route.fulfill(body=body,content_type='text/html'))
                await page.goto(url)
                checker=IcpPlusChecker(Settings())
                assert await checker._clave_requires_attention(page) is ready
            finally:await browser.close()
    asyncio.run(scenario())


@pytest.mark.parametrize('certificate_failure',[False,True])
def test_server_recovery_backoff_survives_return_to_authorization(certificate_failure):
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw)
            try:
                ctx=await browser.new_context();page=await ctx.new_page()
                message=('Error al validar el certificado: Internal server error generating the SAML response: Error invocando el servicio web [DSSAfirmaVerifyCertificate] de la plataforma @Firma. 0401' if certificate_failure else '500 Internal Server Error')
                pages={'error':'<p>'+message+'</p><button onclick="location.href=\'province\'">Aceptar</button>',
                       'province':'<select id="form"><option>Barcelona</option></select>',
                       'procedure':'<select id="tramiteGrupo[0]"><option>Demo</option></select>',
                       'auth':'<button id="btnAccesoClave">Con Clave</button>'}
                await page.route('**/*',lambda route:route.fulfill(body=pages[route.request.url.rsplit('/',1)[-1]],content_type='text/html'))
                r=runner_for(page,ctx);r.act=AsyncMock();delays=[]
                async def countdown(screen,delay,action):delays.append(delay);return True
                r.countdown=countdown
                for cycle in range(2):
                    await page.goto(BASE+'error');await r.step()
                    for stage in ('province','procedure','auth'):
                        await page.goto(BASE+stage);await r.step()
                assert delays==([180,180] if certificate_failure else [0,30])
            finally:await browser.close()
    asyncio.run(scenario())


def test_manual_office_navigation_during_capture_does_not_wait_for_old_select():
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw);ctx=await browser.new_context();page=await ctx.new_page()
            # Fixture navigation has its own browser timeout. Start the tighter
            # old-select regression bound only after the contact form is ready.
            page.set_default_timeout(10000)
            page.set_default_navigation_timeout(15000)
            async def respond(route):
                body = ('<select id="idSede"><option value="1">Office A</option><option value="2">Office B</option></select>'
                        if route.request.url.endswith('acCitar') else
                        '<input id="txtTelefonoCitado"><input id="emailUNO"><input id="emailDOS">')
                await route.fulfill(body=body,content_type='text/html; charset=utf-8')
            await page.route('**/*',respond)
            r=runner_for(page,ctx)
            await page.goto(BASE+'acCitar')
            observed=await r.observe()
            navigation_ready=asyncio.Event()
            continue_after_capture=asyncio.Event()
            async def save(observed,reason):
                await page.goto(BASE+'acVerFormulario')
                await page.locator('#emailDOS').wait_for(state='visible')
                navigation_ready.set()
                await continue_after_capture.wait()
            r.save=save
            r.silence=AsyncMock()
            operation=asyncio.create_task(r.office(observed))
            ready=asyncio.create_task(navigation_ready.wait())
            try:
                done,_=await asyncio.wait((operation,ready),return_when=asyncio.FIRST_COMPLETED)
                if operation in done:
                    await operation  # Surface fixture/navigation failures directly.
                assert navigation_ready.is_set()
                continue_after_capture.set()
                await asyncio.wait_for(operation,timeout=5)
                r.silence.assert_awaited()
                assert (await r.observe()).kind=='contact'
            finally:
                operation.cancel();ready.cancel()
                await asyncio.gather(operation,ready,return_exceptions=True)
                await browser.close()
    asyncio.run(scenario())


@pytest.mark.parametrize('outcome',['menu','province','applicant_warning','missing','ambiguous'])
def test_profile_salir_mode_follows_actual_button_without_replaying_search(outcome):
    async def scenario():
        from playwright.async_api import async_playwright
        async with async_playwright() as pw:
            browser=await launch_browser(pw)
            try:
                ctx=await browser.new_context();page=await ctx.new_page();calls=[]
                async def respond(route):
                    endpoint=route.request.url.rsplit('/',1)[-1];calls.append((route.request.method,endpoint))
                    if endpoint=='acCitar':
                        body='<p>En este momento no hay citas disponibles</p><form action="salirInicio" method="post">'
                        if outcome!='missing':body+='<input type="button" value="Salir" onclick="goAc_opc_direct()">'
                        if outcome=='ambiguous':body+='<button type="button">Salir</button>'
                        body+='</form><script>function goAc_opc_direct(){document.forms[0].action="acOpcDirect";document.forms[0].submit();}</script>'
                    elif outcome=='menu':body='<form action="acCitar" method="post"><button id="btnEnviar">Solicitar cita</button></form>'
                    elif outcome=='province':body='<select id="form"><option>Test</option></select>'
                    else:body=FORM+'<p>No hay citas disponibles</p>'
                    await route.fulfill(body=body,content_type='text/html; charset=utf-8')
                await page.route('**/*',respond)
                r=runner_for(page,ctx);r.task=SimpleNamespace(id='test',no_slots_mode='salir')
                r.countdown=AsyncMock(return_value=True);r.hold=AsyncMock()
                await page.goto(BASE+'acCitar');await r.step()
                assert r.countdown.await_count==1
                if outcome in {'missing','ambiguous'}:
                    assert calls==[('GET','acCitar')];assert r.hold.await_count==1;return
                assert calls==[('GET','acCitar'),('POST','acOpcDirect')]
                current=await r.observe()
                assert current.kind=={'menu':'request_menu','province':'province','applicant_warning':'applicant_no_slots'}[outcome]
                assert r.control.snapshot()['checks']==0
                if outcome=='menu':
                    await r.step()
                    assert calls[-1]==('POST','acCitar') and r.control.snapshot()['checks']==1
            finally:await browser.close()
    asyncio.run(scenario())


def test_applicant_warning_after_post_never_resubmits_identity():
    async def scenario():
        from playwright.async_api import async_playwright,expect
        async with async_playwright() as pw:
            browser=await launch_browser(pw)
            try:
                ctx=await browser.new_context();page=await ctx.new_page();calls=[]
                async def respond(route):
                    path=route.request.url.rsplit('/',1)[-1];calls.append((route.request.method,path))
                    body='<form action="acEntrada" method="post"><button>Open form</button></form>' if path=='citar' else FORM+'<p>No hay citas disponibles</p>'
                    await route.fulfill(body=body,content_type='text/html; charset=utf-8')
                await page.route('**/*',respond);runner=runner_for(page,ctx)
                runner.countdown=AsyncMock(return_value=True);runner.hold=AsyncMock()
                await page.goto(BASE+'citar');await page.get_by_role('button',name='Open form').click()
                await expect(page.locator('#txtIdCitado')).to_be_visible(timeout=10_000)
                assert runner.last_method=='POST'
                await runner.step()
                assert calls==[('GET','citar'),('POST','acEntrada'),('GET','acEntrada')]
                assert await page.locator('#txtIdCitado').input_value()==''
                runner.hold.assert_not_awaited();runner.countdown.assert_awaited_once()
            finally:await browser.close()
    asyncio.run(scenario())
