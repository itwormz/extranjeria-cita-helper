"""Own one browser context for a desktop assist run."""
import asyncio
import logging
import random
from contextlib import suppress
from datetime import datetime
from dataclasses import replace
import time

from .assist_control import AssistControl
from .browser_visibility import show_browser
from .assist_flow import AssistRunner
from .diagnostics import PageTrace
from .logging_utils import log_event
from .site import SiteCheckError

log = logging.getLogger(__name__)


class AssistRestartRequested(Exception):
    def __init__(self, reason, *, clear_profile=False, delay_seconds=None, reopen_browser=False, cooldown_completed=False, user_requested=False):
        super().__init__(reason)
        # Every automatic restart has a cooldown unless its recovery branch
        # already waited. Explicit human restart remains immediate.
        delay=float(delay_seconds or 0)
        if not cooldown_completed and not user_requested:
            delay=max(delay,random.uniform(30,60))
        self.reason, self.clear_profile, self.delay_seconds = reason, clear_profile, delay
        self.reopen_browser = reopen_browser


async def wait_restart(control, delay, *, reuse_browser=False):
    remaining = float(delay)
    previous = time.monotonic()
    while (remaining > 0 or control.paused) and not control.stop_requested:
        now = time.monotonic()
        if not control.paused:
            remaining -= now - previous
        previous = now
        if control.restart_requested:
            control.clear_restart()
            remaining = 0
        control.touch()
        control.publish(screen="Новый проход" if reuse_browser else "Новая сессия",
                        evidence="Существующее окно сохранено" if reuse_browser else "Предыдущая сессия закрыта",
                        next_action="Начать с выбора провинции" if reuse_browser else "Запустить Camoufox и начать заново", mode="recovery",
                        remaining_seconds=max(0, remaining))
        await asyncio.sleep(0.2)
    return not control.stop_requested


async def retain_browser(page, control):
    """Stop automation without destroying the user's live appointment page."""
    if control.close_browser_requested:
        return
    control.browser_retained = True
    control.record('handoff', detail='Поиск остановлен — Camoufox оставлен для ручного оформления')
    control.publish(screen='Camoufox оставлен открытым', evidence='Автоматические действия остановлены',
                    next_action='Завершите оформление на сайте. Закройте Camoufox, когда закончите', mode='manual')
    while not page.is_closed() and not control.close_browser_requested:
        control.touch()
        if control.show_browser_requested:
            control.show_browser_requested = False
            await show_browser(page, control)
        await asyncio.sleep(.2)
    control.browser_retained = False


async def run_assist(task, settings, *, max_cycles=None, control=None, open_panel=False):
    control = control or AssistControl()
    if control.diagnostics_enabled is None:
        control.set_diagnostics(settings.diagnostics_enabled)
    try:
        while not control.stop_requested:
            try:
                return await _run_session(task, settings, control, max_cycles=max_cycles)
            except AssistRestartRequested as restart:
                from .site import reset_browser_profile
                control.record('flow_restart', detail=f'Новая сессия: {restart.reason}')
                if restart.clear_profile:
                    reset_browser_profile(settings, task.id)
                control.clear_restart()
                if not await wait_restart(control, restart.delay_seconds):
                    return 0
        return 0
    finally:
        control.record('finish', detail='Поиск остановлен')
        control.finish('Поиск остановлен. Можно запустить снова')


async def open_initial_page(checker,page,task,run_id,runner):
    """A committing initial GET belongs to the same recovery loop as later pages."""
    from playwright.async_api import Error as BrowserError
    try:
        await checker._open_entry(page,task,run_id)
    except SiteCheckError:
        pass  # Recognize the actual block/error document in the ordinary flow.
    except BrowserError as exc:
        if page.is_closed() or type(exc).__name__=='TargetClosedError':
            raise
        runner.save_error_details(exc)
        runner.record('action_interrupted',runner.last_observed,
                      'Первое открытие сайта прервано — проверяем загруженную страницу')
        log_event(log,logging.WARNING,'assist_initial_navigation_interrupted',
                  task_id=task.id,run_id=run_id,error_type=type(exc).__name__)


