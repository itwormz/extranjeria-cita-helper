"""Worker failures must leave a structured report instead of a bare exit code."""
import asyncio
import io
import json
import logging
import os
import sys
from dataclasses import dataclass, field
from datetime import date
from pathlib import Path

import pytest

from app.config import AppointmentProtection


@dataclass
class FakeTask:
    id: str = 'fictitious-profile'
    errors: list = field(default_factory=list)
    access_mode: str = 'certificate'
    applicant_fill_mode: str = 'profile'
    document_type: str = 'NIE'
    document_number: str = 'Y0000000Z'
    full_name: str = 'TEST APPLICANT'
    contact_phone: str = '600000000'
    contact_email: str = 'applicant@example.invalid'
    appointment_protection: AppointmentProtection = field(default_factory=AppointmentProtection)


class FakeProfiles:
    def __init__(self, root):
        self.path = root / 'tasks.yml'

    def preferences(self):
        return {'diagnostics': False, 'sound': False}

    def load(self):
        return [FakeTask()]

    def settings(self, _task_id):
        return None


class FakeStatistics:
    def __init__(self, _path):
        pass

    def total(self, *_args):
        return 0


class FakeSession:
    def __init__(self, *_args):
        pass

    def close(self, _reason):
        pass


class Stdout:
    def __init__(self, capture):
        self.capture = capture

    def messages(self):
        out = self.capture.readouterr().out
        return [json.loads(line) for line in out.splitlines() if line.strip()]


@pytest.fixture
def worker_env(tmp_path, monkeypatch, capsysbinary):
    from app import main
    from app.desktop import startup_report, statistics, statistics_session, worker

    monkeypatch.setattr(main, '_paths', lambda: (tmp_path / 'tasks.yml', tmp_path / 'catalog.json',
                                                 tmp_path / 'data' / 'state.db'))
    monkeypatch.setattr(main, 'ROOT', tmp_path)
    monkeypatch.setattr(worker, 'ProfileStore', lambda *_: FakeProfiles(tmp_path))
    monkeypatch.setattr(statistics, 'ProfileStatistics', FakeStatistics)
    monkeypatch.setattr(statistics_session, 'StatisticsSession', FakeSession)
    monkeypatch.setattr(startup_report, 'runtime_root', lambda: tmp_path)
    monkeypatch.setattr(startup_report, 'bundled_browser_candidate', lambda: None)
    stdout = Stdout(capsysbinary)

    async def idle_commands(_stream, _control, _emit):
        await asyncio.Event().wait()

    monkeypatch.setattr(worker, 'consume_commands', idle_commands)
    monkeypatch.setattr(sys, 'stdin', type('Stdin', (), {'buffer': io.BytesIO()})())
    return tmp_path, stdout


def reports(root):
    return [json.loads(path.read_text(encoding='utf-8'))
            for path in sorted((root / 'startup-errors').glob('*.json'))]


def test_budget_rollover_save_failure_keeps_worker_alive(worker_env, monkeypatch, caplog):
    root, stdout = worker_env
    from app.desktop import worker

    async def assist(task, _settings, *, control, open_panel):
        def denied(_budget):
            raise PermissionError(13, 'Access is denied')
        control.save_appointment_budget = denied
        control.appointment_budget.day = date(2000, 1, 1)
        await asyncio.sleep(0.7)
        return 0

    monkeypatch.setattr(worker, 'run_assist', assist)
    with caplog.at_level(logging.WARNING):
        assert asyncio.run(worker.run_worker('fictitious-profile')) == 0
    assert 'event=appointment_budget_save_failed' in caplog.text
    assert 'Y0000000Z' not in caplog.text
    assert any(m['type'] == 'state' for m in stdout.messages())
    assert not reports(root)


def test_background_task_failure_is_reported_not_raised(worker_env, monkeypatch):
    root, stdout = worker_env
    from app.desktop import worker

    async def broken_commands(_stream, _control, _emit):
        raise PermissionError(13, 'Access is denied')

    async def assist(task, _settings, *, control, open_panel):
        await asyncio.sleep(0.05)
        return 0

    monkeypatch.setattr(worker, 'consume_commands', broken_commands)
    monkeypatch.setattr(worker, 'run_assist', assist)
    # A reported background failure must not turn a run into a successful exit.
    assert asyncio.run(worker.run_worker('fictitious-profile')) == 1
    errors = [m for m in stdout.messages() if m['type'] == 'error']
    assert errors and 'startup-errors/' in errors[-1]['message']
    report, = reports(root)
    assert report['phase'] == 'worker_background'
    assert report['error_type'] == 'PermissionError'
    assert any(frame['function'] == 'broken_commands' for frame in report['traceback'])


def test_escaping_worker_exception_writes_report_with_frames(worker_env, monkeypatch):
    root, stdout = worker_env
    from app.desktop import worker

    def denied(*_args):
        raise PermissionError(13, 'Access is denied', str(root / 'Y0000000Z.json'))

    monkeypatch.setattr(worker, 'ProfileStore', denied)
    assert worker.worker_main('fictitious-profile') == 1
    errors = [m for m in stdout.messages() if m['type'] == 'error']
    assert errors and 'startup-errors/' in errors[-1]['message']
    report, = reports(root)
    assert report['phase'] == 'worker'
    assert report['error_type'] == 'PermissionError'
    assert any(frame['function'] == 'run_worker' for frame in report['traceback'])
    assert any(frame['function'] == 'denied' for frame in report['traceback'])


