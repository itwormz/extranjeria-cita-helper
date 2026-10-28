"""Diagnostic failures must not replace the actual test failure."""
import asyncio
from types import SimpleNamespace
from unittest.mock import AsyncMock
import pytest
from tests.browser_support import save_failure_trace


def test_browser_trace_failure_keeps_original_exception(tmp_path,monkeypatch):
    monkeypatch.chdir(tmp_path)
    context=SimpleNamespace(tracing=SimpleNamespace(stop=AsyncMock(side_effect=OSError('unwritable'))))
    async def scenario():
        try:
            raise AssertionError('original form failure')
        except AssertionError:
            await save_failure_trace(context)
            raise
    with pytest.raises(AssertionError,match='original form failure'):
        asyncio.run(scenario())


def test_browser_trace_name_is_safe_for_parameterized_tests(tmp_path,monkeypatch):
    monkeypatch.chdir(tmp_path)
    monkeypatch.setenv('PYTEST_CURRENT_TEST','tests/test_form.py::test_form[DNI:POST/slow] (call)')
    context=SimpleNamespace(tracing=SimpleNamespace(stop=AsyncMock()))
    asyncio.run(save_failure_trace(context))
    from pathlib import Path
    artifact=Path(context.tracing.stop.await_args.kwargs['path'])
    assert artifact.parent==Path('.local/builds/test-artifacts')
    assert artifact.suffix=='.zip'
    assert ':' not in artifact.name and '[' not in artifact.name
