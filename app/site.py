"""Playwright implementation of the ICP Plus browser flow."""

from __future__ import annotations

import asyncio
import json
import logging
import os
import platform
import random
import re
import shutil
import time
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit

from .config import DEFAULT_START_URL, Settings, TaskConfig
from .logging_utils import log_event

log = logging.getLogger(__name__)

from .desktop.paths import runtime_root, bundled_browser, bundled_addons
PROJECT_ROOT = runtime_root()
_SUPPORT_ID_RE = re.compile(r"support\s+id\s+is\s*:?\s*<?\s*(\d{6,})", re.IGNORECASE)


@dataclass
class SiteCheckError(RuntimeError):
    code: str
    retryable: bool = False
    state: str = "BLOCKED_ERROR"
    detail: str = ""
    support_id: str = ""

    def __str__(self) -> str:
        return self.code


def _challenge_code(visible_text: str) -> str | None:
    normalized = " ".join(visible_text.lower().split())
    if "captcha" in normalized or "código de la imagen" in normalized:
        return "CAPTCHA_DETECTED"
    if any(
        marker in normalized
        for marker in (
            "access denied",
            "request rejected",
            "requested url was rejected",
            "forbidden",
            "too many requests",
            "demasiadas solicitudes",
            "intrusion prevention triggered",
            "attempt to access the internet resource is blocked",
        )
    ):
        return "HTTP_BLOCKED"
    return None


def _support_id(visible_text: str) -> str | None:
    match = _SUPPORT_ID_RE.search(visible_text)
    return match.group(1) if match else None


def _temporary_site_error_code(visible_text: str) -> str | None:
    normalized = " ".join(visible_text.lower().split())
    markers = (
        "se ha producido un error en el sistema",
        "por favor inténtelo de nuevo",
        "por favor intentelo de nuevo",
    )
    clave_error = "se ha producido un error" in normalized and "reinténtelo de nuevo más tarde" in normalized
    if clave_error or any(marker in normalized for marker in markers):
        return "SITE_TEMPORARY_ERROR"
    return None


def _http_500_page(visible_text: str) -> bool:
    normalized = " ".join(visible_text.lower().split())
    return any(
        marker in normalized
        for marker in (
            "500 internal server error",
            "http error 500",
            "http 500",
            "error 500",
            "status code 500",
            "error interno del servidor",
            "internal server error",
            "the server encountered an internal error",
            # 502/503/504 gateway pages (nginx/Apache) belong to the same
            # outage class: retry with the server-error pauses instead of
            # dying with AUTH_REQUIRED.
            "bad gateway",
            "http error 502",
            "http 502",
            "error 502",
            "status code 502",
            "service unavailable",
            "servicio no disponible",
            "http error 503",
            "http 503",
            "error 503",
            "status code 503",
            "gateway time-out",
            "gateway timeout",
            "http error 504",
            "http 504",
            "error 504",
            "status code 504",
            "error de puerta de enlace",
        )
    )


SESSION_EXPIRED_MARKERS = (
    "sesión caducada",
    "sesion caducada",
    "sesión ha caducado",
    "sesion ha caducado",
)


def _session_expired_code(visible_text: str) -> str | None:
    normalized = " ".join(visible_text.lower().split())
    if any(marker in normalized for marker in SESSION_EXPIRED_MARKERS):
        return "SESSION_EXPIRED"
    return None


def _certificate_service_error(text: str) -> bool:
    normalized=' '.join(text.casefold().split())
    return ('dssafirmaverifycertificate' in normalized
            or ('error al validar el certificado' in normalized and ('saml' in normalized or '@firma' in normalized)))


def _dead_page_kind(page_text: str) -> str | None:
    """Name the dead page we landed on: expired session or a server 500."""
    if _session_expired_code(page_text):
        return "session_expired"
    if ' '.join(page_text.casefold().split()) in {'problema al cargar la página','problem loading page','server not found','la conexión ha caducado'}:
        return 'server_500'
    if _certificate_service_error(page_text) or _http_500_page(page_text) or _temporary_site_error_code(page_text):
        return "server_500"
    return None


def _flow_endpoint(url: str) -> str:
    return urlsplit(url).path.rstrip("/").rsplit("/", 1)[-1].split(";", 1)[0]


def _is_icp_page(url: str) -> bool:
    return urlsplit(url).hostname == "icp.administracionelectronica.gob.es"


def _safe_page_url(url: str) -> str:
    parts = urlsplit(url)
    return urlunsplit((parts.scheme, parts.netloc, parts.path, "", ""))


def _configure_browser_path(project_root: Path = PROJECT_ROOT) -> None:
    """Use the project-local Playwright download when it exists."""
    local_path = project_root / ".playwright"
    if "PLAYWRIGHT_BROWSERS_PATH" not in os.environ and local_path.is_dir():
        os.environ["PLAYWRIGHT_BROWSERS_PATH"] = str(local_path)


def _browser_launch_error_code(exc: Exception) -> str:
    message = str(exc).lower()
    missing_markers = (
        "executable doesn't exist",
        "executable does not exist",
        "playwright install",
        "browser was not found",
    )
    if any(marker in message for marker in missing_markers):
        return "BROWSER_NOT_INSTALLED"
    return "BROWSER_LAUNCH_FAILED"


def _regular_chrome_user_agent(browser_version: str, system: str | None = None) -> str:
    """Build Chrome's reduced desktop UA without the headless-only product token."""
    major = browser_version.split(".", 1)[0]
    if not major.isdigit():
        raise ValueError("Invalid Chromium version")
    platform_name = system or platform.system()
    platform_token = {
        "Darwin": "Macintosh; Intel Mac OS X 10_15_7",
        "Windows": "Windows NT 10.0; Win64; x64",
    }.get(platform_name, "X11; Linux x86_64")
    return (
        f"Mozilla/5.0 ({platform_token}) AppleWebKit/537.36 "
        f"(KHTML, like Gecko) Chrome/{major}.0.0.0 Safari/537.36"
    )


def _browser_profile_location(settings: Settings, task_id: str) -> Path:
    root = Path(settings.browser_profile_directory)
    if not root.is_absolute():
        root = PROJECT_ROOT / root
    return root / settings.browser_engine / task_id


