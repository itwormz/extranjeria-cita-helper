# Working on Extranjería Cita Helper

## Scope and behaviour

This is a Python desktop assistant for **Toma de huellas / TIE** appointments
on ICP Plus. Other appointment procedures are outside the supported scope.
Read `README.md`, `CHANGELOG.md` and the relevant modules before making changes.

- Appointment date/time, CAPTCHA and final booking confirmation remain manual.
- When handing control to the user, stop form actions and preserve their scroll,
  input and open browser page. Manual mode and pause must not capture screenshots.
- Exactly one available office can be selected automatically; multiple offices
  require user choice. Fill applicant data before requesting an appointment.
- Recognize the current document before acting. An embedded expired-session
  message takes precedence over the surrounding form; follow its recovery control.
- A sent action is not a confirmed transition. Do not replay an ambiguous POST.
  Preserve rate-limit cooldowns and configurable timings instead of tight retries.
- Keep the browser usable behind other windows. Test visibility/lock changes
  separately; do not assume a headless test proves native background behaviour.
- Keep the normal browser configuration unchanged when fixing input failures.
  macOS locked-session keyboard handling lives in `app/desktop_session.py` and
  `app/site.py`; other platforms must not be reported as verified by that test.

## Code map

| Area | Files |
| --- | --- |
| CLI / desktop startup | `app/main.py`, `app/desktop/__init__.py` |
| Page recognition, actions and recovery | `app/assist_flow.py`, `app/site.py`, `app/page_inspection.py` |
| Browser lifecycle and visibility | `app/assist_runtime.py`, `app/browser_visibility.py`, `app/desktop_session.py` |
| Worker / controller communication | `app/desktop/worker.py`, `process.py`, `protocol.py`, `app/assist_control.py` |
| UI, themes and translations | `app/desktop/window.py`, `theme.py`, `i18n.py`, `assets/translations.json` |
| Profiles, timings and counters | `app/desktop/profiles.py`, `statistics.py`, `app/config.py`, `app/timings.py` |
| User timeline / page diagnostics | `app/assist_journal.py`, `app/diagnostics.py`, `app/logging_utils.py` |
| Updates / TLS intermediates | `app/desktop/updates.py`, `certificates.py` |
| Public dictionaries | `catalogs/snapshot-2026-08-12.json`, `catalogs/README.md` |
| Native builds and releases | `scripts/`, `.github/workflows/build.yml`, `docs/BUILD.md` |

Short filenames in this table refer to the directory of the preceding full path.

## Setup and checks

Commands below use a POSIX shell; source usage on Windows is documented through WSL.
Python dependencies are pinned in `uv.lock`; build environments use Python 3.13.

```sh
uv python install 3.13
uv sync --frozen --python 3.13 --extra desktop --extra build
uv run python -m camoufox fetch
uv run python -m app.main desktop
```

Before a release push, run `uv run python scripts/preflight.py` (add `--bundle` for a native bundle check); see `docs/BUILD.md`.

Run tests appropriate to the changed behaviour. Add regressions for meaningful
bugs; browser tests are opt-in and intercept website requests with synthetic pages.

```sh
uv run pytest
CITA_BROWSER_TESTS=1 CITA_TEST_ENGINE=camoufox uv run pytest tests/test_local_assist_browser.py tests/test_contact_fields_browser.py
uv run python scripts/check_privacy.py --tree .
```

`check_privacy.py --personal-source PATH` compares against a local configuration
without printing its values. Never treat a clean scan as proof against unknown
secrets. Native package checks and installer commands are in `docs/BUILD.md`.
Build on the target OS; do not present cross-platform source tests as native GUI
or installer validation. Preserve an active browser session when rebuilding.

## UI and documentation

- Keep colours in `theme.py`. Keep user logs focused on pages, actions and outcomes;
  raw capture/debug events belong in diagnostics. Do not translate site data.
- Update English and Spanish mappings when changing a translatable UI string.
- Keep the three README files consistent. Use only fictitious data in screenshots,
  with the same rectangular dimensions for the gallery.
- Version changes must agree in `app/version.py`, `pyproject.toml`, `uv.lock`,
  README headers and `CHANGELOG.md`. Use MAJOR.MINOR.PATCH.

## Privacy and repository hygiene

- Use fictitious profiles for tests. Live-site testing requires user authorization;
  do not close, refresh or submit an open booking as a side effect of a test.
- Never commit real applicant data, credentials, certificates, browser profiles,
  captures, logs, environment files or build output. See `docs/PRIVACY.md`.
- `tasks.yml` is private runtime storage managed through the UI, not a shipped
  sample or prerequisite users should create manually.
- Keep preparation notes, audits and temporary design material under ignored
  `.local/`. Keep public documentation limited to maintained project information.
- Review the actual Git candidate list before staging. Do not commit, push, tag
  or publish unless requested. Do not alter Git author identity implicitly.
- Project code uses PolyForm Noncommercial 1.0.0. Preserve its notice and the
  separate licences/notices of bundled third-party components.