async def _run_session(task, settings, control, *, max_cycles=None):
    from playwright.async_api import async_playwright
    from .alerts import Alerter
    from .site import IcpPlusChecker, _configure_browser_path
    from .assist import _document_key, _install_lifecycle_logging, reuse_policy_from_settings

    _configure_browser_path()
    checker = IcpPlusChecker(replace(settings, diagnostics_enabled=control.diagnostics_enabled))
    checker._assist_control = control
    alerter = Alerter(settings.assist_alert_sound, repeat_seconds=settings.assist_timings.alert_seconds)
    run_id = datetime.now().astimezone().strftime("%Y%m%d%H%M%S%f")
    control.set_timeline_path(checker._debug_directory(task.id, run_id) / "timeline.jsonl")
    control.set_session(run_id)
    control.record("start", detail=f"Запуск assist — сессия {run_id}")
    trace = None
    context = None
    page = None
    runner = None
    operation = asyncio.current_task()
    playwright = None

    async def heartbeat():
        nonlocal trace
        while True:
            control.touch()
            enabled = control.diagnostics_enabled
            if checker.settings.diagnostics_enabled != enabled:
                checker.settings = replace(checker.settings, diagnostics_enabled=enabled)
            if control.stop_requested or control.restart_requested:
                operation.cancel()
                return
            if runner is not None and operation is not None:
                if control.show_browser_requested:
                    control.show_browser_requested = False
                    await show_browser(page, control)
                if enabled and trace is None:
                    trace = PageTrace(page, checker, task.id, run_id, _document_key)
                    trace.start()
                elif not enabled and trace is not None:
                    await trace.stop()
                    trace = None
                if not control.sound_enabled:
                    await alerter.stop()
                    runner.attention = None
            await asyncio.sleep(0.2)

    pulse = asyncio.create_task(heartbeat())
    try:
        playwright = await async_playwright().start()
        context = await checker.launch_context(playwright, task, force_headed=True)
        page = await checker.page_for_context(context)
        page.set_default_timeout(settings.navigation_timeout_seconds * 1000)
        _install_lifecycle_logging(page, context, task.id, run_id)
        control.publish(screen="Загрузка страницы", evidence="Camoufox запущен", next_action="Открыть сайт", mode="working")
        if control.diagnostics_enabled:
            trace = PageTrace(page, checker, task.id, run_id, _document_key)
            trace.start()
        async def operate():
            control.publish(screen="Начальная страница", evidence="Открываем сайт",
                            next_action="Распознать форму провинции", mode="working")
            await runner.wait_unpaused()
            try:
                await open_initial_page(checker,page,task,run_id,runner)
            except BaseException:
                runner.detach()
                raise
            return await runner.run(max_cycles=max_cycles)
        while not control.stop_requested:
            runner = AssistRunner(page, context, checker, task, run_id, control,
                                  alerter, reuse_policy_from_settings(settings))
            operation = asyncio.create_task(operate())
            try:
                return await operation
            except AssistRestartRequested as restart:
                if restart.clear_profile or restart.reopen_browser:
                    raise
                await alerter.stop()
                runner = None
                operation = asyncio.current_task()
                control.clear_restart()
                control.record('flow_restart', detail=f'Новый проход: {restart.reason} — окно и профиль сохранены')
                if not await wait_restart(control, restart.delay_seconds, reuse_browser=True):
                    await retain_browser(page, control)
                    return 0
        return 0
    except AssistRestartRequested:
        raise
    except (KeyboardInterrupt, asyncio.CancelledError):
        if control.restart_requested and not control.stop_requested:
            raise AssistRestartRequested('ручной перезапуск', clear_profile=True, delay_seconds=0,user_requested=True)
        if control.stop_requested and page is not None and not page.is_closed():
            await alerter.stop()
            await retain_browser(page, control)
        return 0 if control.stop_requested else 130
    except Exception as exc:
        if type(exc).__name__ == "TargetClosedError" and runner is not None:
            log_event(log, logging.INFO, "assist_browser_closed", task_id=task.id, run_id=run_id)
            return 0
        if runner is None:
            from .desktop.startup_report import save_startup_error, StartupFailure
            report_id, summary = save_startup_error(exc, 'browser_launch')
            location = f"startup-errors/{report_id}.json" if report_id else 'report unavailable'
            detail = f"Browser startup failed — {summary} — {location}"
            control.record('fatal_error', detail=detail)
            raise StartupFailure(detail) from exc
        if runner is not None:
            runner.save_error_details(exc)
        control.record('fatal_error', detail=f'Ошибка помощника: {type(exc).__name__} — подробности в диагностике сессии')
        log_event(log, logging.ERROR, "assist_failed", task_id=task.id, run_id=run_id,
                  error_code="INTERNAL_ERROR", error_type=type(exc).__name__)
        return 1
    finally:
        pulse.cancel()
        with suppress(asyncio.CancelledError):
            await pulse
        if trace:
            await trace.stop()
        await alerter.stop()
        if context:
            await checker.close_context(context)
        if playwright:
            await playwright.stop()
