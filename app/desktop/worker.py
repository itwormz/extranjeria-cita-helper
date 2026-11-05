"""One task's browser owner; no Qt and no user-interface event loop."""
import asyncio
import sqlite3
import os
import signal
import sys
from contextlib import suppress

from ..assist_control import AssistControl
from ..assist_runtime import run_assist
from ..logging_utils import register_sensitive
from .profiles import ProfileStore
from .protocol import JsonLinesDecoder, encode_message, validate_command


def apply_command(control, message):
    if not validate_command(message):
        return False
    command = message['command']
    if command == 'pause':
        control.set_paused(message['paused'])
    elif command == 'shutdown':
        control.close_browser_requested = True
        control.request_stop()
    elif command == 'stop':
        control.request_stop()
    elif command == 'restart':
        control.request_restart()
    elif command == 'show_browser':
        control.show_browser_requested = True
    elif command == 'appointment_daily_target':
        control.pending_daily_target = message['target']
        control.daily_target_pending = True
    elif command == 'appointment_try_now':
        control.pending_attempt_now = True
    elif command == 'appointment_distribution':
        budget = getattr(control, 'appointment_budget', None)
        if budget is not None and isinstance(message.get('disabled'), bool):
            budget.distribution_disabled = message['disabled']
            budget.windows = ()
            control.save_appointment_budget(budget)
    elif command == 'profile_timings':
        from ..config import AppointmentProtection
        from ..timings import parse_timings
        try:
            timings = parse_timings(message['timings'])
        except (ValueError, TypeError):
            return False
        if 'protection' in message:
            errors = []
            protection = AppointmentProtection.parse(message['protection'], errors)
            if errors:
                return False
            previous = (control.pending_appointment_protection
                        or control.effective_appointment_protection or AppointmentProtection())
            if previous.adaptive_target and not protection.adaptive_target:
                # A later switch to the base budget supersedes an earlier queued override.
                control.pending_daily_target = None
                control.daily_target_pending = False
            control.pending_appointment_protection = protection
        control.pending_profile_timings = timings
    elif command == 'preferences':
        control.set_diagnostics(message['diagnostics'])
        control.set_sound(message['sound'])
    return True


def load_bound_budget(control, store, policy, *, transfer_manual=False):
    """Keep today's user choice when a copied document gets its own budget."""
    from datetime import datetime
    from ..appointment_schedule import MADRID

    budget = store.load()
    now = datetime.now(MADRID)
    choice = control.daily_target_choice
    previous = getattr(control, 'appointment_budget', None)
    if (choice is None and transfer_manual and previous is not None
            and previous.day == now.date() and previous.manual_target is not None):
        choice = (previous.day, previous.manual_target)
    if choice is not None and choice[0] == now.date():
        budget.set_daily_target(choice[1], now, policy)
        store.save(budget)
        control.daily_target_choice = choice
    return budget


async def consume_commands(stream, control, emit):
    """Read a pipe using a daemon thread; Windows asyncio cannot read stdin pipes."""
    import threading
    loop = asyncio.get_running_loop()
    queue = asyncio.Queue()
    try:
        descriptor = stream.fileno()
    except (AttributeError, OSError, ValueError):
        descriptor = None

    # Never hold a pending Windows stdin read: native imports such as NumPy
    # inspect the same pipe and can deadlock until the controller sends data.
    # Peek first, then read only available bytes; the idle wait holds no IO lock.
    if descriptor is not None and sys.platform == 'win32':
        import ctypes
        from ctypes import wintypes
        import msvcrt
        handle = msvcrt.get_osfhandle(descriptor)
        kernel = ctypes.WinDLL('kernel32', use_last_error=True)
        kernel.ReadFile.argtypes = [wintypes.HANDLE, wintypes.LPVOID,
                                    wintypes.DWORD, ctypes.POINTER(wintypes.DWORD), wintypes.LPVOID]
        kernel.ReadFile.restype = wintypes.BOOL
        kernel.PeekNamedPipe.argtypes = [wintypes.HANDLE, wintypes.LPVOID, wintypes.DWORD,
                                        wintypes.LPVOID, ctypes.POINTER(wintypes.DWORD), wintypes.LPVOID]
        kernel.PeekNamedPipe.restype = wintypes.BOOL
        idle = threading.Event()

        def read(size):
            available = wintypes.DWORD()
            while True:
                if not kernel.PeekNamedPipe(handle, None, 0, None, ctypes.byref(available), None):
                    error = ctypes.get_last_error()
                    if error == 109:
                        return b''
                    raise ctypes.WinError(error)
                if available.value:
                    break
                idle.wait(.05)
            size = min(size, available.value)
            buffer = ctypes.create_string_buffer(size)
            count = wintypes.DWORD()
            if not kernel.ReadFile(handle, buffer, size, ctypes.byref(count), None):
                error = ctypes.get_last_error()
                if error == 109:  # ERROR_BROKEN_PIPE: controller closed stdin.
                    return b''
                raise ctypes.WinError(error)
            return buffer.raw[:count.value]
    else:
        read = (lambda size: os.read(descriptor, size)) if descriptor is not None else stream.read

    def pump():
        try:
            while True:
                data = read(4096)
                loop.call_soon_threadsafe(queue.put_nowait, data)
                if not data:
                    return
        except (OSError, ValueError):
            with suppress(RuntimeError):
                loop.call_soon_threadsafe(queue.put_nowait, b'')
        except RuntimeError:
            pass  # The worker's event loop has already ended.

    threading.Thread(target=pump, name='worker-commands', daemon=True).start()
    decoder = JsonLinesDecoder()
    while True:
        data = await queue.get()
        if not data:
            control.request_stop()
            return
        for message in decoder.feed(data):
            if control.browser_retained and message.get('command') not in {'show_browser', 'preferences', 'stop', 'shutdown'}:
                continue
            if not apply_command(control, message):
                emit({'type':'protocol_error'})


