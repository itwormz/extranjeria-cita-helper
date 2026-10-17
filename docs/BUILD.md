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

`.github/workflows/build.yml` runs on pull requests, manually through **Actions → Build desktop packages → Run workflow**, and on `v*` tags. It tests source/privacy, builds all four packages, verifies the browser and worker offline, then uploads packages with SHA256 sidecars as Actions artifacts (14 days).

Set the same numeric version in `app/version.py`, `pyproject.toml`, and the project entry in `uv.lock`; update `CHANGELOG.md`. Versions use MAJOR.MINOR.PATCH: fixes increment PATCH, features MINOR, incompatible changes MAJOR. All CPU architectures share one version. A tag such as `v0.1.0` must match the source version exactly.

After every native build passes, a tag run creates a **draft Release** with two DMGs, one MSI, one Linux AppImage and `SHA256SUMS`. Review the packages, finish the release notes and publish the draft in GitHub. Existing releases are never overwritten by reruns. Build jobs use read-only repository access; only the draft-release job receives `contents: write`. No personal credentials are needed; it uses the scoped GitHub token.

## Signatures and browser resources

macOS bundles use an ad-hoc signature, without Developer ID or notarization. Windows bundles are unsigned. Signing with a verified publisher is a separate release setup requiring the appropriate certificates stored in GitHub Secrets.

Only public resources are bundled. The macOS builder omits the large Camoufox font collection tested against system fonts; Windows/Linux omit font groups the host does not load. Legacy flat Windows bundles retain files containing configured Windows faces (including complete collections), unrecognized files and notices; unrelated faces are omitted. Browser version metadata and bundled notices remain with the browser. Camoufox is fetched separately from Python dependencies, so release maintainers should record its version from `bundled-browser/version.json` and inspect download verification in CI logs.

Local macOS tests do not replace native Windows/Linux CI or manual checks of certificate/Cl@ve authentication, sounds, background operation and final manual booking. CI deliberately never requests real appointments.

References: [PyInstaller](https://pyinstaller.org/en/stable/usage.html), [GitHub runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners), [draft releases](https://cli.github.com/manual/gh_release_create).
