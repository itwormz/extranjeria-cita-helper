# Working on SantaCita

This is the Rust/Tauri/Svelte line for **Toma de huellas / TIE** and **Autorización de regreso** on ICP Plus. Python 0.3.x remains on `main` during beta testing; SantaCita targets `santacita-beta`. Do not merge or rename branches, publish, tag or push without the user's instruction. The repository name stays `itwormz/extranjeria-cita-helper` until a separate rename.

Read README.md, CHANGELOG.md and relevant modules before changing behavior.

## Behavior boundaries

- Date/time, CAPTCHA and final booking confirmation remain manual. Preserve the user's scroll, input and current page during handoff and pause; do not capture screenshots then.
- Exactly one valid office can be selected automatically; multiple offices require user choice. Fill applicant data before requesting an appointment.
- Recognize the actual document before acting. Embedded session-expiry messages take precedence over surrounding forms.
- A dispatched action is not proof of navigation. Never replay an ambiguous POST or discard protective timings in favor of tight retries.
- Browser behavior is tested separately from native window visibility and lock behavior. Do not claim Windows/Linux/macOS native verification from source or headless tests.
- Keep profiles, credentials, browser data and real diagnostics out of Git. Use synthetic profiles in tests and preserve any active booking.

## Code map

| Area | Location |
| --- | --- |
| Flow, recognition, timing, budgets | `crates/ceh-core/` |
| Installed browser discovery, CDP/BiDi, input and page capture | `crates/ceh-browser/` |
| YAML profiles, SQLite, history and Python import | `crates/ceh-store/` |
| Notifications, native vault, exports and verified updates | `crates/ceh-services/` |
| Desktop commands, search controller and view models | `crates/ceh-desktop/` |
| Svelte interface, translations and UI contracts | `desktop-ui/`, `assets/translations.json` |
| Public resources and contract fixtures | `assets/`, `contracts/fixtures/`, `catalogs/` |
| Native packaging, release metadata and workflows | `packaging/`, `.github/workflows/` |

## Checks and release preparation

Use `rust-toolchain.toml`, Node 24, locked Cargo/npm dependencies. Run checks appropriate to the changed behavior, once per meaningful completed block:

```sh
cargo test --workspace --locked
node desktop-ui/scripts/prepare.mjs
node --test desktop-ui/tests/*.test.mjs packaging/tests/*.test.mjs
node packaging/check-docs.mjs
```

Browser/vault live checks are opt-in and require authorization. Never repeat a real booking attempt as a test side effect. Native packages must be built on their target OS; see docs/BUILD.md.

Release version is declared in `.github/release-version` and prepared with `node packaging/release.mjs prepare`. Keep Cargo.toml, all workspace versions in Cargo.lock, frontend package/lock and the versioned CHANGELOG section consistent. Tags are `vX.Y.Z-beta.N` for beta, `vX.Y.Z` for stable. A release PR has final notes and versions before merge. Manual builds do not publish; normal infrastructure PRs do not tag.

Keep the build workflow path `.github/workflows/build.yml`, GitHub-hosted push tag identity, exact package names and strict SHA256/Sigstore policy compatible with Python 0.3.8. Never skip verification on errors or substitute an arbitrary repository/URL.

## UI, data and documentation

Reuse existing components, spacing and theme variables. Update English/Spanish mappings in desktop-ui/src/messages.json or assets/translations.json with Russian source labels. Keep all three README files consistent and screenshots fictitious. Public booking details enrich the original event without changing its time or recounting it; hidden IDs, CAPTCHA and confirmation numbers are not public statistics.

Preserve original Python data when importing. Native secrets retain compatible references; there is no plaintext fallback. Runtime and preview artifacts belong in ignored `.local/builds/`; preparation notes and audits belong under `.local/`. `tasks.yml` is private UI-managed storage, not a shipped sample.

Review actual candidate files before staging. Do not change Git author identity implicitly, clean up unrelated worktrees, overwrite parallel edits or commit other work. Preserve PolyForm Noncommercial 1.0.0 and third-party licence notices.
