"""Exclusive worker ownership; no legacy scheduler database or outbox."""
from contextlib import contextmanager
from .locking import locked_file

class StateStore:
    def __init__(self, path):
        self.path = path
        path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)

    @contextmanager
    def task_lock(self, task_id):
        with self._acquire(task_id) as acquired:
            yield acquired

    @contextmanager
    def _acquire(self, task_id):
        try:
            manager = locked_file(self.path.parent/f'.{task_id}.lock', blocking=False)
            manager.__enter__()
        except BlockingIOError:
            yield False
            return
        try:
            yield True
        finally:
            manager.__exit__(None, None, None)
