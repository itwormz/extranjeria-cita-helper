"""Small local startup reports; never collect page content or profile settings."""
import json
import os
from pathlib import Path
import platform
import re
import sys
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


def save_startup_error(exc, phase):
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
            "browser": {"path": clean(browser) if browser else None,
                        "exists": bool(browser and browser.is_file()),
                        "executable": bool(browser and os.access(browser, os.X_OK))},
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
    return ('Cita Extranjeria Helper — CEH-' + report_id + '\n'
            + clean(json.dumps(report, ensure_ascii=False, indent=2)).replace('\\n', '\n'))


def worker_exit_summary(code, stderr):
    """Extract only recognized technical identifiers, never arbitrary stderr text."""
    kinds = re.findall(r"(?m)^([A-Za-z_][A-Za-z0-9_]*(?:Error|Exception)):", stderr)
    kind = kinds[-1] if kinds else 'WorkerExit'
    summary = f'Worker exit {code}: {kind}'
    missing = re.search(r"ModuleNotFoundError: No module named ['\"]([A-Za-z_][A-Za-z0-9_.]*)['\"]", stderr)
    if missing:
        summary += ' — missing module: ' + missing.group(1)
    return summary
