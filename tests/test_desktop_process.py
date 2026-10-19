import os
os.environ.setdefault('QT_QPA_PLATFORM','offscreen')
import sys
import time
import pytest
pytest.importorskip('PySide6')
from PySide6.QtWidgets import QApplication
from PySide6.QtCore import QProcess
from PySide6.QtTest import QTest
from app.desktop.process import DesktopController
from app.desktop.profiles import ProfileStore


def test_real_process_partial_json_and_graceful_stop(tmp_path,catalog_path,qt_app):
    app=qt_app
    controller=DesktopController(ProfileStore(tmp_path/'tasks.yml',catalog_path))
    states=[]; done=[]; errors=[]
    controller.state_changed.connect(states.append);controller.finished.connect(done.append);controller.failed.connect(errors.append)
    script="import sys,time,json;sys.stdout.write('{\"type\":\"state\",');sys.stdout.flush();time.sleep(.1);print('\"state\":{\"screen\":\"test\"}}',flush=True);line=sys.stdin.readline();sys.exit(0 if json.loads(line)['command']=='stop' else 4)"
    controller.process.start(sys.executable,['-c',script])
    deadline=time.monotonic()+5
    while not states and time.monotonic()<deadline: QTest.qWait(25)
    assert states[-1]['screen']=='test'
    controller.last_message=time.monotonic()-5;controller.check_heartbeat()
    assert states[-1]['connection_lost']
    controller.shutdown()
    while not done and time.monotonic()<deadline: QTest.qWait(25)
    assert done==[0] and not controller.active
    assert not controller.terminate_timer.isActive()
    assert not errors


def test_retained_browser_acknowledgement_disarms_forced_shutdown(tmp_path,catalog_path,qt_app):
    controller=DesktopController(ProfileStore(tmp_path/'tasks.yml',catalog_path))
    script="import sys,json,time;sys.stdin.readline();print(json.dumps({'type':'state','state':{'browser_retained':True}}),flush=True);sys.stdin.readline()"
    controller.process.start(sys.executable,['-c',script])
    assert controller.process.waitForStarted(2000)
    controller.shutdown()
    deadline=time.monotonic()+3
    while not controller.last_state.get('browser_retained') and time.monotonic()<deadline: QTest.qWait(25)
    assert controller.last_state.get('browser_retained')
    assert controller.active and not controller.terminate_timer.isActive() and not controller.kill_timer.isActive()
    controller.process.write(b'close\n')
    assert controller.process.waitForFinished(2000)


def test_close_after_stop_sends_shutdown_and_keeps_fallback_timer(tmp_path,catalog_path,qt_app):
    controller=DesktopController(ProfileStore(tmp_path/'tasks.yml',catalog_path))
    script="import sys,json;sys.stdin.readline();print(json.dumps({'type':'state','state':{'browser_retained':True}}),flush=True);cmd=json.loads(sys.stdin.readline());print(json.dumps({'type':'state','state':{'browser_retained':True}}),flush=True);sys.exit(0 if cmd['command']=='shutdown' else 4)"
    controller.process.start(sys.executable,['-c',script])
    assert controller.process.waitForStarted(2000)
    controller.shutdown()
    deadline=time.monotonic()+3
    while not controller.last_state.get('browser_retained') and time.monotonic()<deadline:QTest.qWait(25)
    assert controller.last_state.get('browser_retained')
    controller.shutdown(close_browser=True)
    assert controller.terminate_timer.isActive()
    assert controller.process.waitForFinished(2000)
    assert controller.process.exitCode()==0


def test_show_grants_only_worker_foreground_permission_before_ipc(monkeypatch):
    from types import SimpleNamespace
    from app.desktop import process as module
    calls=[]
    controller=SimpleNamespace(active=True, process=SimpleNamespace(
        processId=lambda:3248, write=lambda payload:calls.append(('write',payload))))
    monkeypatch.setattr(module,'allow_worker_foreground',lambda pid:calls.append(('grant',pid)))
    DesktopController.send(controller,{'command':'show_browser'})
    assert calls[0]==('grant',3248)
    assert calls[1][0]=='write'
    calls.clear()
    DesktopController.send(controller,{'command':'pause'})
    assert [kind for kind,_ in calls]==['write']
    calls.clear();controller.active=False
    DesktopController.send(controller,{'command':'show_browser'})
    assert calls==[]


def test_worker_crash_stderr_is_reported_once(tmp_path, catalog_path, qt_app, monkeypatch):
    from app.desktop import startup_report as reports
    monkeypatch.setattr(reports, 'runtime_root', lambda: tmp_path)
    controller = DesktopController(ProfileStore(tmp_path/'tasks.yml', catalog_path))
    errors = []; controller.failed.connect(errors.append)
    controller.process.start(sys.executable, ['-c', "import sys;sys.stderr.write('ImportError: browser component missing');sys.exit(3)"])
    assert controller.process.waitForFinished(5000)
    assert len(errors) == 1
    report_id = reports.report_id_from_message(errors[0])
    report = reports.read_startup_report(report_id)
    assert report['phase'] == 'worker_exit'
    assert 'ImportError' in report['message']
    assert 'browser component missing' not in report['message']
