"""One task's browser owner; no Qt and no user-interface event loop."""
import asyncio
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
    elif command == 'preferences':
        control.set_diagnostics(message['diagnostics'])
        control.set_sound(message['sound'])
    return True


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
    register_sensitive([task.document_number, task.full_name, task.contact_phone, task.contact_email])
    loop = asyncio.get_running_loop()
    signals = []
    if sys.platform != 'win32':
        for signum in (signal.SIGTERM, signal.SIGINT):
            loop.add_signal_handler(signum, control.request_stop)
            signals.append(signum)

    async def states():
        while True:
            control.statistics_tick()
            emit({'type':'state', 'state':control.snapshot()})
            await asyncio.sleep(0.5)

    incoming = asyncio.create_task(consume_commands(sys.stdin.buffer, control, emit))
    outgoing = asyncio.create_task(states())
    try:
        with StateStore(state_path).task_lock(task_id) as acquired:
            if not acquired:
                emit({'type':'error','message':'Этот профиль уже используется другим процессом.'})
                return 1
            from .statistics import ProfileStatistics
            statistics = ProfileStatistics(profiles.path.parent / 'data' / 'profile-statistics.json')
            try:
                control.set_check_counter(statistics.total(task_id), lambda: statistics.increment(task_id))
                statistics.add_active(task_id, 0)
                control.set_statistics_sink(
                    lambda kind, document, **kwargs: statistics.observe(task_id, kind, document, **kwargs),
                    lambda seconds: statistics.add_active(task_id, seconds))
            except (OSError, ValueError):
                pass
            emit({'type':'ready', 'profile_id':task_id})
            result = await run_assist(task, profiles.settings(task_id), control=control, open_panel=False)
            emit({'type':'state', 'state':control.snapshot()})
            return result
    except Exception as exc:
        from .startup_report import StartupFailure, save_startup_error
        if isinstance(exc, StartupFailure):
            message = str(exc)
        else:
            report_id, summary = save_startup_error(exc, 'worker')
            location = f'startup-errors/{report_id}.json' if report_id else 'report unavailable'
            message = f'Worker failed — {summary} — {location}'
        emit({'type':'error','message':message})
        return 1
    finally:
        control.flush_statistics()
        for pending in (incoming, outgoing):
            pending.cancel()
            with suppress(asyncio.CancelledError):
                await pending
        for signum in signals:
            loop.remove_signal_handler(signum)
