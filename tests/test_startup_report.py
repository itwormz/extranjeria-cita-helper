import json
import os
import pytest
from pathlib import Path


def test_startup_report_masks_private_values_and_records_missing_browser(tmp_path, monkeypatch):
    from app.desktop import startup_report as reports
    from app.logging_utils import register_sensitive
    monkeypatch.setattr(reports, "runtime_root", lambda: tmp_path)
    monkeypatch.setattr(reports, "bundled_browser_candidate", lambda: tmp_path / "missing-camoufox")
    register_sensitive(["PRIVATE APPLICANT", "secret@example.invalid"])
    error = RuntimeError(f"missing browser {Path.home()}/Library/test PRIVATE APPLICANT secret@example.invalid")
    report_id, summary = reports.save_startup_error(error, "browser_launch")
    path = tmp_path / "startup-errors" / (report_id + ".json")
    raw = path.read_text()
    data = json.loads(raw)
    assert data["phase"] == "browser_launch"
    assert data["browser"]["exists"] is False
    assert data["error_type"] == "RuntimeError"
    assert "PRIVATE APPLICANT" not in raw + summary
    assert "secret@example.invalid" not in raw + summary
    assert str(Path.home()) not in raw + summary
    assert "traceback" in data
    if os.name != "nt":
        assert path.stat().st_mode & 0o777 == 0o600


def test_report_write_failure_does_not_hide_original_error(tmp_path, monkeypatch):
    from app.desktop import startup_report as reports
    blocked = tmp_path / "file"
    blocked.write_text("not a directory")
    monkeypatch.setattr(reports, "runtime_root", lambda: blocked)
    report_id, summary = reports.save_startup_error(RuntimeError("browser unavailable"), "browser_launch")
    assert report_id is None
    assert "RuntimeError" in summary


@pytest.mark.parametrize("error_type", [FileNotFoundError, type("TargetClosedError", (Exception,), {})])
def test_browser_launch_failure_is_reported_with_diagnostics_disabled(tmp_path, monkeypatch, error_type):
    import asyncio
    from types import SimpleNamespace
    import playwright.async_api
    from app import assist_runtime, site
    from app.assist_control import AssistControl
    from app.config import Settings
    from app.desktop import startup_report as reports
    import pytest
    class Driver:
        async def start(self): return self
        async def stop(self): pass
    class Checker:
        def __init__(self, settings): self.settings = settings
        def _debug_directory(self, *args): return tmp_path
        async def launch_context(self, *args, **kwargs):
            raise error_type("bundled Camoufox missing")
    monkeypatch.setattr(playwright.async_api, "async_playwright", Driver)
    monkeypatch.setattr(site, "IcpPlusChecker", Checker)
    monkeypatch.setattr(reports, "runtime_root", lambda: tmp_path)
    control = AssistControl(); control.set_diagnostics(False)
    with pytest.raises(reports.StartupFailure, match=error_type.__name__):
        asyncio.run(assist_runtime._run_session(SimpleNamespace(id="test"), Settings(), control))
    assert len(list((tmp_path / "startup-errors").glob("*.json"))) == 1


def test_report_reader_rejects_paths_and_missing_files(tmp_path, monkeypatch):
    from app.desktop import startup_report as reports
    monkeypatch.setattr(reports, 'runtime_root', lambda: tmp_path)
    assert reports.read_startup_report('../tasks') is None
    assert reports.read_startup_report('20261019T123456-1234abcd') is None


def test_worker_stderr_summary_does_not_keep_arbitrary_values():
    from app.desktop.startup_report import worker_exit_summary
    stderr = 'Cookie: session=PRIVATE_COOKIE; full_name=FICTITIOUS PERSON; phone=612345678\nRuntimeError: FICTITIOUS PERSON 612345678 PRIVATE_COOKIE'
    summary = worker_exit_summary(3, stderr)
    assert 'RuntimeError' in summary
    for private in ('PRIVATE_COOKIE', 'FICTITIOUS PERSON', '612345678'):
        assert private not in summary
    assert "missing.module" in worker_exit_summary(1, "ModuleNotFoundError: No module named 'missing.module'")


