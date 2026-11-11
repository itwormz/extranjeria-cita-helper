# Building SantaCita

This document describes the Rust branch. Python 0.3.x remains on `main` during beta testing. The repository is still `itwormz/extranjeria-cita-helper`; renaming it is a separate operation.

## Toolchain and source checks

Use the Rust version in `rust-toolchain.toml` and Node.js **24**. Cargo and npm dependencies are locked by `Cargo.lock` and `desktop-ui/package-lock.json`.

```sh
node packaging/release.mjs metadata
node --test packaging/tests/*.test.mjs
cargo test --workspace --locked
node desktop-ui/scripts/prepare.mjs
node --test desktop-ui/tests/*.test.mjs
```

`prepare.mjs` installs the locked frontend dependencies, generates icons, type-checks Svelte and builds the frontend. Its dependencies and generated files stay in `.local/builds/`.

To run a development copy without touching installed profiles:

```sh
CEH_DATA_DIR="$PWD/.local/builds/runtime/dev" cargo run -p ceh-desktop --features shell,custom-protocol
```

In Windows PowerShell, set `$env:CEH_DATA_DIR` to an absolute test directory before the Cargo command. No test needs a real applicant profile or live appointment request. Native browser fixtures and vault tests are opt-in; do not turn them on against an active booking.

## Release compilation

The workspace defines `[profile.release]`:

```toml
opt-level = 3
lto = "thin"
codegen-units = 1
strip = "symbols"
debug = 0
incremental = false
```

The packager uses Tauri's release build with `--locked`, never `--debug`. Panic behavior is unchanged. Symbols and debug information are stripped, so development builds remain the appropriate place for detailed debugger work.

Build on the target operating system:

```sh
node desktop-ui/scripts/prepare.mjs
node packaging/build.mjs macos arm64
# Other native targets: macos x64, windows x64, linux x64
```

All outputs are under `.local/builds/releases/`. The Cargo target directory is `.local/builds/rust-target/`.

| Runner | Final package |
| --- | --- |
| `macos-15` | `SantaCita-<version>-macOS-Apple-Silicon.dmg` |
| `macos-15-intel` | `SantaCita-<version>-macOS-Intel.dmg` |
| `windows-2022` | `SantaCita-<version>-windows-x64.msi` and `SantaCita-<version>-windows-x64-portable.zip` |
| `ubuntu-22.04` | `SantaCita-<version>-x86_64.AppImage` |

macOS uses ad-hoc signing without notarization. `packaging/macos/make-dmg.sh` creates the final image with the app, Applications link and visible licence; it verifies the image and its contents. The native packager checks the app signature.

Windows requires .NET 8 and WiX **4.0.6** (`dotnet tool install --global wix --version 4.0.6`). MSI installs for the current user. The portable ZIP contains the EXE, licence and `portable.flag`, never a real `data` directory. WebView2 is a runtime prerequisite; the MSI does not install it.

Linux requires `libwebkit2gtk-4.1-dev`, `build-essential`, `libssl-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `patchelf`, `libfuse2` and `file` to build. Ubuntu 22.04 is the baseline. CI extracts the AppImage and checks the executable and desktop entry. This is not a native GUI or browser test.

## Workflows

- **Tests:** Rust workspace, frontend tests and Svelte checks for code changes. Documentation-only changes use local-link and tooling checks; failed/unknown scope classification runs full checks.
- **CodeQL:** Rust, JavaScript/TypeScript and GitHub Actions. Documentation-only checks are explicitly exempt. Scheduled and manual scans run normally.
- **Build SantaCita packages:** a manual run builds the four native targets and uploads artifacts, without publishing. A pushed matching version tag also creates provenance and publishes after every target succeeds.
- **Prepare or tag SantaCita release PR:** prepares versions and notes in a `release/<version>` branch; a merged internal release PR creates its tag. A normal infrastructure PR does not create a tag.

Review handoff automation currently runs from the default Python branch. Before opening the beta PR, its allowed target branches must include `santacita-beta`; do not install a second competing scheduler in this branch. The CodeRabbit label gate and Greptile-first policy remain in the repository configuration.

Future branch protection for `santacita-beta` must use `Tests`, `Analyze (rust)`, `Analyze (javascript-typescript)` and `Analyze (actions)`. Python `main` retains its own workflows and required checks. Repository-level Dependabot configuration is read from the default branch: enabling Cargo/npm updates for beta while Python is default needs a separate update there after the beta branch exists.

## Provenance and publication

The workflow path is `.github/workflows/build.yml`, the event is a pushed release tag and builders are GitHub-hosted. Final package names are assigned **before** `actions/attest`. Each package, including portable ZIP, has a `<package>.sigstore.json` and an entry in `SHA256SUMS` (`sha256`, two spaces, basename).

The publisher validates the complete five-package matrix and calls the application's native offline Sigstore verifier for every package. It checks the exact repository, workflow, tag, source commit, subject name and digest, plus certificate chain and transparency evidence against the pinned library's embedded trust root.

Only then does it create a draft, upload all packages/proofs/checksums and publish. Betas are prereleases with `latest=false`; the stable Python release remains latest. Release immutability must remain enabled in repository settings. Existing releases, including failed drafts, are not overwritten automatically: inspect them before retrying. Published versions/tags must never be moved.

[Release PR instructions](release-pr.md).

## Native release checklist

Source tests and a successful compile do not prove native installation. Before the first public beta, run the manual native matrix and check installation/launch on macOS, Windows and Linux. Specifically exercise MSI beta-to-beta/stable upgrades, Windows user-session WebView2/browser startup, portable replacement, installed/portable data migration and a real signed update cycle. Preserve old application data and an active booking during all local work.