def browser_profile_path(settings: Settings, task_id: str) -> Path:
    profile = _browser_profile_location(settings, task_id)
    root = profile.parent.parent
    profile.mkdir(parents=True, exist_ok=True, mode=0o700)
    try:
        root.chmod(0o700)
        profile.parent.chmod(0o700)
        profile.chmod(0o700)
    except OSError:
        pass
    return profile


def reset_browser_profile(settings: Settings, task_id: str) -> bool:
    """Delete only this task's profile; the next browser launch recreates it."""
    from contextlib import ExitStack
    from .locking import lock_handle, unlock_handle

    profile = _browser_profile_location(settings, task_id)
    if profile.is_symlink():
        raise SiteCheckError("BROWSER_PROFILE_RESET_FAILED", detail="profile.symlink")
    if not profile.exists():
        return False
    with ExitStack() as locks:
        for name in ("parent.lock", "lock", "SingletonLock"):
            lock = profile / name
            if not lock.is_symlink():
                continue
            match = re.search(r"[+:\-](\d+)$", os.readlink(lock))
            if not match:
                raise SiteCheckError("BROWSER_PROFILE_IN_USE", detail="profile.lock")
            if os.name == "nt":
                raise SiteCheckError("BROWSER_PROFILE_IN_USE", detail="profile.lock")
            try:
                os.kill(int(match.group(1)), 0)
            except ProcessLookupError:
                continue
            except PermissionError:
                pass
            raise SiteCheckError("BROWSER_PROFILE_IN_USE", detail="profile.lock")
        # Firefox also holds a POSIX lock on .parentlock on macOS/Linux.
        candidates = ('.parentlock', 'parent.lock') if os.name == 'nt' else ('.parentlock',)
        for name in candidates:
            lock = profile / name
            if lock.is_file() and not lock.is_symlink():
                try:
                    handle = locks.enter_context(lock.open('r+b'))
                    lock_handle(handle, blocking=False, posix_record=True)
                    locks.callback(unlock_handle, handle, posix_record=True)
                except OSError as exc:
                    raise SiteCheckError('BROWSER_PROFILE_IN_USE', detail='profile.lock') from exc
        # Windows cannot delete an open lock file. Our worker ownership lock
        # stays held by the caller throughout the reset.
        if os.name == 'nt':
            locks.close()
        shutil.rmtree(profile)
    return True


