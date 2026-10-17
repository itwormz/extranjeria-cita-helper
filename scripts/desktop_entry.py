"""Entry for the self-contained desktop bundle and its child worker."""
import os
import sys
import multiprocessing
multiprocessing.freeze_support()
from app.desktop.paths import runtime_root

root = runtime_root()
root.mkdir(parents=True, exist_ok=True, mode=0o700)
os.chdir(root)
# QProcess owns these pipes in worker mode; a GUI launch may have no streams.
from app.desktop.streams import restore_standard_streams
restore_standard_streams()

if sys.argv[1:] == ['--smoke-test']:
    from app.desktop.smoke import main as smoke
    raise SystemExit(smoke())

if sys.argv[1:] == ['--ipc-smoke-test']:
    import asyncio
    import json
    from app.assist_control import AssistControl
    from app.desktop.worker import consume_commands
    control = AssistControl()
    asyncio.run(consume_commands(sys.stdin.buffer, control, lambda message: None))
    print(json.dumps({'ok': True, 'paused': control.paused, 'stopped': control.stop_requested}), flush=True)
    raise SystemExit(0)

from app.main import main
raise SystemExit(main(sys.argv[1:] or ['desktop']))
