import os


async def launch_browser(playwright):
    if os.getenv('CITA_TEST_ENGINE') == 'camoufox':
        from camoufox.async_api import AsyncNewBrowser
        return await AsyncNewBrowser(
            playwright, headless=os.getenv("CITA_TEST_HEADED") != "1", main_world_eval=True,
            i_know_what_im_doing=True,
        )
    return await playwright.chromium.launch(executable_path=os.getenv('CITA_TEST_BROWSER_PATH'))
