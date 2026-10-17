"""Recover inherited IPC streams in Windows windowed builds without a console."""
import os
import sys


def restore_standard_streams():
    for name, fd, mode in [('stdin', 0, 'r'), ('stdout', 1, 'w'), ('stderr', 2, 'w')]:
        if getattr(sys, name) is not None:
            continue
        try:
            if sys.platform == 'win32':
                import ctypes
                from ctypes import wintypes
                import msvcrt
                kernel = ctypes.WinDLL('kernel32', use_last_error=True)
                kernel.GetStdHandle.argtypes = [wintypes.DWORD]
                kernel.GetStdHandle.restype = wintypes.HANDLE
                handle = kernel.GetStdHandle(-10 - fd)
                if handle in (None, 0, ctypes.c_void_p(-1).value):
                    raise OSError('No inherited standard handle')
                descriptor = msvcrt.open_osfhandle(handle, os.O_RDONLY if mode == 'r' else os.O_WRONLY)
            else:
                descriptor = os.dup(fd)
            stream = os.fdopen(descriptor, mode, encoding='utf-8', buffering=1)
        except OSError:
            stream = open(os.devnull, mode, encoding='utf-8')
        setattr(sys, name, stream)