class IcpPlusChecker:
    def __init__(self, settings: Settings):
        self.settings = settings

    async def _activate_control(self, locator, *, timeout=None, force=False, keyboard=False):
        options = {} if timeout is None else {"timeout": timeout}
        if force:
            options["force"] = True
        if getattr(self, "_assist_guard", None) is None:
            await locator.click(**options)
            return
        from .desktop_session import screen_locked
        keyboard = (keyboard or screen_locked()) and await locator.evaluate("""el =>
            el.matches('button, a[href], input[type="button"], input[type="submit"]')
        """)
        # Keyboard activation avoids the compositor acknowledgement used by
        # mouse dispatch, which can stall while macOS displays its lock screen.
        # Keep the normal pointer path while the desktop is available.
        if not await locator.is_visible() or not await locator.is_enabled():
            raise SiteCheckError("FORM_NOT_READY", True, detail="interaction.control")
        # A dispatched click can outlive its short actionability timeout.
        # Observe navigation before clicking, so a late/same-URL response is
        # awaited without sending the form a second time.
        from contextlib import suppress
        from playwright.async_api import TimeoutError as PlaywrightTimeoutError
        handle = await locator.element_handle(timeout=timeout)
        if handle is None:
            raise SiteCheckError("FORM_NOT_READY", True, detail="interaction.control")
        source_frame = await handle.owner_frame()
        await handle.dispose()
        page = locator.page
        budget = self.settings.navigation_timeout_seconds * 1000
        started = time.monotonic()
        navigation = asyncio.create_task(page.wait_for_event(
            "framenavigated", predicate=lambda frame: frame == source_frame or frame == page.main_frame,
            timeout=budget))
        await asyncio.sleep(0)
        try:
            try:
                if keyboard:
                    await locator.press("Enter", **({"timeout":timeout} if timeout is not None else {}))
                else:
                    await locator.click(**options)
            except PlaywrightTimeoutError as exc:
                message = str(exc)
                sent_click = ("click action done" in message
                              and "waiting for scheduled navigations to finish" in message)
                if not keyboard and not sent_click:
                    raise
                # A keyboard dispatch can time out after sending the form,
                # without the mouse-specific acknowledgement text. Observe
                # the already registered navigation; never send another key.
                try:
                    frame = await navigation
                except PlaywrightTimeoutError:
                    raise exc
                remaining = max(1, budget - int((time.monotonic() - started) * 1000))
                await frame.wait_for_load_state("domcontentloaded", timeout=remaining)
        finally:
            navigation.cancel()
            with suppress(asyncio.CancelledError, Exception):
                await navigation

    async def _settle(self, page) -> None:
        try:
            await page.wait_for_load_state("domcontentloaded", timeout=self.settings.navigation_timeout_seconds * 1000)
        except Exception:
            pass
        await page.wait_for_timeout(500)

    async def _pace(self, page) -> None:
        delay_min, delay_max = self.settings.browser_action_delay_seconds
        if delay_max > 0:
            await page.wait_for_timeout(int(random.uniform(delay_min, delay_max) * 1000))
        guard = getattr(self, "_assist_guard", None)
        if guard is not None:
            await guard(page)

    async def launch_context(self, playwright, task: TaskConfig, *, force_headed: bool = False):
        headless = False if force_headed else self.settings.browser_mode == "headless"
        if self.settings.browser_engine == "camoufox":
            return await self._launch_camoufox(playwright, task, headless=headless)
        launch_options = {"headless": headless}
        preset = getattr(self.settings, "browser_preset", "localized")
        context_options = {}
        if preset == "localized":
            context_options = {
                "locale": "es-ES",
                "timezone_id": task.schedule.timezone,
                "viewport": {"width": 1365, "height": 900},
            }
        browser_type = getattr(playwright, self.settings.browser_engine)
        if self.settings.browser_engine == "chromium" and preset != "vanilla":
            launch_options["channel"] = "chromium"
        executable_path = getattr(self.settings, "browser_executable_path", "")
        if executable_path:
            launch_options.pop("channel", None)
            launch_options["executable_path"] = executable_path
        if headless and preset == "localized":
            descriptor = "Desktop Chrome" if self.settings.browser_engine == "chromium" else "Desktop Safari"
            context_options["user_agent"] = playwright.devices[descriptor]["user_agent"]

        profile: Path | None = None
        if self.settings.browser_session == "persistent":
            profile = browser_profile_path(self.settings, task.id)
            context = await browser_type.launch_persistent_context(
                user_data_dir=str(profile),
                **launch_options,
                **context_options,
            )
        else:
            browser = await browser_type.launch(**launch_options)
            try:
                context = await browser.new_context(**context_options)
            except Exception:
                await browser.close()
                raise
        log_event(
            log,
            logging.INFO,
            "browser_session_opened",
            task_id=task.id,
            engine=self.settings.browser_engine,
            preset=preset,
            executable=Path(executable_path).name if executable_path else "bundled",
            mode="headless" if headless else "headed",
            session=self.settings.browser_session,
            profile_directory=profile,
        )
        return context

    async def _launch_camoufox(self, playwright, task: TaskConfig, *, headless: bool):
        from .desktop.certificates import prepare_intermediate_cache
        from camoufox.async_api import AsyncNewBrowser

        options: dict = {
            "headless": headless,
            "os": {"Darwin": "macos", "Windows": "windows"}.get(platform.system(), "linux"),
            "locale": "es-ES",
            "humanize": 1.5,
            "config": {"timezone": task.schedule.timezone},
            "i_know_what_im_doing": True,
            "enable_cache": True,
            # Dispatch form events in the real DOM without the sandbox's
            # automatic focus side effects.
            "main_world_eval": True,
            "firefox_user_prefs": {
                "security.enterprise_roots.enabled": True,
                "security.osclientcerts.autoload": True,
                "dom.disable_window_flip": True,
                "browser.tabs.loadDivertedInBackground": True,
            },
        }
        packaged_browser = bundled_browser()
        if packaged_browser:
            from .desktop.paths import bundled_browser_major_version
            options['executable_path'] = str(packaged_browser)
            options['ff_version'] = bundled_browser_major_version(packaged_browser)
            packaged_addons = bundled_addons()
            if packaged_addons:
                from camoufox.addons import DefaultAddons
                options['addons'] = [str(p) for p in packaged_addons]
                options['exclude_addons'] = list(DefaultAddons)
        from tempfile import TemporaryDirectory
        from camoufox.pkgman import launch_path
        temporary_profile = None
        if self.settings.browser_session == "persistent":
            profile = browser_profile_path(self.settings, task.id)
        else:
            temporary_profile = TemporaryDirectory(prefix='cita-browser-')
            profile = Path(temporary_profile.name)
        executable = packaged_browser or Path(launch_path())
        try:
            try:
                imported = await asyncio.to_thread(
                    prepare_intermediate_cache, profile, executable,
                    runtime_root() / 'data' / 'certificate-cache',
                )
                log_event(log, logging.INFO, 'tls_intermediates_ready', count=imported)
            except (TimeoutError,ConnectionError,OSError) as exc:
                # Optional intermediate discovery must not prevent the browser
                # opening during a site outage. Browser HTTPS validation stays strict.
                log_event(log,logging.WARNING,'tls_preparation_deferred',error_type=type(exc).__name__)

            options.update(persistent_context=True, user_data_dir=str(profile), ignore_https_errors=False)
            context = await AsyncNewBrowser(playwright, **options)
        except Exception:
            if temporary_profile:
                temporary_profile.cleanup()
            raise
        if temporary_profile:
            context.on('close', lambda _: temporary_profile.cleanup())
        log_event(
            log,
            logging.INFO,
            "browser_session_opened",
            task_id=task.id,
            engine="camoufox",
            preset="camoufox",
            executable="camoufox",
            mode="headless" if headless else "headed",
            session=self.settings.browser_session,
            profile_directory=profile,
        )
        return context

    async def close_context(self, context) -> None:
        # Cleanup must never replace the flow result. In headed mode a user can
        # close the window after seeing an F5 page; Playwright then raises
        # TargetClosedError from context.close(). Preserve the original
        # HTTP_BLOCKED (and its Support ID) instead of turning it into an
        # INTERNAL_ERROR.
        try:
            browser = context.browser
        except Exception:
            browser = None
        try:
            await context.close()
        except Exception:
            log_event(log, logging.DEBUG, "browser_context_already_closed")
        if browser is not None:
            try:
                if browser.is_connected():
                    await browser.close()
            except Exception:
                log_event(log, logging.DEBUG, "browser_process_already_closed")



    async def page_for_context(self, context):
        pages = context.pages
        page = pages[0] if pages else await context.new_page()
        for extra in pages[1:]:
            try:
                await extra.close()
            except Exception:
                pass
        return page

    async def _accept_cookies(self, page) -> bool:
        button = page.locator("#cookie-law-info-bar #cookie_action_close_header").first
        try:
            if not await button.count() or not await button.is_visible():
                return False
            await self._pace(page)
            await self._activate_control(button, timeout=2000)
            await button.wait_for(state="hidden", timeout=2000)
            return True
        except SiteCheckError:
            raise
        except Exception:
            return False


    async def _reload_on_server_error(
        self,
        page,
        error_field: str,
        task_id: str,
        run_id: str,
        response_status: int | None = None,
    ) -> bool:
        visible_text = await page.locator("body").inner_text()
        page_text = f"{await page.title()} {visible_text}"
        if _challenge_code(page_text):
            return False
        server_error = (
            response_status is not None and 500 <= response_status <= 599
        ) or _http_500_page(page_text) or _temporary_site_error_code(page_text)
        if not server_error:
            return False

        last_status = response_status
        for reload_attempt in range(1, self.settings.page_reload_attempts + 1):
            log_event(
                log,
                logging.WARNING,
                "site_reload_started",
                task_id=task_id,
                run_id=run_id,
                error_field=error_field,
                reload_attempt=reload_attempt,
                http_status=last_status,
            )
            await self._pace(page)
            response = await page.reload(wait_until="domcontentloaded")
            await self._settle(page)
            last_status = response.status if response else None
            visible_text = await page.locator("body").inner_text()
            page_text = f"{await page.title()} {visible_text}"
            if _challenge_code(page_text):
                await self._assert_no_challenge(page, error_field)
            still_failing = (
                last_status is not None and 500 <= last_status <= 599
            ) or _http_500_page(page_text) or _temporary_site_error_code(page_text)
            if not still_failing:
                log_event(
                    log,
                    logging.INFO,
                    "site_reload_succeeded",
                    task_id=task_id,
                    run_id=run_id,
                    error_field=error_field,
                    reload_attempt=reload_attempt,
                    http_status=last_status,
                )
                return True

        log_event(
            log,
            logging.WARNING,
            "site_reload_failed",
            task_id=task_id,
            run_id=run_id,
            error_field=error_field,
            reload_attempts=self.settings.page_reload_attempts,
            http_status=last_status,
        )
        raise SiteCheckError("SITE_TEMPORARY_ERROR", True, "ACTIVE", error_field)

    async def _assert_no_challenge(self, page, error_field: str = "browser.page") -> None:
        captcha_selectors = (
            "#captcha",
            "#captchaAnswer",
            "#txtCodigoVerificacion",
            ".g-recaptcha",
            "iframe[src*='captcha' i]",
        )
        for selector in captcha_selectors:
            locator = page.locator(selector)
            try:
                if await locator.count() and await locator.first.is_visible():
                    raise SiteCheckError(
                        code="CAPTCHA_DETECTED",
                        retryable=False,
                        state="COOLDOWN",
                        detail=error_field,
                    )
            except SiteCheckError:
                raise
            except Exception:
                continue
        visible_text = await page.locator("body").inner_text()
        page_text = f"{await page.title()} {visible_text}"
        code = _challenge_code(page_text)
        if code:
            raise SiteCheckError(
                code=code,
                retryable=False,
                state="COOLDOWN",
                detail=error_field,
                support_id=_support_id(page_text) or "",
            )
        code = _temporary_site_error_code(page_text)
        if code:
            raise SiteCheckError(code=code, retryable=True, state="ACTIVE", detail=error_field)

    async def _select_without_focus(self, page, selector, *, label=None, value=None) -> None:
        """Select an option and notify the form without raising a native popup.

        Playwright's select_option focuses the select element. In a headed
        background Camoufox window that can also raise the browser on macOS.
        Preserve input/change behavior while leaving keyboard focus alone.
        """
        guard = getattr(self, '_assist_guard', None)
        if guard is not None:
            await guard()
        locator = page.locator(selector).first
        if not await locator.is_visible() or not await locator.is_enabled():
            raise SiteCheckError('FORM_NOT_READY', True, 'ACTIVE', 'select.option')
        expression = '''choice => {
            const select = document.querySelector(choice.selector);
            if (!select || select.tagName !== 'SELECT' || select.disabled) return false;
            const option = Array.from(select.options).find(option =>
                (choice.value !== null ? option.value === choice.value : option.label === choice.label)
                && !option.disabled && !(option.parentElement.tagName === 'OPTGROUP' && option.parentElement.disabled));
            if (!option) return false;
            select.value = option.value;
            select.dispatchEvent(new Event('input', {bubbles: true, composed: true}));
            select.dispatchEvent(new Event('change', {bubbles: true}));
            return true;
        }'''
        # Camoufox's isolated world focuses the input-event target. The main
        # world dispatches the same form events without that focus side effect.
        if self.settings.browser_engine == 'camoufox':
            expression = 'mw:' + expression
        selected = await page.evaluate(expression, {'selector': selector, 'label': label, 'value': value})
        if not selected:
            raise SiteCheckError('FORM_INTERACTION_FAILED', True, 'ACTIVE', 'select.option')

    async def _select_label(self, page, selector: str, label: str, error_field: str) -> None:
        deadline = time.monotonic() + self.settings.navigation_timeout_seconds
        saw_visible = False
        saw_target = False
        last_error: Exception | None = None
        option_labels: list[str] = []
        while time.monotonic() < deadline:
            try:
                locator = page.locator(selector)
                if await locator.is_visible():
                    saw_visible = True
                    option_labels = await locator.locator("option").all_inner_texts()
                    if label in option_labels:
                        saw_target = True
                        await self._pace(page)
                        await self._select_without_focus(page, selector, label=label)
                        return
            except Exception as exc:
                # F5 and onchange handlers may replace the document while a
                # locator is being resolved. Re-resolve it until the deadline.
                last_error = exc
            await page.wait_for_timeout(250)
        await self._assert_no_challenge(page, error_field)
        if saw_target:
            raise SiteCheckError("FORM_INTERACTION_FAILED", True, "ACTIVE", error_field) from last_error
        if saw_visible and len(option_labels) > 1:
            raise SiteCheckError("CATALOG_VALUE_NOT_FOUND", False, "BLOCKED_ERROR", error_field)
        raise SiteCheckError("FORM_NOT_READY", True, "ACTIVE", error_field) from last_error

    async def _type_like_user(self, page, selector: str, value: str) -> None:
        locator = page.locator(selector)
        await self._pace(page)
        from .desktop_session import screen_locked
        if not screen_locked():
            await locator.click()
        await locator.fill("")
        try:
            await locator.press_sequentially(value, delay=round(random.uniform(*self.settings.assist_timings.typing_ms)))
        except Exception:
            # `fill` is the supported fallback for characters that cannot be
            # represented as individual keyboard events on the current OS.
            await locator.fill(value)

    async def _raise_if_dead_page(self, page, error_field: str) -> None:
        """A dead server page is an outage to retry, not a missing sign-in.

        Without this the Cl@ve round-trip treats "502 Bad Gateway" as a silent
        auth failure and dies with AUTH_REQUIRED instead of pausing and
        re-walking the flow.
        """
        try:
            from .page_inspection import dead_document
            found = await dead_document(page)
        except Exception:  # noqa: BLE001 - mid-navigation reads are expected
            return
        dead_kind = found[1] if found else None
        if dead_kind == "session_expired":
            raise SiteCheckError("SESSION_EXPIRED", True, "ACTIVE", error_field)
        if dead_kind is not None:
            raise SiteCheckError("SITE_TEMPORARY_ERROR", True, "ACTIVE", error_field)

    async def _clave_requires_attention(self, page) -> bool:
        """Distinguish interactive authentication from an empty redirect page."""
        parts=urlsplit(page.url)
        host=parts.hostname or ''
        qr_page=host=='www2.agenciatributaria.gob.es' and parts.path.rstrip('/').endswith('/ObtenerClaveMovilQR')
        if not qr_page and host!='clave.gob.es' and not host.endswith('.clave.gob.es'):
            return False
        inputs=page.locator('input:not([type="hidden"]):not([type="submit"]):not([type="button"]):not([type="checkbox"]):not([type="radio"])')
        for index in range(await inputs.count()):
            field=inputs.nth(index)
            if await field.is_visible() and await field.is_editable():return True
        if qr_page:
            qr=page.locator('img[src*="qr" i], img[id*="qr" i], img[src^="data:image"], canvas, svg[aria-label*="qr" i]')
            for index in range(await qr.count()):
                if await qr.nth(index).is_visible():return True
        return False

    async def _select_clave_method(self, page, task) -> bool:
        """Choose only an unambiguous, visible method on the official portal."""
        if urlsplit(page.url).hostname not in {"pasarela.clave.gob.es", "pasarela-ident.clave.gob.es"}:
            return False
        method=getattr(task,"clave_method","movil")
        pattern=re.compile(r"cl@ve\s+m[oó]vil" if method=="movil" else r"cl@ve\s+permanente",re.I)
        candidates=page.locator('button, a, input[type="button"], input[type="submit"]')
        matches=[]
        for index in range(await candidates.count()):
            control=candidates.nth(index)
            if not await control.is_visible():continue
            text=" ".join(filter(None,[await control.inner_text(),await control.get_attribute("value"),await control.get_attribute("aria-label"),await control.get_attribute("title")]))
            if pattern.search(text):matches.append(control)
        if len(matches)!=1:return False
        await self._activate_control(matches[0])
        return True

    async def _enter_with_clave(self, page, task: TaskConfig, run_id: str) -> None:
        """Follow Cl@ve SSO and stop safely when interactive authentication is needed."""
        await self._pace(page)
        button = page.locator("#btnAccesoClave")
        if not await button.count() or not await button.first.is_visible():
            raise SiteCheckError(
                "SITE_STRUCTURE_CHANGED",
                False,
                "BLOCKED_ERROR",
                "auth.clave.entry",
            )
        await self._activate_control(button.first)

        deadline = time.monotonic() + self.settings.navigation_timeout_seconds
        saw_external_auth = False
        method_selected = False
        portal_ready_since = None
        while time.monotonic() < deadline:
            try:
                await self._raise_if_dead_page(page, "auth.clave")
                current_url = page.url
                parts = urlsplit(current_url)
                path = parts.path.rstrip("/")
                if not _is_icp_page(current_url):
                    saw_external_auth = True
                    if path.endswith("/ObtenerClaveMovilQR"):
                        raise SiteCheckError(
                            "AUTH_REQUIRED",
                            False,
                            "PAUSED_AUTH",
                            "auth.clave",
                        )
                    if not method_selected and await self._select_clave_method(page,task):
                        method_selected=True
                        portal_ready_since=None
                    else:
                        password=page.locator('input[type="password"]')
                        if await password.count() and await password.first.is_visible():
                            raise SiteCheckError("AUTH_REQUIRED",False,"PAUSED_AUTH","auth.clave")
                        if await self._clave_requires_attention(page):
                            if portal_ready_since is None:portal_ready_since=time.monotonic()
                            elif time.monotonic()-portal_ready_since>=2:
                                raise SiteCheckError("AUTH_REQUIRED",False,"PAUSED_AUTH","auth.clave")
                elif _flow_endpoint(current_url) == "acEntrada":
                    copy_button = page.locator("#btnCopiar")
                    if await copy_button.count() and await copy_button.first.is_visible():
                        await self._settle(page)
                        await self._assert_no_challenge(page, "auth.clave.return")
                        log_event(
                            log,
                            logging.INFO,
                            "clave_sso_reused",
                            task_id=task.id,
                            run_id=run_id,
                        )
                        return
                elif _flow_endpoint(current_url) not in {"acInfo", "procesarRespuestaClave"}:
                    await self._assert_no_challenge(page, "auth.clave")
            except SiteCheckError:
                raise
            except Exception:
                # Navigation replaces the execution context several times in
                # the SAML round-trip. Re-resolve locators until it settles.
                pass
            await page.wait_for_timeout(250)

        await self._raise_if_dead_page(page, "auth.clave")
        if saw_external_auth and await self._clave_requires_attention(page):
            raise SiteCheckError("AUTH_REQUIRED", False, "PAUSED_AUTH", "auth.clave")
        await self._assert_no_challenge(page, "auth.clave")
        raise SiteCheckError("FORM_NOT_READY", True, "ACTIVE", "auth.clave")

    async def _fill_applicant_with_clave(self, page, task: TaskConfig, run_id: str) -> None:
        copy_button = page.locator("#btnCopiar")
        if not await copy_button.count() or not await copy_button.first.is_visible():
            raise SiteCheckError(
                "SITE_STRUCTURE_CHANGED",
                False,
                "BLOCKED_ERROR",
                "applicant.copy_from_clave",
            )
        await self._pace(page)
        await self._activate_control(copy_button.first)
        from playwright.async_api import TimeoutError as PlaywrightTimeoutError
        try:
            await page.wait_for_function("""() => ['txtIdCitado','txtDesCitado'].every(id => {
                const field=document.getElementById(id);
                return field && field.value.trim().length>0;
            })""", timeout=5_000)
        except PlaywrightTimeoutError as exc:
            await self._raise_if_dead_page(page, 'applicant.copy_from_clave')
            raise SiteCheckError('FORM_NOT_READY', False, 'BLOCKED_ERROR', 'applicant.copy_from_clave') from exc
        await self._select_label(page, "#txtPaisNac", task.nationality, "applicant.nationality")
        log_event(
            log,
            logging.DEBUG,
            "clave_applicant_data_copied",
            task_id=task.id,
            run_id=run_id,
        )

    async def _activate_certificate_control(self,button,on_attention):
        from .certificate_prompt import certificate_prompt_ids
        from contextlib import suppress
        baseline=await asyncio.to_thread(certificate_prompt_ids) if on_attention else set()
        activation=asyncio.create_task(self._activate_control(button));alerted=False
        async def inspect():
            nonlocal alerted
            if on_attention and not alerted and (await asyncio.to_thread(certificate_prompt_ids))-baseline:
                alerted=True;await on_attention('certificate_approval',repeat=False)
        try:
            while not activation.done():
                await inspect();await asyncio.sleep(.25)
            await activation;await inspect()
        finally:
            if not activation.done():
                activation.cancel()
                with suppress(asyncio.CancelledError):await activation

    async def _enter_with_certificate(self, page, task, run_id, on_attention=None):
        """Remind only after certificate entry is selected, including a blocked click."""
        if on_attention is None:
            return await self._enter_with_certificate_impl(page, task, run_id)
        from contextlib import suppress
        alerted = False
        certificate_started = asyncio.Event()
        async def attention(reason, repeat=False):
            nonlocal alerted
            if not alerted:
                alerted = True
                await on_attention(reason, repeat=False)
        async def reminder():
            await certificate_started.wait()
            await asyncio.sleep(5)
            await attention('certificate_wait')
        timer = asyncio.create_task(reminder())
        try:
            return await self._enter_with_certificate_impl(page, task, run_id, on_attention=attention,
                                                           on_certificate_started=certificate_started.set)
        finally:
            timer.cancel()
            with suppress(asyncio.CancelledError):
                await timer

    async def _enter_with_certificate_impl(
        self, page, task: TaskConfig, run_id: str, on_attention=None, on_certificate_started=None
    ) -> str:
        """Cl@ve entry with certificado electrónico. Returns 'silent' or 'interactive'.

        A live Cl@ve session redirects straight through and never shows the
        certificate button; a fresh one lands on the ServiceProvider portal and
        needs the "Acceso DNIe / Certificado electrónico" button plus a manual
        Keychain approval.
        """
        await self._pace(page)
        button = page.locator("#btnAccesoClave")
        if not await button.count() or not await button.first.is_visible():
            page_text = f"{await page.title()} {await page.locator('body').inner_text()}"
            if _dead_page_kind(page_text) == "session_expired":
                raise SiteCheckError(
                    "SESSION_EXPIRED", True, "ACTIVE", "auth.certificate.entry"
                )
            if _dead_page_kind(page_text) is not None:
                raise SiteCheckError(
                    "SITE_TEMPORARY_ERROR", True, "ACTIVE", "auth.certificate.entry"
                )
            raise SiteCheckError(
                "SITE_STRUCTURE_CHANGED",
                False,
                "BLOCKED_ERROR",
                "auth.certificate.entry",
            )
        await self._activate_control(button.first)

        deadline = time.monotonic() + self.settings.navigation_timeout_seconds
        cert_clicked = False
        saw_portal = False
        attention_raised = False
        while time.monotonic() < deadline:
            try:
                await self._raise_if_dead_page(page, "auth.certificate")
                current_url = page.url
                path = urlsplit(current_url).path.rstrip("/")
                if not _is_icp_page(current_url):
                    saw_portal = True
                    if path.endswith("/ObtenerClaveMovilQR"):
                        raise SiteCheckError(
                            "AUTH_REQUIRED",
                            False,
                            "PAUSED_AUTH",
                            "auth.certificate",
                        )
                    cert_button = page.locator(
                        "button", has_text="Certificado electrónico"
                    )
                    if (
                        not cert_clicked
                        and await cert_button.count()
                        and await cert_button.first.is_visible()
                    ):
                        await self._pace(page)
                        cert_clicked = True
                        if on_certificate_started is not None:
                            on_certificate_started()
                        await self._activate_certificate_control(cert_button.first,on_attention)
                        log_event(log,logging.INFO,'certificate_button_clicked',task_id=task.id,run_id=run_id)
                elif _flow_endpoint(current_url) == "acEntrada":
                    copy_button = page.locator("#btnCopiar")
                    if await copy_button.count() and await copy_button.first.is_visible():
                        await self._settle(page)
                        await self._assert_no_challenge(page, "auth.certificate.return")
                        mode = "interactive" if cert_clicked else "silent"
                        log_event(
                            log,
                            logging.INFO,
                            "certificate_sso_reused" if mode == "silent" else "certificate_sso_completed",
                            task_id=task.id,
                            run_id=run_id,
                        )
                        return mode
                elif _flow_endpoint(current_url) not in {"acInfo", "procesarRespuestaClave"}:
                    await self._assert_no_challenge(page, "auth.certificate")
            except SiteCheckError:
                raise
            except Exception:
                pass
            await page.wait_for_timeout(250)

        await self._raise_if_dead_page(page, "auth.certificate")
        if saw_portal:
            raise SiteCheckError("AUTH_REQUIRED", False, "PAUSED_AUTH", "auth.certificate")
        await self._assert_no_challenge(page, "auth.certificate")
        raise SiteCheckError("FORM_NOT_READY", True, "ACTIVE", "auth.certificate")

    async def _fill_applicant_manual(self, page, task: TaskConfig, run_id: str) -> None:
        """Enter the appointment subject's data without btnCopiar.

        The profile explicitly selects manual entry when the appointment subject
        differs from the authenticated person.
        """
        await self._pace(page)
        nie_radio = page.locator("#rdbTipoDocNie")
        if await nie_radio.count() and await nie_radio.first.is_visible():
            from .desktop_session import screen_locked
            if screen_locked():
                if not await nie_radio.first.is_checked():
                    await nie_radio.first.press("Space")
            else:
                await nie_radio.first.check()
        await self._type_like_user(page, "#txtIdCitado", task.document_number)
        await self._type_like_user(page, "#txtDesCitado", task.full_name)
        await self._select_label(page, "#txtPaisNac", task.nationality, "applicant.nationality")
        log_event(
            log,
            logging.DEBUG,
            "applicant_entered_manually",
            task_id=task.id,
            run_id=run_id,
        )

    async def _fill_contact(self, page, task: TaskConfig, run_id: str) -> bool:
        """Type phone and the e-mail twice; the form rejects pasted values.

        The contact form ("Rellena el siguiente formulario") has stable ids
        (#txtTelefonoCitado, #emailUNO, #emailDOS behind a noPaste guard), so
        they are tried first and heuristics only back them up. Only visible and
        editable fields are used; the ICP form carries hidden leftovers that
        must not swallow keystrokes or block the run.
        """
        phone_fields = await self._usable_fields(page, ("#txtTelefonoCitado",))
        if not phone_fields:
            phone_fields = await self._usable_fields(
                page,
                (
                    "input[type='tel']",
                    "input[id*='tel' i]",
                    "input[name*='tel' i]",
                    "input[id*='movil' i]",
                    "input[name*='movil' i]",
                ),
            )
        email_fields = await self._usable_fields(page, ("#emailUNO", "#emailDOS"))
        if len(email_fields) < 2:
            email_fields = await self._usable_fields(
                page,
                (
                    "input[type='email']",
                    "input[id*='correo' i]",
                    "input[name*='correo' i]",
                    "input[id*='email' i]",
                    "input[name*='email' i]",
                ),
            )
        phone_typed = False
        for locator in phone_fields:
            if await self._type_no_paste(locator, task.contact_phone):
                phone_typed = True
                break
        email_typed = 0
        for locator in email_fields:
            if email_typed >= 2:
                break
            if await self._type_no_paste(locator, task.contact_email):
                email_typed += 1
        filled = phone_typed and email_typed >= 2
        log_event(
            log,
            logging.INFO,
            "contact_autofill",
            task_id=task.id,
            run_id=run_id,
            phone_typed=phone_typed,
            email_fields=email_typed,
        )
        return filled


    async def _usable_fields(self, page, selectors) -> list:
        found = []
        # A CSS selector union returns each DOM element once even if it
        # matches several fallbacks (type=email plus id/name=correo).
        locator = page.locator(", ".join(selectors))
        for index in range(await locator.count()):
            try:
                candidate = locator.nth(index)
                if await candidate.is_visible() and await candidate.is_editable():
                    found.append(candidate)
            except Exception:
                continue
        return found

    async def _type_no_paste(self, locator, value: str) -> bool:
        try:
            guard = getattr(self, "_assist_guard", None)
            if guard is not None:
                await guard()
            from .desktop_session import screen_locked
            if not screen_locked():
                await locator.click(timeout=3_000)
            await locator.fill("", timeout=3_000)
            await locator.press_sequentially(value, delay=round(random.uniform(*self.settings.assist_timings.contact_typing_ms)), timeout=20_000)
            return True
        except Exception:
            return False



    async def _open_entry(self, page, task: TaskConfig, run_id: str) -> None:
        response = await page.goto(DEFAULT_START_URL, wait_until="domcontentloaded")
        if response and response.status == 429:
            raise SiteCheckError('HTTP_RATE_LIMITED', True, 'COOLDOWN', 'entry')
        if response and response.status == 403:
            await self._assert_no_challenge(page, "entry")
            raise SiteCheckError("HTTP_BLOCKED", False, "COOLDOWN", "entry")
        await self._settle(page)
        await self._assert_no_challenge(page, "entry")

        if self.settings.browser_bootstrap_enabled:
            wait_min, wait_max = self.settings.browser_bootstrap_wait_seconds
            log_event(
                log,
                logging.INFO,
                "f5_bootstrap_started",
                task_id=task.id,
                run_id=run_id,
            )
            await page.wait_for_timeout(int(random.uniform(wait_min, wait_max) * 1000))
            try:
                await page.wait_for_load_state("networkidle", timeout=10_000)
            except Exception:
                pass
            guard = getattr(self, "_assist_guard", None)
            if guard is not None:
                await guard(page)
            response = await page.reload(wait_until="domcontentloaded")
            await self._settle(page)
            await page.wait_for_timeout(int(random.uniform(wait_min, wait_max) * 1000))
            try:
                await page.wait_for_load_state("networkidle", timeout=10_000)
            except Exception:
                pass
            log_event(
                log,
                logging.INFO,
                "f5_bootstrap_finished",
                task_id=task.id,
                run_id=run_id,
                http_status=response.status if response else None,
            )

        if response and response.status == 429:
            raise SiteCheckError('HTTP_RATE_LIMITED', True, 'COOLDOWN', 'entry')
        if response and response.status == 403:
            await self._assert_no_challenge(page, "entry")
            raise SiteCheckError("HTTP_BLOCKED", False, "COOLDOWN", "entry")
        await self._reload_on_server_error(
            page,
            "entry",
            task.id,
            run_id,
            response.status if response else None,
        )
        await self._accept_cookies(page)
        await self._assert_no_challenge(page, "entry")

    def _debug_directory(self, task_id, run_id):
        root = Path(os.environ.get("DEBUG_DIR", self.settings.diagnostics_directory))
        if not root.is_absolute():
            root = PROJECT_ROOT / root
        sessions = root / "sessions"
        sessions.mkdir(parents=True, exist_ok=True, mode=0o700)
        session_name = re.sub(r"[^A-Za-z0-9_.-]", "_", f"{task_id}-{run_id}")
        directory = sessions / session_name
        directory.mkdir(parents=True, exist_ok=True, mode=0o700)
        cutoff = time.time() - self.settings.diagnostics_retention_days * 86400
        for old in sessions.iterdir():
            if old != directory and not old.is_symlink() and old.is_dir() and old.stat().st_mtime < cutoff:
                shutil.rmtree(old)
        return directory

    async def _save_response(self, response, task_id, run_id):
        """Keep raw navigation HTML even when a fast redirect replaces its DOM."""
        if getattr(getattr(self, "_assist_control", None), "diagnostics_enabled", None) is False or not self.settings.diagnostics_enabled or not response.request.is_navigation_request():
            return
        try:
            directory = self._debug_directory(task_id, run_id)
            stamp = datetime.now().astimezone().strftime("%Y%m%dT%H%M%S%f%z")
            base = directory / stamp
            metadata = {"url": _safe_page_url(response.url), "run_id": run_id,
                        "saved_at": datetime.now().astimezone().isoformat(),
                        "error_code": "TRACE_RESPONSE", "status": response.status,
                        "method": response.request.method}
            if _is_icp_page(response.url) and "html" in response.headers.get("content-type", "").lower():
                body = await response.body()
                path = base.with_suffix(".response.html")
                path.write_bytes(body)
                path.chmod(0o600)
                metadata["html"] = path.name
            else:
                metadata["metadata_only"] = True
            path = base.with_suffix(".response.json")
            path.write_text(json.dumps(metadata, ensure_ascii=False, indent=2), encoding="utf-8")
            path.chmod(0o600)
        except Exception as exc:
            log_event(log, logging.WARNING, "assist_response_capture_failed",
                      task_id=task_id, run_id=run_id, error_type=type(exc).__name__)

    async def _save_debug(
        self,
        page,
        task_id: str,
        run_id: str,
        *,
        error_code: str | None = None,
        error_field: str | None = None,
        support_id: str | None = None,
        include_controls: bool = False,
    ) -> bool:
        if getattr(getattr(self, "_assist_control", None), "diagnostics_enabled", None) is False or not self.settings.diagnostics_enabled:
            return False
        try:
            directory = self._debug_directory(task_id, run_id)
            stamp = datetime.now().astimezone().strftime("%Y%m%dT%H%M%S%f%z")
            base = directory / stamp
            artifact_paths: list[Path] = []
            controls = None
            frames = []
            # External Cl@ve/AEAT pages may contain QR codes or authentication
            # material. Keep only non-sensitive metadata for those origins.
            initial_url = page.url
            if _is_icp_page(initial_url):
                from .page_inspection import is_icp_frame
                identity = """() => {
                    const root = document.documentElement;
                    if (!root.hasAttribute('data-cita-document')) root.setAttribute('data-cita-document',crypto.randomUUID());
                    return root.getAttribute('data-cita-document');
                }"""
                initial_document = await page.evaluate(identity)
                initial_frames = list(getattr(page, "frames", ()))
                frame_identities = []
                external_masks = []
                for index, frame in enumerate(initial_frames):
                    if frame == page.main_frame:
                        continue
                    frame_url = frame.url
                    if not is_icp_frame(frame):
                        owner = await frame.frame_element()
                        await owner.evaluate("(el, index) => el.setAttribute('data-cita-capture-mask',String(index))", index)
                        external_masks.append(frame.parent_frame.locator(f'[data-cita-capture-mask="{index}"]'))
                        frame_identities.append((frame, frame_url, None))
                    else:
                        frame_identities.append((frame, frame_url, await frame.evaluate(identity)))
                if include_controls:
                    controls = await page.evaluate("""() => Array.from(
                        document.querySelectorAll('input, select, textarea, button')
                    ).map(el => ({
                        tag: el.tagName.toLowerCase(), id: el.id, name: el.name,
                        type: el.type, disabled: el.disabled,
                        text: el.tagName === 'BUTTON' ? el.innerText.trim() : undefined,
                        options: el.tagName === 'SELECT' ? Array.from(el.options).map(o => ({
                            value: o.value, text: o.text, disabled: o.disabled, selected: o.selected
                        })) : undefined
                    }))""")
                control = getattr(self, "_assist_control", None)
                from .desktop_session import screen_locked
                capture_image = not screen_locked() and (control is None or (not control.booking_handoff and control.snapshot()["mode"] not in {"manual", "paused"}))
                screenshot_options = {"mask": external_masks} if external_masks else {}
                # Full-page Firefox capture resizes the viewport and can fight
                # manual scrolling. Preserve the current viewport; HTML is complete.
                # Screenshots use browser rendering machinery even in viewport
                # mode. Once the human takes over, keep passive HTML only.
                image = await page.screenshot(full_page=False, **screenshot_options) if capture_image else None
                html = await page.content()
                pending_files = [(base.with_suffix(".html"), html.encode("utf-8"))]
                if image is not None:
                    pending_files.append((base.with_suffix(".png"), image))
                for index, frame in enumerate(initial_frames):
                    if frame == page.main_frame:
                        continue
                    record = {"index": index, "url": _safe_page_url(frame.url)}
                    if is_icp_frame(frame):
                        frame_path = base.with_suffix(f".frame-{index}.html")
                        pending_files.append((frame_path, (await frame.content()).encode("utf-8")))
                        record["html"] = frame_path.name
                        record["error_kind"] = _dead_page_kind(await frame.locator("body").inner_text(timeout=2_000))
                    else:
                        record["metadata_only"] = True
                    frames.append(record)
                # Keep all bytes in memory until the original origin/documents
                # are verified again. An ICP -> Cl@ve navigation must not write
                # authentication content under an earlier ICP classification.
                if page.url != initial_url or await page.evaluate(identity) != initial_document:
                    return False
                if list(getattr(page, "frames", ())) != initial_frames:
                    return False
                for frame, frame_url, token in frame_identities:
                    if frame.url != frame_url or (token is not None and await frame.evaluate(identity) != token):
                        return False
                for path, content in pending_files:
                    with path.open("wb") as output:
                        path.chmod(0o600)
                        output.write(content)
                    artifact_paths.append(path)
            base.with_suffix(".json").write_text(
                json.dumps(
                    {
                        "url": _safe_page_url(page.url),
                        "run_id": run_id,
                        "saved_at": datetime.now().astimezone().isoformat(),
                        "error_code": error_code,
                        "error_field": error_field,
                        "support_id": support_id,
                        "frames": frames,
                        **({"controls": controls} if include_controls else {}),
                    },
                    ensure_ascii=False,
                    indent=2,
                ),
                encoding="utf-8",
            )
            artifact_paths.append(base.with_suffix(".json"))
            for path in artifact_paths:
                path.chmod(0o600)
            log_event(
                log,
                logging.INFO,
                "diagnostics_saved",
                task_id=task_id,
                run_id=run_id,
                page_url=_safe_page_url(page.url),
                error_code=error_code,
                error_field=error_field,
                support_id=support_id,
            )
            return True
        except Exception:
            log_event(log, logging.WARNING, "diagnostics_save_failed", task_id=task_id, run_id=run_id)
            return False
