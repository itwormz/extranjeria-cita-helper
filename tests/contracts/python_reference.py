"""Execute real Python flow decisions against a synthetic effect-recording boundary.

Does not claim browser action success/navigation. DOM/dispatch/ownership remain
covered by the existing browser/flow tests until the Rust adapter is implemented.
"""
import asyncio
from types import SimpleNamespace
from unittest.mock import AsyncMock

from app.assist_control import AssistControl
from app.assist_runtime import AssistRestartRequested
from app.assist_flow import AssistRunner, Observed
from app.config import REGRESO_PROCEDURE, TIE_PROCEDURE


async def replay_action(case):
    options = case['options']
    effects = []
    kind = case['kind']
    observed = Observed(kind, 'fixture-document', 'fixture-signature', None)
    endpoint = options.get('endpoint', 'acCitar')
    page = SimpleNamespace(
        url='https://icp.administracionelectronica.gob.es/icpplus/' + endpoint,
        wait_for_timeout=AsyncMock(),
    )

    async def reload_page(**_kwargs):
        effects.append(['reload'])

    async def goto(url, **_kwargs):
        if endpoint == 'acEntrada' and url == page.url:
            target = 'applicant'
        elif url == page.url:
            target = 'current'
        else:
            assert url.endswith('/index.html')
            target = 'entry'
        effects.append(['goto', target])

    page.reload = reload_page
    page.goto = goto
    procedure = options.get('procedure', 'huellas')
    task = SimpleNamespace(
        id='fixture-profile', province='Fixture Province',
        access_mode=options.get('access_mode', 'certificate'),
        procedure=REGRESO_PROCEDURE if procedure == 'regreso' else TIE_PROCEDURE,
        applicant_fill_mode=options.get('fill_mode', 'manual'),
        applicant_no_slots_mode=options.get('applicant_mode', 'wait'),
        no_slots_mode=options.get('result_mode', 'salir'),
    )

    async def select_province(*_args):
        effects.append(['select_province'])

    async def select_procedure(_page, actual):
        assert actual == task.procedure
        effects.append(['select_procedure', procedure])

    async def fill_manual(*_args):
        effects.append(['fill_applicant', 'manual', procedure])

    async def fill_copy(*_args):
        effects.append(['fill_applicant', 'copy', procedure])

    async def fill_contact(*_args):
        effects.append(['fill_contact'])
        return True

    async def activate(control, **_kwargs):
        expected_control = 'continue' if kind == 'office' else 'salir'
        assert control == 'fixture-' + expected_control
        effects.append(['activate', expected_control])

    selected_office = {'value': ''}
    office_rows = [dict(value=str(i + 1), text=f'Fixture office {i + 1}', disabled=False)
                   for i in range(options.get('office_count', 0))]

    async def evaluate(script):
        if 'select.options' in script:
            return dict(value=selected_office['value'], options=office_rows)
        return None

    async def select_office(_page, _selector, *, value):
        selected_office['value'] = value
        effects.append(['select_office', value])

    async def alert(_reason, *, repeat):
        effects.append(['alert', repeat])

    async def input_value():
        return selected_office['value']

    page.evaluate = evaluate
    element = SimpleNamespace(input_value=input_value, get_attribute=AsyncMock(return_value=None))
    element.first = element
    page.locator = lambda _selector: element

    async def enter_certificate(*_args, **_kwargs):
        effects.append(['auth', 'certificate'])

    async def enter_clave(*_args, **_kwargs):
        effects.append(['auth', 'clave'])

    checker = SimpleNamespace(
        _enter_with_certificate=enter_certificate, _enter_with_clave=enter_clave,
        _accept_cookies=AsyncMock(return_value=False),
        _select_label=select_province, _select_procedure=select_procedure,
        _fill_applicant_manual=fill_manual, _fill_applicant_with_clave=fill_copy,
        _fill_contact=fill_contact, _activate_control=activate,
        _select_without_focus=select_office,
        settings=SimpleNamespace(navigation_timeout_seconds=45),
    )
    runner = AssistRunner(page, SimpleNamespace(), checker, task, 'fixture-run',
                          AssistControl(), SimpleNamespace(stop=AsyncMock(), start=alert),
                          SimpleNamespace(next_delay=lambda **_kwargs: 30))
    runner.last_method = options.get('method', 'GET')
    runner.last_action = options.get('last_action')
    if options.get('attempted'):
        runner.attempted.add((observed.document, options['attempted']))
    next_observation = observed
    if options.get('document_changed'):
        next_observation = Observed('request_menu', 'new-document', 'new-signature', None)
    runner.observe = AsyncMock(return_value=next_observation)
    runner.guard = AsyncMock(return_value=True)
    runner.save = AsyncMock()
    runner.continuation = AsyncMock(return_value=None if options.get('button_missing') else ('fixture-continue' if kind == 'office' else 'fixture-salir'))

    async def click(_observation, _selector, **kwargs):
        effects.append(['click', kwargs['action']])
        return True

    async def countdown(_observation, delay, _description):
        effects.append(['wait', delay])
        return options.get('wait_completed', True)

    async def hold(*_args):
        effects.append(['hold'])

    runner.click = click
    runner.countdown = countdown
    runner.hold = hold
    original_record = runner.record

    def record(event, *args, **kwargs):
        if event == 'handoff':
            effects.append(['handoff'])
        return original_record(event, *args, **kwargs)

    runner.record = record
    runner.expiry_scope = AsyncMock(return_value=None)
    runner.record_recovery_button = AsyncMock(return_value='Salir')
    try:
        operation = options.get('operation', 'act')
        if operation == 'act':
            await runner.act(observed)
        elif operation == 'recover':
            await runner.recover(observed)
        elif operation == 'step':
            await runner.step()
        else:
            raise ValueError('Unsupported fixture operation')
    except AssistRestartRequested as restart:
        effects.append(['restart', restart.clear_profile, restart.reopen_browser])
    return effects


def replay(case):
    return asyncio.run(replay_action(case))
