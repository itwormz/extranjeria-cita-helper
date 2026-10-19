from pathlib import Path
import sys

from app.desktop.paths import runtime_root, resource_root, worker_arguments, APP_NAME


def test_frozen_worker_uses_background_helper(monkeypatch,tmp_path):
    monkeypatch.setattr(sys,'platform','darwin')
    from app.desktop.paths import worker_executable
    executable=tmp_path/'Main.app'/'Contents'/'MacOS'/APP_NAME
    monkeypatch.setattr(sys,'frozen',True,raising=False)
    monkeypatch.setattr(sys,'executable',str(executable))
    assert worker_executable()==executable.parent.parent/'Helpers'/'Cita Worker.app'/'Contents'/'MacOS'/APP_NAME


def test_frozen_paths_separate_user_data_from_bundle(monkeypatch,tmp_path):
    monkeypatch.setattr(sys,'frozen',True,raising=False)
    monkeypatch.setattr(sys,'_MEIPASS',str(tmp_path/'bundle'),raising=False)
    monkeypatch.setenv('CITA_DATA_HOME',str(tmp_path/'private'))
    assert runtime_root()==tmp_path/'private'
    assert resource_root()==tmp_path/'bundle'
    assert worker_arguments('profile')==['worker','--task','profile']


def test_frozen_default_data_location(monkeypatch):
    monkeypatch.setattr(sys,'platform','darwin')
    monkeypatch.setattr(sys,'frozen',True,raising=False)
    monkeypatch.delenv('CITA_DATA_HOME',raising=False)
    assert runtime_root()==Path.home()/'Library'/'Application Support'/APP_NAME


def test_experimental_bundle_has_separate_data(monkeypatch,tmp_path):
    monkeypatch.setattr(sys,'platform','darwin')
    import json
    monkeypatch.setattr(sys,'frozen',True,raising=False)
    monkeypatch.setattr(sys,'_MEIPASS',str(tmp_path),raising=False)
    monkeypatch.delenv('CITA_DATA_HOME',raising=False)
    (tmp_path/'app-variant.json').write_text(json.dumps({'name':APP_NAME+' Experimental'}))
    assert runtime_root()==Path.home()/'Library'/'Application Support'/(APP_NAME+' Experimental')


def test_source_worker_uses_python_module(monkeypatch):
    monkeypatch.delattr(sys,'frozen',raising=False)
    assert worker_arguments('profile')==['-m','app.main','worker','--task','profile']


def test_camoufox_launch_uses_packaged_browser(monkeypatch, tmp_path):
    import asyncio
    from types import SimpleNamespace
    from unittest.mock import AsyncMock
    import camoufox.async_api
    from app import site
    from app.config import Settings
    executable=tmp_path/'Camoufox.app'/'Contents'/'MacOS'/'camoufox'
    metadata=executable.parent.parent/'Resources'/'application.ini'
    metadata.parent.mkdir(parents=True)
    metadata.write_text('[App]\nVersion=156.0.1-beta.34\n')
    launch=AsyncMock(return_value=SimpleNamespace(new_context=AsyncMock(), on=lambda *args: None))
    from app.desktop import certificates
    monkeypatch.setattr(certificates, 'prepare_intermediate_cache', lambda *args: 1)
    monkeypatch.setattr(camoufox.async_api,'AsyncNewBrowser',launch)
    monkeypatch.setattr('camoufox.pkgman.launch_path', lambda: '/tmp/offline-camoufox')
    monkeypatch.setattr(site,'bundled_browser',lambda:executable)
    monkeypatch.setattr(site,'bundled_addons',lambda:[])
    checker=site.IcpPlusChecker(Settings(browser_session='fresh'))
    task=SimpleNamespace(id='offline',schedule=SimpleNamespace(timezone='Europe/Madrid'))
    asyncio.run(checker._launch_camoufox(None,task,headless=True))
    assert launch.await_args.kwargs['executable_path']==str(executable)
    assert launch.await_args.kwargs['ff_version']==156


