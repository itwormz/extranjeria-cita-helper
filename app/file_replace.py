"""Atomic replace that tolerates a reader briefly holding the target on Windows."""
import os
import time

# Windows refuses MoveFileEx with ERROR_ACCESS_DENIED while another process
# (for example the window reading the same JSON) has the target open.
# Readers close quickly, so a short bounded backoff is enough; then give up.
RETRY_DELAYS = (0.01, 0.02, 0.05, 0.1, 0.2, 0.3)


def replace_file(source, target):
    for delay in RETRY_DELAYS:
        try:
            os.replace(source, target)
            return
        except PermissionError:
            time.sleep(delay)
    os.replace(source, target)
