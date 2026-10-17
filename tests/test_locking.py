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
