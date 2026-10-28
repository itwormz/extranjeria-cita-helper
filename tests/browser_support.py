import os


async def launch_browser(playwright):
    if os.getenv('CITA_TEST_ENGINE') == 'camoufox':
        from camoufox.async_api import AsyncNewBrowser
        return await AsyncNewBrowser(
            playwright, headless=os.getenv("CITA_TEST_HEADED") != "1", main_world_eval=True,
            i_know_what_im_doing=True,
        )
    return await playwright.chromium.launch(executable_path=os.getenv('CITA_TEST_BROWSER_PATH'))


async def start_failure_trace(context):
    """Start optional synthetic diagnostics without preventing form assertions."""
    import asyncio
    import sys
    try:
        await asyncio.wait_for(context.tracing.start(
            screenshots=False, snapshots=True, sources=False,
        ), timeout=10)
    except Exception as exc:
        print(f'Browser trace start unavailable: {type(exc).__name__}', file=sys.stderr)
        return False
    return True


async def save_failure_trace(context):
    """Keep a bounded synthetic-page trace without masking the original failure."""
    import asyncio
    import hashlib
    from pathlib import Path
    import re
    import sys
    name = os.getenv('PYTEST_CURRENT_TEST', 'browser-failure').split(' (')[0]
    identifier = hashlib.sha256(name.encode()).hexdigest()[:12]
    label = re.sub(r'[^A-Za-z0-9_-]+', '-', name.rsplit('::', 1)[-1])[:60]
    try:
        directory = Path('.local/builds/test-artifacts')
        directory.mkdir(parents=True, exist_ok=True)
        path = directory / f'{label}-{identifier}.zip'
        await asyncio.wait_for(context.tracing.stop(path=str(path)), timeout=10)
        print(f'Browser failure trace: {path}', file=sys.stderr)
    except Exception as exc:
        print(f'Browser failure trace unavailable: {type(exc).__name__}', file=sys.stderr)
