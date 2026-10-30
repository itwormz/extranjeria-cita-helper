# Native builds and GitHub Releases

Build each package on its target OS. Python dependencies are pinned in `uv.lock`.

| Runner | Package |
| --- | --- |
| macos-15 | macOS arm64 DMG |
| macos-15-intel | macOS Intel DMG |
| ubuntu-22.04 | Linux x64 AppImage |
| windows-2022 | Windows x64 MSI |

## Local build

```sh
uv python install 3.13
uv sync --frozen --python 3.13 --extra desktop --extra build
uv run python -m camoufox fetch
uv run python scripts/build_desktop_app.py
```

Linux also needs Firefox/Qt system libraries:

```sh
uv run python -m playwright install-deps firefox
sudo apt-get install libgl1 libegl1 libxcb-cursor0 libxkbcommon-x11-0 libpulse0 libxcb-keysyms1 libxcb-icccm4 desktop-file-utils zsync
```

Output is `data/dist/Cita Extranjeria Helper.app` on macOS and `data/dist/Cita Extranjeria Helper/` on Windows/Linux. Extract and distribute the whole bundle, including `_internal`; a standalone executable is insufficient. Use `--output-dir PATH` to preserve an existing local build.

Run offline packaging checks:

```sh
uv run python scripts/verify_bundle.py "data/dist/Cita Extranjeria Helper.app"
```

Use the directory without `.app` on Windows/Linux. This checks bundled Camoufox and worker pipes without contacting the appointment site. Source tests run with `uv run pytest`; browser fixture tests are opt-in with `CITA_BROWSER_TESTS=1` and route all website requests to synthetic local responses.

Package the verified bundle:

```sh
uv run python scripts/package_release.py --platform macos --arch arm64
```

Use `windows --arch x64` or `linux --arch x64` on those systems. Before Linux packaging, run `uv run python scripts/prepare_appimage.py data/build-appimage-tools` and export the two paths it prints (`APPIMAGETOOL` and `APPIMAGE_RUNTIME`). The official tool and its embedded runtime are pinned by SHA256. Outputs are
written to `data/releases/`. macOS uses the built-in `hdiutil`. Windows uses
WiX Toolset **4.0.6**, installed with `dotnet tool install --global wix --version
4.0.6`; .NET SDK 8 and `DOTNET_ROLL_FORWARD=Major` are configured in Actions.
MSI installs for the current user under Local AppData/Programs, creates a Start
Menu shortcut and supports major upgrades using a stable UpgradeCode. User data
is stored outside the installation and survives upgrades and removal. Linux AppImage contains the complete application/browser bundle in one executable file. All packages include the
project license; browser and dependency notices retain their own terms.

Windows MSI is the default. To create the optional portable edition, add `--portable` to the Windows packaging command. Extract the ZIP as a whole; `portable.flag` makes profiles, browser sessions and diagnostics use the adjacent `data/` directory. Installed MSI editions continue to use Local AppData. Portable archives must never be built from a directory containing real user data.

Run Linux AppImage with execute permission (`chmod +x file.AppImage`). On systems without FUSE, use `file.AppImage --appimage-extract-and-run`.

## GitHub Actions

`.github/workflows/build.yml` runs manually through **Actions → Build desktop packages → Run workflow**, and on `v*` tags. It tests source/privacy, builds all four native targets, verifies the browser and worker offline, then uploads packages with SHA256 sidecars as Actions artifacts (14 days).

Set the same numeric version in `app/version.py`, `pyproject.toml`, and the project entry in `uv.lock`; update `CHANGELOG.md`. Versions use MAJOR.MINOR.PATCH: fixes increment PATCH, features MINOR, incompatible changes MAJOR. All CPU architectures share one version. README headers use a dynamic latest-published-release badge and do not need a source-version edit. A tag such as `v0.1.0` must match the source version exactly.

After every native build and verification passes, a tag run automatically publishes a **GitHub Release** with two DMGs, one MSI, one Windows portable ZIP, one Linux AppImage, their Sigstore bundles and `SHA256SUMS`. Release notes include the matching `CHANGELOG.md` section. Packages are first uploaded to a draft; it becomes public only after all uploads succeed. Failed uploads leave a draft for investigation. Existing releases are never overwritten by reruns. Build jobs have read-only repository contents access plus `id-token: write` and `attestations: write` for keyless tag-build attestations; only the publish-release job receives `contents: write`. No personal credentials are needed; it uses the scoped GitHub token.

For a prepared source version of `0.2.1`, push the source commit and its matching
tag (use the actual source version for later releases):

