import asyncio
from types import SimpleNamespace
from unittest.mock import AsyncMock
from app.assist_control import AssistControl
from app import browser_visibility


def test_show_restores_native_browser_and_selects_page(monkeypatch):
    calls=[]
    monkeypatch.setattr(browser_visibility, 'activate_native_browser', lambda pid: calls.append(pid) or True)
    page=SimpleNamespace(bring_to_front=AsyncMock(), context=SimpleNamespace(_cita_browser_pid=123))
    control=AssistControl()
    assert asyncio.run(browser_visibility.show_browser(page, control))
    page.bring_to_front.assert_awaited_once()
    assert calls == [123]
    assert control.snapshot()['timeline'][-1]['event'] == 'browser_shown'


def test_failed_show_is_reported_instead_of_silently_discarded(monkeypatch):
    monkeypatch.setattr(browser_visibility, 'activate_native_browser', lambda pid: False)
    page=SimpleNamespace(bring_to_front=AsyncMock(), context=SimpleNamespace(_cita_browser_pid=None))
    control=AssistControl()
    assert not asyncio.run(browser_visibility.show_browser(page, control))
    assert control.snapshot()['timeline'][-1]['event'] == 'browser_show_failed'


def test_process_selection_ignores_other_sessions_and_content_children():
    rows=[(10,1,'worker'),(20,10,'node'),(30,20,'/browser/camoufox'),
          (31,30,'/browser/camoufox'),(40,1,'/browser/camoufox')]
    assert browser_visibility.owned_browser_pid(rows,10)==30
    assert browser_visibility.owned_browser_pid(rows,99) is None


def test_windows_launcher_and_real_window_process_are_both_owned():
    rows=[(10,1,'worker'), (20,10,'node.exe'), (30,20,'camoufox.exe'),
          (31,30,'camoufox.exe'), (32,31,'camoufox.exe'), (40,1,'camoufox.exe')]
    assert browser_visibility.owned_browser_pids(rows,10) == [30,31,32]


def test_windows_activation_receives_launcher_and_window_owner(monkeypatch):
    rows=[(20,10,'node.exe'),(30,20,'camoufox.exe'),(31,30,'camoufox.exe'),(40,1,'camoufox.exe')]
    monkeypatch.setattr(browser_visibility.sys,'platform','win32')
    monkeypatch.setattr(browser_visibility.os,'getpid',lambda:10)
    monkeypatch.setattr(browser_visibility,'_processes',lambda:rows)
    calls=[]
    monkeypatch.setattr(browser_visibility,'_activate_windows',lambda pids: calls.append(pids) or True)
    assert browser_visibility.activate_native_browser()
    assert calls==[[30,31]]
