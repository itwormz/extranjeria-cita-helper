import asyncio
import io
import sys
from pathlib import Path
import pytest
from app.desktop import paths
from app.main import build_parser

@pytest.mark.parametrize('system,variable,base', [('win32','LOCALAPPDATA','local'), ('linux','XDG_DATA_HOME','xdg')])
def test_platform_private_data(monkeypatch, tmp_path, system, variable, base):
    monkeypatch.setattr(sys,'frozen',True,raising=False)
    monkeypatch.setattr(sys,'platform',system)
    monkeypatch.delenv('CITA_DATA_HOME',raising=False)
    monkeypatch.setenv(variable,str(tmp_path/base))
    assert paths.runtime_root()==tmp_path/base/paths.APP_NAME

@pytest.mark.parametrize('system,name',[('win32','camoufox.exe'),('linux','camoufox-bin')])
def test_native_packaged_browser_and_worker(monkeypatch,tmp_path,system,name):
    monkeypatch.setattr(sys,'frozen',True,raising=False)
    monkeypatch.setattr(sys,'platform',system)
    monkeypatch.setattr(sys,'_MEIPASS',str(tmp_path),raising=False)
    executable=tmp_path/'helper'
    monkeypatch.setattr(sys,'executable',str(executable))
    browser=tmp_path/'bundled-browser'/name
    browser.parent.mkdir(); browser.touch()
    assert paths.bundled_browser()==browser
    assert paths.worker_executable()==executable

@pytest.mark.parametrize('command',['run','check','test-telegram','capture-flow'])
def test_legacy_commands_removed(command):
    with pytest.raises(SystemExit):build_parser().parse_args([command])
