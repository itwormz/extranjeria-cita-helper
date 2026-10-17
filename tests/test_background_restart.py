import asyncio
from unittest.mock import AsyncMock
import pytest
from types import SimpleNamespace
from app.assist_control import AssistControl
from app.config import Settings
from app.assist_runtime import AssistRestartRequested


@pytest.mark.parametrize('reopen_browser, expected_launches', [(False,1),(True,2)])
def test_automatic_flow_restart_reuses_existing_window(monkeypatch, tmp_path, reopen_browser, expected_launches):
    from app import assist_runtime, assist, site
    import playwright.async_api
    launches, fronts, runs = [], [], []
    async def front(): fronts.append(True)
    page = SimpleNamespace(set_default_timeout=lambda *_:None, is_closed=lambda:False, bring_to_front=front)
    class Checker:
        def __init__(self, settings): self.settings = settings
        def _debug_directory(self,*_): return tmp_path
        async def launch_context(self,*_,**kw): launches.append(True); return SimpleNamespace()
        async def page_for_context(self,*_): return page
        async def _open_entry(self,*_): pass
        async def close_context(self,*_): pass
    class Driver:
        async def start(self): return self
        async def stop(self): pass
    class Runner:
        def __init__(self,*_): pass
        async def wait_unpaused(self): pass
        async def run(self,**_):
            runs.append(True)
            if len(runs)==1:
                raise AssistRestartRequested('timeout', clear_profile=False, delay_seconds=0, reopen_browser=reopen_browser)
            return 0
    monkeypatch.setattr(playwright.async_api,'async_playwright',Driver)
    monkeypatch.setattr(site,'IcpPlusChecker',Checker)
    monkeypatch.setattr(assist,'_install_lifecycle_logging',lambda *_:None)
    monkeypatch.setattr(assist_runtime,'AssistRunner',Runner)
    monkeypatch.setattr(assist_runtime,'wait_restart',AsyncMock(return_value=True))
    control = AssistControl(); control.set_diagnostics(False)
    assert asyncio.run(assist_runtime.run_assist(SimpleNamespace(id='test'),Settings(),control=control))==0
    assert len(launches)==expected_launches and len(runs)==2 and fronts==[]
