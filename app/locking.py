"""Advisory cross-process locking for cooperating profile editors and workers."""
from contextlib import contextmanager
import errno
import os
from pathlib import Path


def lock_handle(handle, *, blocking=True, posix_record=False):
    if os.name == 'nt':
        import msvcrt
        handle.seek(0)
        if os.fstat(handle.fileno()).st_size == 0:
            handle.write(b'\0')
            handle.flush()
        handle.seek(0)
        # LK_LOCK has a finite retry window. Poll explicitly for a truly
        # blocking editor lock; workers use one immediate attempt.
        import time
        while True:
            try:
                msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
                return
            except OSError as exc:
                if exc.errno not in (errno.EACCES, errno.EAGAIN, errno.EDEADLK):
                    raise
                if not blocking:
                    raise BlockingIOError(exc.errno, str(exc)) from exc
                time.sleep(.05)
    else:
        import fcntl
        operation = fcntl.LOCK_EX | (0 if blocking else fcntl.LOCK_NB)
        (fcntl.lockf if posix_record else fcntl.flock)(handle.fileno(), operation)


def unlock_handle(handle, *, posix_record=False):
    if os.name == 'nt':
        import msvcrt
        handle.seek(0)
        msvcrt.locking(handle.fileno(), msvcrt.LK_UNLCK, 1)
    else:
        import fcntl
        (fcntl.lockf if posix_record else fcntl.flock)(handle.fileno(), fcntl.LOCK_UN)


@contextmanager
def locked_file(path: Path, *, blocking=True):
    fd = os.open(path, os.O_CREAT | os.O_RDWR, 0o600)
    with os.fdopen(fd, 'r+b') as handle:
        lock_handle(handle, blocking=blocking)
        try:
            yield handle
        finally:
            unlock_handle(handle)
