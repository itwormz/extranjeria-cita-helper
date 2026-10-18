import subprocess
import sys
from app.locking import locked_file


def test_competing_process_cannot_take_worker_lock(tmp_path):
    path = tmp_path / 'profile.lock'
    code = '''
import sys
from pathlib import Path
from app.locking import locked_file
try:
    with locked_file(Path(sys.argv[1]), blocking=False):
        sys.exit(2)
except BlockingIOError:
    sys.exit(0)
'''
    with locked_file(path):
        result = subprocess.run([sys.executable, '-c', code, str(path)], timeout=5)
        assert result.returncode == 0
    with locked_file(path, blocking=False):
        pass


def test_windows_lock_does_not_write_before_acquiring(monkeypatch):
    import sys
    from types import SimpleNamespace
    from app import locking
    calls = []
    class Handle:
        def seek(self, offset):
            assert offset == 0
        def fileno(self):
            return 17
        def write(self, value):
            raise PermissionError("another handle already owns the byte")
    monkeypatch.setattr(locking, 'os', SimpleNamespace(
        name='nt', fstat=lambda fd: SimpleNamespace(st_size=0)))
    monkeypatch.setitem(sys.modules, 'msvcrt', SimpleNamespace(
        LK_NBLCK=2, locking=lambda *args: calls.append(args)))
    locking.lock_handle(Handle())
    assert calls == [(17, 2, 1)]
