# Browser boundary

SantaCita uses an installed Firefox through WebDriver BiDi or Chrome, Chromium,
Brave and Windows Edge through its CDP adapter. Each search profile and engine
has a separate app-owned persistent browser profile. Personal tabs are not attached.
Snap installations are rejected because they cannot access that profile directory.

The desktop worker in `ceh-desktop/src/search.rs` owns browser startup, observation,
core effects, timing updates, recovery and shutdown. `SessionDriver` maps core
effects to guarded native actions; the interface cannot submit scripts or selectors.
The common transport records sanitized command summaries and first-party network
metadata. It never logs typed values.

## Safety and control

- Recognize the actual loaded document and its realm before an action. Embedded
  expiry and blocking messages take priority over surrounding forms.
- One valid office can be selected automatically; multiple offices require the person.
  Applicant and contact entry use trusted browser input with configured delays.
- Dispatch is not navigation. A proven unsent action may be retried after its configured
  pause; an ambiguous or sent POST is never replayed automatically.
- Date/time, CAPTCHA and final booking confirmation remain manual. Handoff and pause
  revoke the dispatch gate. Public office/date details are read without changing inputs
  or scroll; a failed enrichment read leaves the generic notification intact.
- Browser close detection, explicit continuation, a fresh pass and a cleared browser
  session are different operations. A possible active booking must remain available.
- HTTPS checks stay enabled. Optional intermediate-certificate preparation does not
  install roots or prevent startup when it cannot complete.

## Diagnostics

Optional diagnostics archive bounded first-party resources and sanitized page facts;
external authentication and manual booking documents are excluded. Network events are
observed without intercepting or replaying requests. Capture requires automatic ownership
and an unlocked console both before and after rendering. Unknown lock state disables
images and is reported as unavailable rather than as a confirmed lock.

## Checks

```sh
cargo test -p ceh-browser --features bidi --locked
```

Most tests use deterministic fixtures or local protocol servers. Installed-browser tests
are explicitly ignored by default and must be opted into on an isolated profile:

- `chromium_live_contract`: trusted input, navigation, frames, response metadata and
  stale realms on a localhost page (Chrome, Brave, Windows Edge).
- `firefox_contract` and the trusted-input unit fixture: owned launch, close, handoff,
  continuation and keyboard input.
- `firefox_background_live`: a separate native background-window probe.

Passing protocol tests does not establish native minimize, lock, macOS Spaces, RDP,
certificate dialogs or end-to-end booking behavior. Those require target-OS acceptance.
The desktop's Show Browser action uses the owned process identity; Linux activation
requires an X11/XWayland window. Native Wayland may require opening the window manually.
