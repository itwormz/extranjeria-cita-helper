# Browser boundary

Default builds provide deterministic replay and a shared command-publication gate.
The optional `bidi` feature provides a bounded, blocking loopback WebSocket transport
and native page observations plus guarded province, procedure, single-office,
applicant, contact, request, known no-slots exits, authentication and cookie actions. Run it on an owned
browser worker, never the UI thread. It does not yet launch a production browser or implement the complete
profile-action map; desktop search remains disabled until those are integrated.

`BidiConnection` separates one command write from its response. A timeout, unexpected
response ID, invalid stream or event-budget overflow invalidates the connection;
there are no automatic retries or reconnects. Raw messages and page text are not
included in errors. Message/frame, event-count/bytes and timeout bounds are enforced.

`DispatchGate` shares trusted session/epoch/mode authority with the controller.
Only the bounded write belongs inside its closure, never response waiting. An
ambiguous delivery remains reserved across pause/resume. It cannot undo a command
already sent: native actions must also target the observed document realm and
validate their current DOM immediately before mutation. A partially executed action
must never be classified as proven non-delivery.

`PageObserver` reads visible frame owners by exact context, validates document realms,
and combines transient DOM facts with method/status from the matching loaded
navigation. Pending navigation is held without guessing GET. A visible booking
still yields promptly to the person. A caller must use `observation` as the control
input; the accompanying raw recognition/snapshot is not an alternative authority.
The source-frame map allows recovery to identify the actual embedded error.

`ActionPermit` guards every write in a compound action. Core preparation and final
activation have separate reservations; acknowledged field preparation can resume,
whereas ambiguous delivery cannot be repeated. `prepare_selection` and
`prepare_applicant` take an explicit caller deadline for non-mutating readiness
checks. Copying authorization data is clicked once; its private values remain in
the browser. Service-specific applicant fields and maxlength are checked before
submission. These actions do not yet establish native background/lock or input
handler parity for all website forms.

Final province/procedure/office/applicant/request activation resolves a guarded element in its observed realm, reobserves
the page and sends one BiDi pointer sequence using the shared element reference.
It works with trusted-event-only test forms; ambiguous input is never retried.
This cannot undo an already published sequence and is not proof of locked-desktop
behaviour. Contact preparation uses guarded trusted keys for the phone and two email fields;
already matching fields are left unchanged. Copy/radio controls and general error recovery
still need their own trusted-input integration. Known no-slots exits provide a
read-only control-availability fact to the core and activate only its issued effect;
selection preparation does not re-fire change on an already selected option. All actions still require platform
background/lock and full runtime acceptance.

Passive response metadata is opt-in, limited to eight pending owned first-party
HTML/JS responses and 256 recent IDs, with query/session parameters removed.
Disabling it clears pending metadata. The native `passive` fixture proves bounded
BiDi response-body retrieval without refetch or DOM capture; the production
collector lifespan, body archiving and overflow cleanup are still integration work.

The flow supplies typed last/pending-action context through `set_actions`; it is
never inferred from page text. Integration with the action mapper and process/session
lifecycle is still pending. Existing core safety and
statistics are not replaced by this protocol layer.

```sh
cargo test -p ceh-browser --features bidi --locked
```

The opt-in CLI `browser_spike camoufox guards` tests the native mechanisms only on
an isolated fixture/profile. The `portal`, `applicant`, `contact`, `exit` and `passive` variants intercept all
requests with synthetic responses at the official origin; they keep the production
origin guard enabled and block external networking. It does not use the user's browsing data or
navigate the appointment website. Native authorization, visibility/lock behaviour,
production recovery and packaging have separate acceptance gates.

Authentication entry uses the known ICP button. Method selection is restricted to
the two exact Cl@ve provider origins; credential values never leave the page.
Read-only facts separate method controls from editable authentication and QR.
Certificate reminder publication is reported before waiting for the input response;
the core starts its five-second, pause-aware, single reminder only at that point.
These offline proofs do not establish actual native certificate-popup handling,
sound delivery or background/lock behaviour.

Cookies are a separate guarded pre-action on known automatic ICP stages, followed
by another observation. Manual office/booking pages and recovery/authentication
provider pages are excluded. A still-visible attempted banner is polled without
another click or proceeding with a form. A trusted cancellation receipt can clear a proven unpublished cookie/provider
intent after its epoch was revoked. It cannot clear Unknown/accepted delivery or
a newer intent; runtime must explicitly bind these receipts before UI integration.

Public office labels and explicit offered radio-label dates/times are bounded,
read-only reads. Hidden/disabled offers and time filters are excluded; identifiers,
form values, CAPTCHA and confirmation numbers are not exported. Booking reads
must happen after handoff, with a 750 ms command deadline. A failed read keeps the
generic alert and never justifies closing a manual browser. The caller associates
the selected public office with the exact session/document; enrichment, persistence
and notifications still need runtime binding.

Safe refresh supports applicant/result GET and guarded GET conversion of a
certificate-return POST. Availability POST reload is explicitly unsupported:
native standard BiDi reload did not publish the second request in the isolated
TLS probe. No ambiguous POST is retried. Default Salir has its separate passing
proof; this backend compatibility gate must be resolved before full parity.

Additional isolated modes: `auth`, `auth-methods`, `cookies`, `public-details`,
`refresh`; `refresh-tls` is a development-only local TLS/NSS probe. Its temporary
CA is trusted only in its own fresh browser profile, never the system Keychain.


`ResponseBody` accepts bounded UTF-8/base64 protocol bytes (128KiB), without
Debug/serialization of content. `read_response` requests disown on success; if
getData is rejected before its disown step, it issues a separate disownData without
retrying body retrieval. Transport failure is not retried. The native passive
fixture now uses this API and the existing sensitive-value-filtered ScriptArchive
for one static JS object, with exact HTML/JS bytes and manual page state preserved.
It does not persist manual HTML. Collector lifetime/overflow/ignored-body release,
HTML storage policy and runtime archive wiring remain unfinished.
See the [W3C network data commands](https://www.w3.org/TR/webdriver-bidi/#command-network-getData).
