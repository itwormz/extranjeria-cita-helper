"""Read the macOS login-session lock without changing desktop settings."""
import ctypes
from functools import lru_cache
import sys

@lru_cache(maxsize=1)
def _frameworks():
    cg=ctypes.CDLL('/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics')
    cf=ctypes.CDLL('/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation')
    cg.CGSessionCopyCurrentDictionary.restype=ctypes.c_void_p
    cf.CFStringCreateWithCString.argtypes=[ctypes.c_void_p,ctypes.c_char_p,ctypes.c_uint32]
    cf.CFStringCreateWithCString.restype=ctypes.c_void_p
    cf.CFDictionaryGetValue.argtypes=[ctypes.c_void_p,ctypes.c_void_p]
    cf.CFDictionaryGetValue.restype=ctypes.c_void_p
    cf.CFBooleanGetValue.argtypes=[ctypes.c_void_p]
    cf.CFBooleanGetValue.restype=ctypes.c_bool
    cf.CFRelease.argtypes=[ctypes.c_void_p]
    return cg,cf

def screen_locked():
    if sys.platform!='darwin':return False
    try:
        cg,cf=_frameworks()
        session=cg.CGSessionCopyCurrentDictionary()
        if not session:return False
        key=None
        try:
            # This optional key is provided by WindowServer; absence means
            # unlocked. Missing frameworks/session never imply a lock.
            key=cf.CFStringCreateWithCString(None,b'CGSSessionScreenIsLocked',0x08000100)
            if not key:return False
            value=cf.CFDictionaryGetValue(session,key)
            return bool(value and cf.CFBooleanGetValue(value))
        finally:
            if key:cf.CFRelease(key)
            cf.CFRelease(session)
    except (OSError,AttributeError):return False
