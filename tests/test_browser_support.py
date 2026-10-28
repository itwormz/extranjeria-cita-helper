"""Diagnostic failures must not replace the actual test failure."""
import asyncio
from types import SimpleNamespace
from unittest.mock import AsyncMock
import pytest
from tests.browser_support import save_failure_trace, start_failure_trace


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


@pytest.mark.parametrize('failure', [None, OSError('sensitive diagnostic details')])
def test_trace_start_failure_does_not_prevent_scenario(failure, capsys):
    context = SimpleNamespace(tracing=SimpleNamespace(
        start=AsyncMock(side_effect=failure), stop=AsyncMock(),
    ))
    form_check = AsyncMock()

    async def scenario():
        traced = await start_failure_trace(context)
        await form_check()
        return traced

    assert asyncio.run(scenario()) is (failure is None)
    form_check.assert_awaited_once()
    context.tracing.start.assert_awaited_once_with(
        screenshots=False, snapshots=True, sources=False,
    )
    context.tracing.stop.assert_not_awaited()
    error = capsys.readouterr().err
    assert 'sensitive diagnostic details' not in error
    if failure:
        assert 'OSError' in error
    else:
        assert error == ''
