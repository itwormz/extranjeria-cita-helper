# Contributing

I welcome bug reports, documentation fixes, translations and focused code changes.
Issues and pull requests are welcome in **English, Spanish or Russian**.

## Report a problem or suggest a change

Use [Discussions](https://github.com/itwormz/extranjeria-cita-helper/discussions)
for usage questions, early ideas and experiences with the app. See the
[pinned welcome](https://github.com/itwormz/extranjeria-cita-helper/discussions/1)
for categories and guidance.

Use the [issue forms](https://github.com/itwormz/extranjeria-cita-helper/issues/new/choose)
for an application bug, a changed website screen or an improvement.
Describe what happened and what was expected. Include the application version and
operating system; for website problems, include the province and selected service.
Share additional technical details only if available. There is no need to repeat a live search or
risk an open appointment to reproduce a problem.

For a startup failure, **Copy report** in the error panel provides technical
information. Review it before posting. Screenshots and timeline excerpts are
optional. Do not publish NIE, DNI or passport numbers, names, phone/email,
appointment confirmation numbers, credentials, cookies, certificates, browser
profiles or raw page captures.
See [privacy notes](docs/PRIVACY.md). Report vulnerabilities through
[the security policy](SECURITY.md), rather than a public bug report.

This is an independent project maintained in my spare time. I cannot promise a
response time, a fix or an appointment.

## Scope

The supported procedures are **Toma de huellas / TIE** and **Autorización de regreso**
on the official ICP Plus site. Select the service in each profile. Fingerprinting
uses NIE and citizenship; Regreso supports NIE, DNI or passport. Other procedures
are outside the supported scope. Date/time selection, CAPTCHA and final booking
confirmation remain manual.
Changes must preserve user control of manual screens and existing request pauses.
Bulk appointment capture, resale, CAPTCHA solving and aggressive polling are
outside this project's scope.

For a substantial change, open an issue before implementation so its scope can be
agreed. Small fixes and documentation improvements can be proposed directly.

## Development and checks

Read [AGENTS.md](AGENTS.md) for the code map and behaviour constraints, and the
[README](README.md#-run-from-source) for setup and platform requirements.
The build environment uses Python 3.13 and dependencies pinned in `uv.lock`.

```sh
uv sync --frozen --python 3.13 --extra desktop --extra build
uv run python -m camoufox fetch
uv run pytest
uv run python scripts/check_privacy.py --tree .
```

Browser regressions run offline with intercepted requests and synthetic pages:

```sh
CITA_BROWSER_TESTS=1 CITA_TEST_ENGINE=camoufox uv run pytest tests/test_local_assist_browser.py tests/test_contact_fields_browser.py tests/test_procedure_groups_browser.py tests/test_regreso_browser.py
```

Before a release push, run `uv run python scripts/preflight.py`; add `--bundle`
to build and verify the native package on the current OS. The preflight includes
version/privacy/workflow checks and the standard and offline browser suites for
both supported services; it requires `actionlint`.

The environment-variable syntax above is for a POSIX shell. Native packaging and
platform checks are documented in [docs/BUILD.md](docs/BUILD.md).
Use fictitious profiles. Do not run automated test suites against the live service.

Keep pull requests focused. Explain the problem, resulting behaviour and checks
performed, including which platforms were actually tested. Add a regression for a
behavioural bug where practical. Keep English and Spanish translations aligned
with changed interface strings, and update the three READMEs when their content
changes. Release versions and tags are managed by the maintainer.

Never include runtime files, real profiles, credentials, captures or build output.
Review the actual diff before submitting; a privacy scan is an additional check,
not a guarantee that every secret has been found.

## Licence and conduct

Contributions must be compatible with [PolyForm Noncommercial 1.0.0](LICENSE).
Preserve third-party licence notices. Submitting a contribution does not require
transferring copyright to me.

Follow the [code of conduct](CODE_OF_CONDUCT.md) when participating.