def test_main_worker_branch_uses_reporting_entry(monkeypatch):
    from app import main
    from app.desktop import worker
    calls = []
    monkeypatch.setattr(worker, 'worker_main', lambda task_id: calls.append(task_id) or 7)
    assert main.main(['worker', '--task', 'fictitious-profile']) == 7
    assert calls == ['fictitious-profile']


def test_worker_exit_report_keeps_code_locations_from_stderr():
    from app.desktop.startup_report import worker_exit_frames
    stderr = ('Traceback (most recent call last):\n'
              '  File "C:\\Users\\Someone\\Cita\\_internal\\app\\desktop\\worker.py", line 311, in run_worker\n'
              '    await pending\n'
              '  File "/Users/someone/cita/app/appointment_budget_store.py", line 98, in _write_path\n'
              '    os.replace(temporary, path)\n'
              '  File "<frozen runpy>", line 88, in _run_code\n'
              "PermissionError: [WinError 5] Access is denied: 'C:\\\\Users\\\\Someone\\\\x.json'\n")
    frames = worker_exit_frames(stderr)
    assert frames == [
        {'file': 'app/desktop/worker.py', 'line': 311, 'function': 'run_worker'},
        {'file': 'app/appointment_budget_store.py', 'line': 98, 'function': '_write_path'},
        {'file': '<frozen runpy>', 'line': 88, 'function': '_run_code'},
    ]
    assert 'Someone' not in json.dumps(frames) and 'someone' not in json.dumps(frames)


def test_worker_exit_report_records_parsed_frames(tmp_path, monkeypatch):
    from app.desktop import startup_report as reports_module
    monkeypatch.setattr(reports_module, 'runtime_root', lambda: tmp_path)
    monkeypatch.setattr(reports_module, 'bundled_browser_candidate', lambda: None)
    frames = [{'file': 'app/desktop/worker.py', 'line': 311, 'function': 'run_worker'}]
    report_id, _ = reports_module.save_startup_error(
        RuntimeError('Worker exit 1: PermissionError'), 'worker_exit', external_checks=False, frames=frames)
    report = json.loads((tmp_path / 'startup-errors' / (report_id + '.json')).read_text())
    assert report['traceback'] == frames


def test_replace_retries_transient_permission_error(tmp_path, monkeypatch):
    from app import file_replace
    calls = []
    real = os.replace

    def flaky(source, target):
        calls.append(target)
        if len(calls) < 3:
            raise PermissionError(13, 'Access is denied')
        real(source, target)

    monkeypatch.setattr(file_replace.os, 'replace', flaky)
    monkeypatch.setattr(file_replace.time, 'sleep', lambda _s: None)
    source, target = tmp_path / 'a', tmp_path / 'b'
    source.write_text('new')
    file_replace.replace_file(source, target)
    assert target.read_text() == 'new' and len(calls) == 3


def test_replace_gives_up_after_bounded_attempts(tmp_path, monkeypatch):
    from app import file_replace
    delays = []

    def denied(_source, _target):
        raise PermissionError(13, 'Access is denied')

    monkeypatch.setattr(file_replace.os, 'replace', denied)
    monkeypatch.setattr(file_replace.time, 'sleep', delays.append)
    with pytest.raises(PermissionError):
        file_replace.replace_file(tmp_path / 'a', tmp_path / 'b')
    assert 0 < len(delays) <= 10 and sum(delays) < 2


def test_replace_does_not_retry_other_errors(tmp_path, monkeypatch):
    from app import file_replace
    monkeypatch.setattr(file_replace.time, 'sleep', lambda _s: pytest.fail('unexpected retry'))
    with pytest.raises(FileNotFoundError):
        file_replace.replace_file(tmp_path / 'missing', tmp_path / 'b')


@pytest.mark.parametrize('writer', ['budget', 'profiles', 'achievements'])
def test_atomic_writers_use_retrying_replace(tmp_path, monkeypatch, writer):
    from app import file_replace
    attempts = []
    real = os.replace

    def flaky(source, target):
        attempts.append(target)
        if len(attempts) == 1:
            raise PermissionError(13, 'Access is denied')
        real(source, target)

    monkeypatch.setattr(file_replace.os, 'replace', flaky)
    monkeypatch.setattr(file_replace.time, 'sleep', lambda _s: None)
    if writer == 'budget':
        from app.appointment_budget import AppointmentBudget
        from app.appointment_budget_store import BudgetStore
        store = BudgetStore(tmp_path / 'budget', FakeTask())
        store.save(AppointmentBudget())
        assert store.path.exists()
    elif writer == 'profiles':
        from app.desktop.profiles import ProfileStore
        store = ProfileStore(tmp_path / 'tasks.yml', Path('catalogs/snapshot-2026-08-12.json'))
        store._write({'tasks': []}, store.revision())
        assert (tmp_path / 'tasks.yml').exists()
    else:
        from app.desktop.achievements import AchievementStore
        store = AchievementStore(tmp_path / 'achievements.json')
        store._write({})
        assert (tmp_path / 'achievements.json').exists()
    assert len(attempts) == 2
    assert not [p for p in tmp_path.rglob('*') if p.name.startswith(('.profiles-', '.achievements-', 'tmp'))]
