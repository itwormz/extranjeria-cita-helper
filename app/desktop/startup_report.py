"""Small local startup reports; never collect page content or profile settings."""
import json
import os
from pathlib import Path
import platform
import re
import sys
import stat
import subprocess
import traceback
from datetime import datetime, timezone
from uuid import uuid4

from .paths import runtime_root, bundled_browser_candidate
from ..logging_utils import sanitize
from ..version import VERSION


class StartupFailure(RuntimeError):
    """A startup error whose safe explanation can be shown by the controller."""


def clean(value):
    text = str(value).replace(str(Path.home()), "~")
    text = re.sub(r"/Users/[^/\s]+", "/Users/[user]", text)
    text = re.sub(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+", "[email]", text)
    text = re.sub(r"(?im)\b(?:cookie|set-cookie|authorization)\s*:[^\r\n]*", "[private header]", text)
    text = re.sub(r"(?i)\b(?:access_token|refresh_token|password|pin|sessionid|csrf)\s*[=:]\s*[^\s;]+", "[private value]", text)
    return sanitize(text)[:16000]


def save_startup_error(exc, phase, *, external_checks=True):
    """Return a report ID and safe summary, even when writing is impossible."""
    summary = f"{type(exc).__name__}: {clean(exc)[:600]}"
    try:
        browser = bundled_browser_candidate()
        report = {
            "version": VERSION, "time": datetime.now(timezone.utc).isoformat(),
            "system": platform.system(), "release": platform.release(),
            "macos": platform.mac_ver()[0], "architecture": platform.machine(),
            "frozen": bool(getattr(sys, "frozen", False)),
            "executable": clean(sys.executable), "phase": phase,
            "browser": inspect_path(browser) if browser else {"path": None, "exists": False},
            "bundle_checks": bundle_checks(external_checks=external_checks),
            "error_type": type(exc).__name__, "message": clean(exc),
            "traceback": [{"file": clean(frame.filename), "line": frame.lineno,
                           "function": frame.name} for frame in traceback.extract_tb(exc.__traceback__)],
        }
        folder = runtime_root() / "startup-errors"
        folder.mkdir(parents=True, exist_ok=True, mode=0o700)
        # Retain only a bounded history of technical startup failures.
        for old in sorted(folder.glob("*.json"), key=lambda p: p.stat().st_mtime, reverse=True)[9:]:
            old.unlink()
        report_id = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S") + "-" + uuid4().hex[:8]
        path = folder / (report_id + ".json")
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            json.dump(report, handle, ensure_ascii=False, indent=2)
        return report_id, summary
    except Exception:
        return None, summary


def report_id_from_message(message):
    match = re.search(r"startup-errors/([0-9]{8}T[0-9]{6}-[a-f0-9]{8})\.json", message)
    return match.group(1) if match else None


def read_startup_report(report_id):
    if not report_id or not re.fullmatch(r"[0-9]{8}T[0-9]{6}-[a-f0-9]{8}", report_id):
        return None
    try:
        path = runtime_root() / 'startup-errors' / (report_id + '.json')
        if path.is_symlink() or path.stat().st_size > 128 * 1024:
            return None
        report = json.loads(path.read_text(encoding='utf-8'))
        return report if isinstance(report, dict) else None
    except (OSError, ValueError):
        return None


def report_clipboard_text(report_id, report):
    # Reapply masking when copying: old reports may predate a masking change.
    def scrub(value):
        if isinstance(value, dict): return {key: scrub(item) for key, item in value.items()}
        if isinstance(value, list): return [scrub(item) for item in value]
        return clean(value) if isinstance(value, str) else value
    return ('Cita Extranjeria Helper — CEH-' + report_id + '\n'
            + json.dumps(scrub(report), ensure_ascii=False, indent=2))


def worker_exit_summary(code, stderr):
    """Extract only recognized technical identifiers, never arbitrary stderr text."""
    kinds = re.findall(r"(?m)^([A-Za-z_][A-Za-z0-9_]*(?:Error|Exception)):", stderr)
    kind = kinds[-1] if kinds else 'WorkerExit'
    summary = f'Worker exit {code}: {kind}'
    missing = re.search(r"ModuleNotFoundError: No module named ['\"]([A-Za-z_][A-Za-z0-9_.]*)['\"]", stderr)
    if missing:
        summary += ' — missing module: ' + missing.group(1)
    return summary


def os_error(exc):
    return {"type": type(exc).__name__, "errno": exc.errno,
            "reason": os.strerror(exc.errno) if exc.errno else type(exc).__name__}


def inspect_path(path):
    """Use stat explicitly so access denial is not confused with a missing file."""
    path = Path(path)
    result = {"path": clean(path), "exists": False, "executable": False}
    try:
        info = path.lstat()
        result['is_symlink'] = stat.S_ISLNK(info.st_mode)
        if result['is_symlink']:
            result['link_target'] = clean(os.readlink(path))
    except OSError as exc:
        result['lstat_error'] = os_error(exc)
    try:
        info = path.stat()
        result.update(exists=True, is_file=stat.S_ISREG(info.st_mode),
                      mode=oct(stat.S_IMODE(info.st_mode)), executable=os.access(path, os.X_OK))
    except OSError as exc:
        result['stat_error'] = os_error(exc)
    try:
        result['resolved'] = clean(path.resolve(strict=True))
    except (OSError, RuntimeError) as exc:
        result['resolve_error'] = os_error(exc) if isinstance(exc, OSError) else {"type": type(exc).__name__}
    return result


def verify_signature(path):
    """Read-only integrity check; valid does not imply notarized or Gatekeeper-approved."""
    try:
        result = subprocess.run(['/usr/bin/codesign', '--verify', '--deep', '--strict', str(path)],
                                capture_output=True, timeout=3, check=False)
        return {"status": 'valid' if result.returncode == 0 else 'invalid',
                "exit_code": result.returncode, "detail": clean(result.stderr.decode('utf-8', errors='replace')[:4000])}
    except subprocess.TimeoutExpired:
        return {"status": 'timeout'}
    except OSError as exc:
        return {"status": 'unavailable', "error": os_error(exc)}


def bundle_checks(*, external_checks=True):
    if sys.platform != 'darwin' or not getattr(sys, 'frozen', False):
        return {"status": 'not_applicable'}
    executable = Path(sys.executable)
    apps = [parent for parent in executable.parents if parent.suffix == '.app']
    if not apps:
        return {"status": 'bundle_not_identified'}
    # Lexical paths preserve the worker resource link for comparison.
    main = apps[-1]
    current_contents = executable.parent.parent
    relative = Path('bundled-browser/Camoufox.app/Contents/MacOS/camoufox')
    main_browser = main / 'Contents/Resources' / relative
    worker = main / 'Contents/Helpers/Cita Worker.app/Contents'
    return {"status": 'checked', "translocated": '/AppTranslocation/' in str(executable),
            "application": inspect_path(main),
            "current_resources": inspect_path(current_contents / 'Resources'),
            "worker_resources": inspect_path(worker / 'Resources'),
            "worker_frameworks": inspect_path(worker / 'Frameworks'),
            "main_browser": inspect_path(main_browser),
            "worker_browser": inspect_path(worker / 'Resources' / relative),
            "quarantine": {"application": quarantine_state(main) if external_checks else {"status": "not_checked", "reason": "gui_crash_fallback"},
                           "browser": quarantine_state(main_browser.parents[2]) if external_checks else {"status": "not_checked", "reason": "gui_crash_fallback"}},
            "signatures": {"application": verify_signature(main) if external_checks else {"status": "not_checked", "reason": "gui_crash_fallback"},
                           "browser": verify_signature(main_browser.parents[2]) if external_checks else {"status": "not_checked", "reason": "gui_crash_fallback"}}}


def quarantine_state(path):
    """Record only presence of quarantine, never its source URL or metadata."""
    try:
        result = subprocess.run(['/usr/bin/xattr', str(path)], capture_output=True, timeout=1, check=False)
        if result.returncode:
            return {"status": 'unavailable', "exit_code": result.returncode}
        return {"status": 'checked', "present": b'com.apple.quarantine' in result.stdout.splitlines()}
    except subprocess.TimeoutExpired:
        return {"status": 'timeout'}
    except OSError as exc:
        return {"status": 'unavailable', "error": os_error(exc)}