def test_path_probe_distinguishes_missing_denied_and_broken_link(tmp_path, monkeypatch):
    import errno
    from app.desktop import startup_report as reports
    missing = reports.inspect_path(tmp_path / 'missing')
    assert missing['stat_error']['errno'] == errno.ENOENT
    link = tmp_path / 'broken'; link.symlink_to('missing')
    broken = reports.inspect_path(link)
    assert broken['is_symlink'] and broken['link_target'] == 'missing'
    assert broken['stat_error']['errno'] == errno.ENOENT
    original = Path.stat
    denied = tmp_path / 'denied'
    def stat(path, **kwargs):
        if path == denied: raise PermissionError(errno.EACCES, 'Permission denied')
        return original(path, **kwargs)
    monkeypatch.setattr(Path, 'stat', stat)
    assert reports.inspect_path(denied)['stat_error']['errno'] == errno.EACCES


def test_macos_bundle_checks_compare_worker_and_main_paths(tmp_path, monkeypatch):
    from app.desktop import startup_report as reports
    import sys
    main = tmp_path / 'Helper.app'
    helper = main / 'Contents/Helpers/Cita Worker.app/Contents'
    (helper / 'MacOS').mkdir(parents=True)
    (helper / 'Resources').symlink_to(Path('..') / '..' / '..' / 'Resources', target_is_directory=True)
    browser = main / 'Contents/Resources/bundled-browser/Camoufox.app/Contents/MacOS/camoufox'
    browser.parent.mkdir(parents=True); browser.write_text('synthetic'); browser.chmod(0o700)
    monkeypatch.setattr(sys, 'platform', 'darwin')
    monkeypatch.setattr(sys, 'frozen', True, raising=False)
    monkeypatch.setattr(sys, 'executable', str(helper / 'MacOS/Helper'))
    monkeypatch.setattr(reports, 'verify_signature', lambda path: {'status':'valid'})
    checks = reports.bundle_checks()
    assert checks['main_browser']['exists']
    assert checks['worker_browser']['exists']
    assert checks['worker_resources']['is_symlink']
    assert checks['worker_resources']['resolved'] == reports.clean(main / 'Contents/Resources')
    assert checks['signatures']['application']['status'] == 'valid'
    (helper / 'Resources').unlink()
    (helper / 'Resources').symlink_to(Path('..') / '..' / '..' / 'MissingResources', target_is_directory=True)
    checks = reports.bundle_checks(external_checks=False)
    assert checks['main_browser']['exists']
    assert not checks['worker_browser']['exists']
    assert checks['worker_resources']['is_symlink']
    assert checks['signatures']['application']['status'] == 'not_checked'


def test_signature_probe_is_read_only_and_bounded(tmp_path, monkeypatch):
    import subprocess
    from app.desktop import startup_report as reports
    calls = []
    def run(args, **kwargs):
        calls.append((args, kwargs))
        raise subprocess.TimeoutExpired(args, kwargs['timeout'])
    monkeypatch.setattr(reports.subprocess, 'run', run)
    assert reports.verify_signature(tmp_path)['status'] == 'timeout'
    args, options = calls[0]
    assert args[:4] == ['/usr/bin/codesign','--verify','--deep','--strict']
    assert options['timeout'] <= 5
    assert '--force' not in args and '--sign' not in args


def test_copy_preserves_complete_structured_report():
    from app.desktop.startup_report import report_clipboard_text
    report = {'parts':[{'detail':'technical detail ' * 500} for _ in range(5)]}
    text = report_clipboard_text('20261019T123456-1234abcd', report)
    assert json.loads(text.split('\n', 1)[1]) == report
