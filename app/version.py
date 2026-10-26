"""Application version exposed in startup logs and CLI output."""
import re

VERSION = "0.2.2"


def native_version(value=VERSION):
    """Numeric file-version fields; the displayed version keeps the dev suffix."""
    match=re.fullmatch(r"(\d+)\.(\d+)\.(\d+)(?:\.dev([1-9]\d*))?",value)
    if not match:raise ValueError('Invalid application version')
    return tuple(int(part or 0) for part in match.groups())


def macos_build_version(value=VERSION):
    major,minor,patch,build=native_version(value)
    base=f'{major}.{minor}.{patch}'
    return f'{base}d{build}' if build else base
