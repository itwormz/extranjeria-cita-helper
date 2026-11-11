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

SantaCita development targets `santacita-beta`; Python maintenance stays on `main` during the beta period. The repository has not been renamed. Use the pinned Rust toolchain and Node.js 24:

```sh
cargo test --workspace --locked
node desktop-ui/scripts/prepare.mjs
node --test desktop-ui/tests/*.test.mjs packaging/tests/*.test.mjs
node packaging/check-docs.mjs
```

[Build instructions](docs/BUILD.md) cover optimized native packages and isolated development data. [Release instructions](docs/release-pr.md) describe version preparation through a PR. Do not publish packages, move release tags or claim native platforms were verified by source tests alone.

Use fictitious profiles and synthetic browser pages. Do not run tests against the live appointment service or disrupt an open booking. Keep user-controlled manual steps and protective timings intact.

Keep PRs focused and state the resulting behavior, verification and platforms actually tested. Update English/Spanish mappings alongside Russian UI text and keep all three README files consistent. Never commit runtime data, credentials, captures or generated builds. Review the actual candidate list before staging.

## Licence and conduct

Contributions must be compatible with [PolyForm Noncommercial 1.0.0](LICENSE).
Preserve third-party licence notices. Submitting a contribution does not require
transferring copyright to me.

Follow the [code of conduct](CODE_OF_CONDUCT.md) when participating.
