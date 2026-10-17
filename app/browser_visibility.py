"""Raise only the browser owned by this worker, on explicit user request."""
import asyncio
import ctypes
import os
import subprocess
import sys


def _processes():
    if sys.platform != 'win32':
        rows = subprocess.check_output(['/bin/ps', '-axo', 'pid=,ppid=,comm='], text=True, timeout=3)
        return [(int(pid), int(parent), name.strip()) for row in rows.splitlines()
                for pid, parent, name in [row.strip().split(None, 2)]]
    from ctypes import wintypes as w
    class Entry(ctypes.Structure):
        _fields_ = [('size',w.DWORD),('usage',w.DWORD),('pid',w.DWORD),('heap',ctypes.c_size_t),
                    ('module',w.DWORD),('threads',w.DWORD),('parent',w.DWORD),
                    ('priority',w.LONG),('flags',w.DWORD),('name',w.WCHAR*260)]
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.CreateToolhelp32Snapshot.restype = w.HANDLE
    kernel.Process32FirstW.argtypes = [w.HANDLE,ctypes.POINTER(Entry)]
    kernel.Process32NextW.argtypes = [w.HANDLE,ctypes.POINTER(Entry)]
    kernel.CloseHandle.argtypes = [w.HANDLE]
    snapshot = kernel.CreateToolhelp32Snapshot(2,0)
    if snapshot == w.HANDLE(-1).value:
        raise ctypes.WinError(ctypes.get_last_error())
    result=[]; entry=Entry();entry.size=ctypes.sizeof(entry)
    try:
        valid=kernel.Process32FirstW(snapshot,ctypes.byref(entry))
        while valid:
            result.append((entry.pid,entry.parent,entry.name))
            valid=kernel.Process32NextW(snapshot,ctypes.byref(entry))
    finally:
        kernel.CloseHandle(snapshot)
    return result


def owned_browser_pids(rows, root_pid):
    parents={pid:parent for pid,parent,_ in rows}
    candidates=[]
    for pid,_,name in rows:
        if name.rsplit('/',1)[-1].lower() not in {'camoufox','camoufox-bin','camoufox.exe'}:
            continue
        current=pid;seen=set();depth=0
        while current in parents and current not in seen:
            seen.add(current);current=parents[current];depth+=1
            if current==root_pid:
                candidates.append((depth,pid));break
    return [pid for _,pid in sorted(candidates)]


def owned_browser_pid(rows, root_pid):
    pids=owned_browser_pids(rows,root_pid)
    return pids[0] if pids else None


def _activate_macos(pid):
    ctypes.CDLL('/System/Library/Frameworks/AppKit.framework/AppKit')
    objc=ctypes.CDLL('/usr/lib/libobjc.A.dylib')
    objc.objc_getClass.argtypes=[ctypes.c_char_p];objc.objc_getClass.restype=ctypes.c_void_p
    objc.sel_registerName.argtypes=[ctypes.c_char_p];objc.sel_registerName.restype=ctypes.c_void_p
    def call(result, args, receiver, selector, *values):
        method=ctypes.CFUNCTYPE(result,ctypes.c_void_p,ctypes.c_void_p,*args)(('objc_msgSend',objc))
        return method(receiver,objc.sel_registerName(selector),*values)
    app=call(ctypes.c_void_p,[ctypes.c_int],objc.objc_getClass(b'NSRunningApplication'),
             b'runningApplicationWithProcessIdentifier:',pid)
    if not app:
        return False
    call(ctypes.c_bool,[],app,b'unhide')
    return bool(call(ctypes.c_bool,[ctypes.c_ulong],app,b'activateWithOptions:',3))


def allow_worker_foreground(pid):
    """Transfer a user-requested foreground activation to our worker on Windows."""
    if sys.platform != 'win32' or pid <= 0:
        return False
    from ctypes import wintypes as w
    try:
        user = ctypes.WinDLL('user32', use_last_error=True)
        user.AllowSetForegroundWindow.argtypes = [w.DWORD]
        user.AllowSetForegroundWindow.restype = w.BOOL
        return bool(user.AllowSetForegroundWindow(pid))
    except (OSError, AttributeError):
        return False


