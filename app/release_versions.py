"""Versions accepted from published stable and beta release tags."""
import re

RELEASE_VERSION = r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-beta\.([1-9][0-9]*))?'


def release_key(value):
    match = re.fullmatch(RELEASE_VERSION, value) if isinstance(value, str) else None
    if not match:
        raise ValueError('Invalid release version')
    major, minor, patch, beta = match.groups()
    return (int(major), int(minor), int(patch), 1 if beta else 2, int(beta or 0))
