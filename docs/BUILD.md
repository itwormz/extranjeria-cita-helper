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
sudo apt-get install libgl1 libegl1 libxcb-cursor0 libxkbcommon-x11-0 libpulse0 libxcb-keysyms1 libxcb-icccm4 desktop-file-utils
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

`.github/workflows/build.yml` runs on pull requests, manually through **Actions → Build desktop packages → Run workflow**, and on `v*` tags. It tests source/privacy, builds all four native targets, verifies the browser and worker offline, then uploads packages with SHA256 sidecars as Actions artifacts (14 days).

Set the same numeric version in `app/version.py`, `pyproject.toml`, and the project entry in `uv.lock`; update `CHANGELOG.md`. Versions use MAJOR.MINOR.PATCH: fixes increment PATCH, features MINOR, incompatible changes MAJOR. All CPU architectures share one version. README headers use a dynamic latest-published-release badge and do not need a source-version edit. A tag such as `v0.1.0` must match the source version exactly.

After every native build and verification passes, a tag run automatically publishes a **GitHub Release** with two DMGs, one MSI, one Windows portable ZIP, one Linux AppImage, their Sigstore bundles and `SHA256SUMS`. Release notes include the matching `CHANGELOG.md` section. Packages are first uploaded to a draft; it becomes public only after all uploads succeed. Failed uploads leave a draft for investigation. Existing releases are never overwritten by reruns. Build jobs have read-only repository contents access plus `id-token: write` and `attestations: write` for keyless tag-build attestations; only the publish-release job receives `contents: write`. No personal credentials are needed; it uses the scoped GitHub token.

For the prepared version, push the source commit and its matching tag:

```sh
git push origin main
git tag v0.1.1
git push origin v0.1.1
```

A normal push to `main` does not start this workflow or publish a release. Manual workflow runs and pull requests build/test packages without publishing them.

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
the standard test suite and the same offline browser regressions as CI. macOS also
runs the locked-session simulations. All browser requests use synthetic pages;
the gate does not commit, push, tag or publish. A failure stops the sequence.
The optional native bundle check does not install the application. Native Windows,
Linux and Intel macOS results still require their respective CI runners; a local
pass cannot guarantee identical timing or installer behavior on other systems.

Tag builds produce the Windows portable ZIP alongside MSI. Manual workflow runs
include it by default and can disable it through the portable input.

For changes that affect builds, use this release order: local preflight → push a
branch and open a pull request → wait for the native matrix to pass → merge →
create the release tag. Pull-request builds never publish a release. Alternatively,
run the workflow manually without a tag to validate packages before tagging.

## Signed build provenance

Tag builds use pinned `actions/attest` after final packaging, then attach its public
Sigstore bundle as `<package>.sigstore.json` for each package. Before any release
upload, `scripts/create_draft_release.py` calls the same verifier as the updater
and requires the workflow source commit to equal `GITHUB_SHA`. Missing or invalid
bundles abort publication. Manual/PR builds do not mint release attestations.

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