async def run_worker(task_id):
    from ..main import _paths, ROOT
    from ..config import load_dotenv
    from ..state import StateStore
    load_dotenv(ROOT / '.env')
    tasks, catalog, state_path = _paths()
    profiles = ProfileStore(tasks, catalog)
    control = AssistControl()
    preferences = profiles.preferences()
    control.set_diagnostics(preferences['diagnostics'])
    control.set_sound(preferences['sound'])

    def emit(message):
        try:
            sys.stdout.buffer.write(encode_message(message))
            sys.stdout.buffer.flush()
        except (BrokenPipeError, OSError):
            control.request_stop()

    try:
        task = next(t for t in profiles.load() if t.id == task_id)
        if task.errors or task.access_mode not in {'certificate', 'clave'}:
            emit({'type':'error', 'message':'Профиль содержит ошибки. Проверить настройки.'})
            return 2
    except (ValueError, StopIteration):
        emit({'type':'error', 'message':'Не удалось загрузить выбранный профиль.'})
        return 2
    control.effective_appointment_protection = task.appointment_protection
    from ..appointment_budget_store import BudgetStore
    from dataclasses import replace
    budget_identity = replace(task, document_number='') if task.applicant_fill_mode == 'copy' else task
    budget_store = BudgetStore(profiles.path.parent / 'data' / 'appointment-budget', budget_identity)
    register_sensitive([task.document_number, task.full_name, task.contact_phone, task.contact_email])
    loop = asyncio.get_running_loop()
    signals = []
    if sys.platform != 'win32':
        for signum in (signal.SIGTERM, signal.SIGINT):
            loop.add_signal_handler(signum, control.request_stop)
            signals.append(signum)

    statistics = None

    async def states():
        while True:
            control.statistics_tick()
            state = control.snapshot()
            if statistics is not None:
                try:
                    state['profile_checks_today'] = statistics.total(task.id, 'today')
                except (OSError, ValueError, sqlite3.Error):
                    pass
            budget = getattr(control, 'appointment_budget', None)
            if budget is not None:
                from datetime import datetime
                from ..appointment_schedule import MADRID
                now = datetime.now(MADRID)
                policy = control.effective_appointment_protection or task.appointment_protection
                previous_day = budget.day
                state['appointment_budget'] = budget.summary(now, policy, hot=getattr(control, 'hot_retry', False))
                if previous_day is not None and budget.day != previous_day:
                    control.save_appointment_budget(budget)
            emit({'type':'state', 'state':state})
            await asyncio.sleep(0.5)

    statistics_session = None
    statistics_end_reason = 'interrupted'
    incoming = asyncio.create_task(consume_commands(sys.stdin.buffer, control, emit))
    outgoing = asyncio.create_task(states())
    try:
        from contextlib import ExitStack
        from dataclasses import replace

        state_store = StateStore(state_path)
        with ExitStack() as locks:
            if not locks.enter_context(state_store.task_lock(task_id)):
                emit({'type':'error','message':'Этот профиль уже используется другим процессом.'})
                return 1
            bound_key = None

            def bind_budget(store):
                nonlocal bound_key
                if store.key == bound_key:
                    return True
                if not locks.enter_context(state_store.task_lock('budget-' + store.key)):
                    return False
                control.appointment_budget = load_bound_budget(
                    control, store, task.appointment_protection,
                    transfer_manual=(bound_key == budget_store.key
                                     and BudgetStore._profile_key(budget_store.root, task.id) in (None, budget_store.key)))
                control.save_appointment_budget = store.save
                if task.applicant_fill_mode != "copy" or store is not budget_store:
                    store.remember_profile(task.id)
                bound_key = store.key
                return True

            if not bind_budget(budget_store):
                emit({'type':'error','message':'Этот документ уже используется другим процессом.'})
                return 1

            if task.applicant_fill_mode == 'copy':
                def bind_copied_budget(number, document_type):
                    copied = replace(task, document_number=number, document_type=document_type)
                    return bind_budget(BudgetStore(budget_store.root, copied))

                control.bind_copied_appointment_budget = bind_copied_budget
            from .statistics import ProfileStatistics
            statistics = ProfileStatistics(profiles.path.parent / 'data' / 'profile-statistics.json')
            from .statistics_session import StatisticsSession
            statistics_session = StatisticsSession(statistics, task, control)
            emit({'type':'ready', 'profile_id':task_id})
            result = await run_assist(task, profiles.settings(task_id), control=control, open_panel=False)
            statistics_end_reason = 'completed' if result == 0 else 'failed'
            emit({'type':'state', 'state':control.snapshot()})
            return result
    except Exception as exc:
        from .startup_report import StartupFailure, save_startup_error
        if isinstance(exc, StartupFailure):
            message = str(exc)
        else:
            report_id, summary = await asyncio.to_thread(save_startup_error, exc, 'worker')
            location = f'startup-errors/{report_id}.json' if report_id else 'report unavailable'
            message = f'Worker failed — {summary} — {location}'
        emit({'type':'error','message':message})
        return 1
    finally:
        control.flush_statistics()
        if statistics_session:
            statistics_session.close(statistics_end_reason)
        for pending in (incoming, outgoing):
            pending.cancel()
            with suppress(asyncio.CancelledError):
                await pending
        for signum in signals:
            loop.remove_signal_handler(signum)