def test_camoufox_does_not_raise_windows_for_page_scripts(monkeypatch):
    import asyncio
    from types import SimpleNamespace
    from unittest.mock import AsyncMock
    import camoufox.async_api
    from app import site
    from app.config import Settings
    launch = AsyncMock(return_value=SimpleNamespace(new_context=AsyncMock(), on=lambda *args: None))
    from app.desktop import certificates
    monkeypatch.setattr(certificates, 'prepare_intermediate_cache', lambda *args: 1)
    monkeypatch.setattr(camoufox.async_api,'AsyncNewBrowser',launch)
    monkeypatch.setattr('camoufox.pkgman.launch_path', lambda: '/tmp/offline-camoufox')
    asyncio.run(site.IcpPlusChecker(Settings(browser_session='fresh'))._launch_camoufox(None,SimpleNamespace(id='offline',schedule=SimpleNamespace(timezone='Europe/Madrid')),headless=True))
    prefs = launch.await_args.kwargs['firefox_user_prefs']
    assert prefs['dom.disable_window_flip'] is True
    assert prefs['browser.tabs.loadDivertedInBackground'] is True
    assert 'focusmanager.testmode' not in prefs


def test_windows_portable_marker_selects_adjacent_data(monkeypatch,tmp_path):
    import sys
    monkeypatch.setattr(sys,'frozen',True,raising=False)
    monkeypatch.setattr(sys,'platform','win32')
    monkeypatch.setattr(sys,'executable',str(tmp_path/'helper.exe'))
    monkeypatch.delenv('CITA_DATA_HOME',raising=False)
    (tmp_path/'portable.flag').write_text('')
    assert runtime_root()==tmp_path/'data'
    monkeypatch.setenv('CITA_DATA_HOME',str(tmp_path/'override'))
    assert runtime_root()==tmp_path/'override'


def test_macos_worker_finds_browser_after_bundle_move(monkeypatch, tmp_path):
    import shutil
    from app.desktop.paths import bundled_browser
    bundle = tmp_path / "Mounted DMG" / "Helper.app"
    contents = bundle / "Contents"
    helper = contents / "Helpers" / "Cita Worker.app" / "Contents"
    (helper / "MacOS").mkdir(parents=True)
    (contents / "Resources").mkdir()
    (helper / "Resources").symlink_to(Path("..") / ".." / ".." / "Resources", target_is_directory=True)
    browser = contents / "Resources" / "bundled-browser" / "Camoufox.app" / "Contents" / "MacOS" / "camoufox"
    browser.parent.mkdir(parents=True)
    browser.write_text("synthetic executable")
    destination = tmp_path / "Applications" / "Helper.app"
    destination.parent.mkdir()
    shutil.move(bundle, destination)
    monkeypatch.setattr(sys, "frozen", True, raising=False)
    monkeypatch.setattr(sys, "platform", "darwin")
    monkeypatch.setattr(sys, "executable", str(destination / "Contents" / "Helpers" / "Cita Worker.app" / "Contents" / "MacOS" / APP_NAME))
    found = bundled_browser()
    assert found is not None
    assert found.read_text() == "synthetic executable"


def test_packaged_macos_version_uses_resources_without_user_cache(tmp_path, monkeypatch):
    from app.desktop.paths import bundled_browser_major_version
    import camoufox.utils
    executable = tmp_path / 'Camoufox.app' / 'Contents' / 'MacOS' / 'camoufox'
    executable.parent.mkdir(parents=True)
    resources = executable.parent.parent / 'Resources'
    resources.mkdir()
    (resources / 'application.ini').write_text('[App]\nVersion=156.0.1-beta.34\n')
    def no_cache():
        raise AssertionError('Bundled browser must not use user cache')
    monkeypatch.setattr(camoufox.utils, 'installed_verstr', no_cache)
    assert bundled_browser_major_version(executable) == 156


def test_packaged_version_reads_adjacent_metadata_on_other_platforms(tmp_path):
    from app.desktop.paths import bundled_browser_major_version
    (tmp_path / 'application.ini').write_text('[App]\nVersion=156.0.1-beta.34\n')
    assert bundled_browser_major_version(tmp_path / 'camoufox.exe') == 156


def test_packaged_version_missing_metadata_fails_without_cache(tmp_path):
    import pytest
    from app.desktop.paths import bundled_browser_major_version
    with pytest.raises(FileNotFoundError):
        bundled_browser_major_version(tmp_path / 'camoufox-bin')
