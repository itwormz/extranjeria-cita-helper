import asyncio
import io
from app.assist_control import AssistControl


def test_pipe_commands_and_eof_stop():
    from app.desktop.worker import consume_commands
    stream = io.BytesIO(b'{"command":"pause","paused":true}\n{"command":"stop"}\n')
    control = AssistControl()
    asyncio.run(consume_commands(stream, control, lambda message: None))
    assert control.paused and control.stop_requested


def test_invalid_command_reports_protocol_error_and_keeps_reading():
    from app.desktop.worker import consume_commands
    stream = io.BytesIO(b'{"command":"unknown"}\n{"command":"preferences","sound":false,"diagnostics":false}\n')
    control = AssistControl()
    events = []
    asyncio.run(consume_commands(stream, control, events.append))
    assert events == [{'type':'protocol_error'}]
    assert not control.sound_enabled and not control.diagnostics_enabled
    assert control.stop_requested


def test_native_import_and_exit_while_controller_keeps_stdin_open():
    import subprocess
    import sys
    code = '''
import asyncio, sys
from app.assist_control import AssistControl
from app.desktop.worker import consume_commands
async def main():
    task = asyncio.create_task(consume_commands(sys.stdin.buffer, AssistControl(), lambda _: None))
    await asyncio.sleep(.1)
    import numpy
    assert numpy.zeros(1).size == 1
    task.cancel()
    try:
        await task
    except asyncio.CancelledError:
        pass
asyncio.run(main())
'''
    process = subprocess.Popen([sys.executable, '-c', code], stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        assert process.wait(timeout=5) == 0, process.stderr.read().decode()
    finally:
        process.stdin.close()
        if process.poll() is None:
            process.kill()
            process.wait()
        process.stdout.close()
        process.stderr.close()