def _activate_windows(pids):
    from ctypes import wintypes as w
    user=ctypes.WinDLL('user32',use_last_error=True)
    callback_type=ctypes.WINFUNCTYPE(w.BOOL,w.HWND,w.LPARAM)
    user.GetWindowThreadProcessId.argtypes=[w.HWND,ctypes.POINTER(w.DWORD)]
    user.IsWindowVisible.argtypes=[w.HWND]
    user.ShowWindow.argtypes=[w.HWND,ctypes.c_int]
    user.SetForegroundWindow.argtypes=[w.HWND]
    user.GetForegroundWindow.restype=w.HWND
    windows=[]
    @callback_type
    def collect(window,_):
        owner=w.DWORD();user.GetWindowThreadProcessId(window,ctypes.byref(owner))
        if owner.value in pids and user.IsWindowVisible(window):
            windows.append(window)
        return True
    user.EnumWindows(collect,0)
    if not windows:
        return False
    user.ShowWindow(windows[0],9)  # SW_RESTORE also restores a minimized window.
    return bool(user.SetForegroundWindow(windows[0])) or user.GetForegroundWindow()==windows[0]


def _activate_x11(pid):
    from ctypes.util import find_library
    library=find_library('X11')
    if not library or not os.environ.get('DISPLAY'):
        return False
    x=ctypes.CDLL(library)
    P=ctypes.c_void_p;L=ctypes.c_ulong;I=ctypes.c_int
    x.XOpenDisplay.argtypes=[ctypes.c_char_p];x.XOpenDisplay.restype=P
    x.XDefaultRootWindow.argtypes=[P];x.XDefaultRootWindow.restype=L
    x.XInternAtom.argtypes=[P,ctypes.c_char_p,I];x.XInternAtom.restype=L
    x.XGetWindowProperty.argtypes=[P,L,L,ctypes.c_long,ctypes.c_long,I,L,ctypes.POINTER(L),ctypes.POINTER(I),ctypes.POINTER(L),ctypes.POINTER(L),ctypes.POINTER(P)]
    x.XFree.argtypes=[P];x.XMapRaised.argtypes=[P,L]
    x.XFlush.argtypes=[P];x.XCloseDisplay.argtypes=[P]
    x.XSendEvent.argtypes=[P,L,I,ctypes.c_long,P]
    display=x.XOpenDisplay(None)
    if not display:
        return False
    def atom(name):return x.XInternAtom(display,name,False)
    def values(window,name):
        actual=L();format_=I();count=L();after=L();data=P()
        result=x.XGetWindowProperty(display,window,atom(name),0,4096,False,0,
             ctypes.byref(actual),ctypes.byref(format_),ctypes.byref(count),ctypes.byref(after),ctypes.byref(data))
        try:
            if result or not data or format_.value!=32:return []
            return list(ctypes.cast(data,ctypes.POINTER(L))[:count.value])
        finally:
            if data:x.XFree(data)
    class Data(ctypes.Union):
        _fields_=[('l',ctypes.c_long*5)]
    class Message(ctypes.Structure):
        _fields_=[('type',I),('serial',L),('send_event',I),('display',P),
                  ('window',L),('message_type',L),('format',I),('data',Data)]
    class Event(ctypes.Union):
        _fields_=[('message',Message),('pad',ctypes.c_long*24)]
    try:
        root=x.XDefaultRootWindow(display)
        window=next((window for window in values(root,b'_NET_CLIENT_LIST')
                     if values(window,b'_NET_WM_PID')==[pid]),None)
        if not window:return False
        x.XMapRaised(display,window)
        event=Event()
        event.message=Message(33,0,True,display,window,atom(b'_NET_ACTIVE_WINDOW'),32,Data())
        event.message.data.l[0]=2  # Explicit user action from a desktop controller.
        sent=x.XSendEvent(display,root,False,(1<<19)|(1<<20),ctypes.byref(event))
        x.XFlush(display)
        return bool(sent)
    finally:
        x.XCloseDisplay(display)


def activate_native_browser(pid=None):
    if sys.platform=='win32':
        rows=_processes()
        pids=owned_browser_pids(rows,os.getpid())
        if pid and pid not in pids:
            return False
        return _activate_windows(pids) if pids else False
    pid=pid or owned_browser_pid(_processes(),os.getpid())
    if not pid:
        return False
    if sys.platform=='darwin':return _activate_macos(pid)
    return _activate_x11(pid)


async def show_browser(page, control):
    try:
        await page.bring_to_front()
        ok=await asyncio.to_thread(activate_native_browser, getattr(page.context,'_cita_browser_pid',None))
    except Exception:
        ok=False
    control.record('browser_shown' if ok else 'browser_show_failed',
                   detail='Camoufox показан' if ok else 'Не удалось показать Camoufox — откройте его вручную')
    return ok
