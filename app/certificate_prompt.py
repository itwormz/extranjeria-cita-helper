"""Read native approval-window metadata; never capture or activate a window."""
import ctypes
import os
import plistlib
import sys


def certificate_prompt_ids():
    try:
        if sys.platform=='darwin':return _macos_prompt_ids()
        if sys.platform=='win32':return _windows_prompt_ids()
    except (OSError,AttributeError,ValueError,TypeError):pass
    return set()


def _macos_prompt_ids():
    cg=ctypes.CDLL('/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics');cf=ctypes.CDLL('/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation');pointer=ctypes.c_void_p
    cg.CGWindowListCopyWindowInfo.argtypes=[ctypes.c_uint32,ctypes.c_uint32];cg.CGWindowListCopyWindowInfo.restype=pointer
    cf.CFPropertyListCreateData.argtypes=[pointer,pointer,ctypes.c_long,ctypes.c_ulong,ctypes.POINTER(pointer)];cf.CFPropertyListCreateData.restype=pointer
    cf.CFDataGetLength.argtypes=[pointer];cf.CFDataGetLength.restype=ctypes.c_long;cf.CFDataGetBytePtr.argtypes=[pointer];cf.CFDataGetBytePtr.restype=pointer;cf.CFRelease.argtypes=[pointer]
    windows=cg.CGWindowListCopyWindowInfo(1,0);data=None
    if not windows:return set()
    try:
        data=cf.CFPropertyListCreateData(None,windows,200,0,None)
        if not data:return set()
        rows=plistlib.loads(ctypes.string_at(cf.CFDataGetBytePtr(data),cf.CFDataGetLength(data)))
        return {row['kCGWindowNumber'] for row in rows if row.get('kCGWindowOwnerName')=='SecurityAgent' and row.get('kCGWindowIsOnscreen',True)}
    finally:
        if data:cf.CFRelease(data)
        cf.CFRelease(windows)


def _windows_prompt_ids():
    from ctypes import wintypes as w
    from .browser_visibility import _processes,owned_browser_pids
    pids=set(owned_browser_pids(_processes(),os.getpid()));user=ctypes.WinDLL('user32',use_last_error=True);callback=ctypes.WINFUNCTYPE(w.BOOL,w.HWND,w.LPARAM)
    user.GetWindowThreadProcessId.argtypes=[w.HWND,ctypes.POINTER(w.DWORD)];user.IsWindowVisible.argtypes=[w.HWND];user.GetWindowTextW.argtypes=[w.HWND,w.LPWSTR,ctypes.c_int];found=set()
    @callback
    def collect(window,_):
        owner=w.DWORD();user.GetWindowThreadProcessId(window,ctypes.byref(owner))
        if owner.value in pids and user.IsWindowVisible(window):
            title=ctypes.create_unicode_buffer(512);user.GetWindowTextW(window,title,512)
            if any(word in title.value.casefold() for word in ('certificate','certificado','certificat','zertifikat')):found.add(int(window))
        return True
    user.EnumWindows(collect,0);return found