```sh
git push origin main
git tag v0.2.1
git push origin v0.2.1
```

A normal push to `main` does not start this workflow or publish a release. Manual workflow runs build/test packages without publishing them. Pull requests run the separate lightweight Tests and CodeQL workflows.

## Signatures and browser resources

macOS bundles use an ad-hoc signature, without Developer ID or notarization. Windows bundles are unsigned. Signing with a verified publisher is a separate release setup requiring the appropriate certificates stored in GitHub Secrets.

Only public resources are bundled. The macOS builder omits the large Camoufox font collection tested against system fonts; Windows/Linux omit font groups the host does not load. Legacy flat Windows bundles retain files containing configured Windows faces (including complete collections), unrecognized files and notices; unrelated faces are omitted. Browser version metadata and bundled notices remain with the browser. Camoufox is fetched separately from Python dependencies, so release maintainers should record its version from `bundled-browser/version.json` and inspect download verification in CI logs.

Local macOS tests do not replace native Windows/Linux CI or manual checks of certificate/Cl@ve authentication, sounds, background operation and final manual booking. CI deliberately never requests real appointments.

References: [PyInstaller](https://pyinstaller.org/en/stable/usage.html), [GitHub runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners), [GitHub releases](https://cli.github.com/manual/gh_release_create).

## Local gate before pushing

Install `actionlint` (macOS: `brew install actionlint`), synchronize the desktop/build
dependencies and fetch Camoufox as described above. Then run:

```sh
uv run python scripts/preflight.py
# Also build and smoke-test the native bundle on the current OS:
uv run python scripts/preflight.py --bundle
```

This checks workflow syntax with actionlint, version consistency, source privacy,
the standard test suite and the same offline browser regressions as CI, including
Toma de huellas / TIE and Autorización de regreso (`tests/test_regreso_browser.py`),
plus passive Telegram booking details and applicant warning modes
(`tests/test_booking_details_browser.py`).
macOS also runs the locked-session simulations. All browser requests use synthetic pages;
the gate does not commit, push, tag or publish. A failure stops the sequence.
The optional native bundle check does not install the application. Native Windows,
Linux and Intel macOS results still require their respective CI runners; a local
pass cannot guarantee identical timing or installer behavior on other systems.

Tag builds produce the Windows portable ZIP alongside MSI. Manual workflow runs
include it by default and can disable it through the portable input.

For changes that affect builds, use this release order: local preflight → push a
branch and open a pull request → wait for Tests and both CodeQL analyses → run
the native matrix manually on that branch when build/runtime changes need it →
merge → create the release tag. PRs run the lightweight Linux test suite without
downloading Camoufox or packaging four platforms. Full native builds run only
manually or for release tags; only tag builds publish a release. New manual runs
on the same ref cancel older package runs; release tag builds are never cancelled
by a newer run. PR test and CodeQL runs supersede older runs for the same PR.

## Signed build provenance

Tag builds use pinned `actions/attest` after final packaging, then attach its public
Sigstore bundle as `<package>.sigstore.json` for each package. Before any release
upload, `scripts/create_draft_release.py` calls the same verifier as the updater
and requires the workflow source commit to equal `GITHUB_SHA`. Missing or invalid
bundles abort publication. Manual builds do not mint release attestations.

The application uses the Sigstore Python verifier against the production trusted
root embedded in its locked dependency. It does not read user TUF caches, request
OIDC credentials, contact Sigstore during verification, or invoke GitHub CLI.
Unknown signing roots fail closed and require a verifier/application update. The
public bundle is verified for the exact repository, `build.yml`, release tag,
GitHub-hosted runner, source commit and package SHA256; installers are rechecked.
Native package smoke tests verify an authentic public GitHub CLI build bundle
offline, exercising the bundled cryptographic libraries and root resources.

Repository release immutability is enabled. Finish all uploads in the draft before
publishing; after publication use a new version/tag for fixes, never rewrite one.
An initial upgrade from an older checksum-only client cannot retroactively gain
provenance checks. A provenance signature does not approve the source code; branch
and workflow access controls remain necessary.

## Security checks before a release

CodeQL uses the explicit `.github/workflows/codeql.yml` workflow (Advanced setup),
with separate Python and GitHub Actions analyses on pull requests to `main`, pushes
to `main`, a weekly schedule and manual runs. Pull requests use `pull_request`,
including Dependabot; never use `pull_request_target` to analyze untrusted code.
The job only reads repository content and uploads code-scanning results. It does
not install application dependencies, access application secrets or publish packages.
Both `Analyze (python)` and `Analyze (actions)` must finish successfully before
merging; a neutral "configurations not found" result is not a successful scan.

Disable Default setup only when activating this explicit workflow, and verify that
both `/language:python` and `/language:actions` results appear on `main` and the PR.
Do not enable both setup modes: Default setup blocks advanced CodeQL uploads.

Dependabot temporarily excludes Playwright versions `>=1.63`, matching Camoufox's
current compatibility ceiling. Revisit this exclusion when upgrading Camoufox;
do not force an unsupported driver/browser combination. This is not an exemption
from reviewing dependency vulnerability alerts or testing browser flows.


Review open CodeQL, Dependabot and secret-scanning alerts before tagging. Update manifest constraints and `uv.lock` together, then run the local preflight and native package verification with the frozen updated environment. A dependency audit is a point-in-time check; a green package build does not replace it. GitHub closes addressed findings only after it analyzes the pushed commit. Never dismiss an alert just to make the release gate green.

Windows CI validates the .NET 8 SDK and runtime already installed in the pinned runner family before installing WiX 4.0.6. Missing prerequisites fail explicitly; the workflow does not download another SDK through `aka.ms`. This avoids an unnecessary dependency on SDK download redirects while preserving the native MSI build and verification steps.

When `cryptography` requires a source build (currently macOS Intel), uv sets `OPENSSL_STATIC=1` through the project configuration. This keeps its OpenSSL symbols separate from Python’s bundled OpenSSL. Prebuilt wheels remain preferred; the packaged smoke test must import the attestation verifier successfully before a package can be published.

## Local development builds

Keep local work on a `codex/` development branch and use numbered PEP 440 versions such as `0.2.2.dev1`, `0.2.2.dev2`. Update `app/version.py`, `pyproject.toml`, the project entry in `uv.lock` and `CHANGELOG.md` together. The app displays the full development version; native numeric metadata uses the corresponding platform format. MSI uses the base three-part version, so a dev MSI is not a separate stable upgrade channel; use a portable build for Windows development checks. Stable update comparison accepts a development current version but still rejects development release tags. Do not create release tags for local builds. The package workflow also requires a stable source version for MSI/DMG/AppImage/portable packaging; a dev branch can run source and offline browser checks, but must not be submitted to that workflow as a stable release. Local commits do not run the package workflow; pushing a stable `vMAJOR.MINOR.PATCH` tag is the publication trigger.

## Browser regression contracts

Real-browser fixtures use supported application navigation budgets; they must not shorten a navigation deadline below the configuration minimum to speed up a test. Confirm visible source controls and the expected destination, not only completion of a click. Delayed Regreso responses explicitly check one POST, exact applicant fields and the recognized request menu. Pure timeout tests use controlled dependencies separately.

The regular browser suite reports all failures in a run rather than stopping at the first one; it does not retry failed tests. CI browser steps write JUnit reports; failures upload them as `browser-failure-<platform>-<arch>` artifacts for seven days. The Regreso submission scenario also preserves a Playwright trace of its intercepted synthetic pages on failure. These diagnostics are separate from `package-*` artifacts and never enter release assets. No real appointment data or live website requests are required. Production pauses, action/navigation timeouts and POST safeguards are unchanged.

## AppImage catalog metadata and compatibility

Linux packages use `Cita_Extranjeria_Helper-<version>-x86_64.AppImage`, matching
`Name=Cita Extranjeria Helper` in the desktop entry. There is no legacy-name alias.
The first release using this naming requires users of 0.2.3 and earlier to download
the Linux AppImage manually from GitHub Releases and make it executable. Their old
updater expects the previous filename and cannot download the renamed package.
The new updater selects the new filename and retains checksum/provenance verification.

The AppImage embeds `gh-releases-zsync` metadata pointing to this repository's
latest stable release and `Cita_Extranjeria_Helper-*-x86_64.AppImage.zsync`.
Install `zsync` (providing `zsyncmake`) before packaging. Packaging requires a
nonempty `.zsync` sidecar; tag builds attest both files, and publication verifies
both checksums and attestations before uploading them. AppImageUpdate can use
these files; its verification mechanism is separate from the application's built-in
Sigstore-verified updater. See the [AppImage update documentation](https://docs.appimage.org/packaging-guide/optional/updates.html).

The Linux build remains on Ubuntu 22.04 and requires system glibc 2.35 or newer.
The AppImage includes the application and browser but uses the host C library;
older Linux distributions are not supported by this build. A suitable glibc
version alone does not guarantee all desktop/runtime requirements are satisfied.
