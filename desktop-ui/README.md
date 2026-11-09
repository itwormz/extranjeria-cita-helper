# Rust desktop UI

Tauri + Svelte/TypeScript frontend for the Rust migration. The current preview
implements the main shell, basic profile editing, validated timing fields and
the two independent search-behavior options. Telegram/webhook settings, lazy credential editing, pairing and explicit test
actions are connected to Rust services. Profile statistics and office history read existing SQLite storage through scoped
Rust commands. The timeline supports clips, event markers and read-only selection
statistics; physical trackpad acceptance remains pending. Local appointment notes,
immutable achievement snapshots, Google Calendar/ICS preparation, public PNG cards
and text/card sharing are connected to scoped Rust commands. Private appointment
fields remain separate from public exports. Native Save dialogs and PNG clipboard
were checked on macOS; opening a social network never publishes automatically.
Browser integration, attention sound/status lifecycle and updates are still being migrated. Real notification delivery was not
used during preview acceptance; synthetic transport/GUI fixtures cover it.
It is not a replacement for the released Python desktop application yet.

The preview uses a separate application ID and runtime under
`.local/builds/runtime/rust-preview`; it does not automatically open the stable
application's profiles or credentials. A debug-only `CEH_PREVIEW_LEGACY_COPY` selector
can explicitly open a prepared `stable`, `historical`, `json-only` or `experimental`
copy under `.local/builds/runtime/legacy-compat/<case>/rust`; it never opens the
original runtime and labels the window as a legacy-data copy. Browser search stays disabled until the
Rust browser adapter is connected. Node is a build/test tool, not a sidecar.

## Local macOS development

Use the repository's Rust toolchain and Node 24 (`.nvmrc`). From the repository root:

```sh
node desktop-ui/scripts/prepare.mjs
npm run test --prefix desktop-ui
cargo test -p ceh-desktop
```

The preparation script installs the exact frontend lockfile under
`.local/builds/work/rust-ui`, generates the existing app icon and checks/builds
the frontend. `desktop-ui/node_modules` is an ignored link to those dependencies.
The frontend output is `.local/builds/work/rust-ui-dist`.

To compile the native shell without launching it:

```sh
cargo check -p ceh-desktop --features shell,custom-protocol --locked
```

To create the separate debug `.app`, after preparing the frontend:

```sh
cd crates/ceh-desktop
node ../../desktop-ui/node_modules/@tauri-apps/cli/tauri.js build --debug --features shell --bundles app --ci
```

Cargo output follows the workspace target directory under `.local/builds`.
The app bundle is in `rust-target/debug/bundle/macos/CEH Rust Preview.app`.
The Tauri build hook always checks and rebuilds frontend assets before embedding
them; it does not install dependencies. Run the preparation script first on a
fresh checkout. The macOS preview uses an explicit ad-hoc bundle signature, binding its Info.plist
and resources; it is not a Developer ID signature or notarization. Verify it with
`codesign --verify --deep --strict` before native acceptance. An already running
preview must be closed before replacing its bundle.

These instructions describe the current macOS preview; other native targets and
release installers have separate acceptance gates.

Only the local main webview can call desktop commands. Site content is not loaded
into that privileged view. UI assets reuse the existing application fonts,
translations, icons and theme; their original licences remain applicable.
