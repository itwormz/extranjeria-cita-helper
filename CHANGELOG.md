# Changelog

## 0.2.3 — Search options and booking details

- Separate real-browser offer extraction from deterministic handoff/history checks. Test successful enrichment and timeout fallback independently instead of requiring a loaded CI browser to beat the production read deadline.

- Keep simulated human input independent of the bounded details reader in the offline booking regression; wait for confirmed input delivery and eventual acknowledgement instead of assuming two manual ticks.

- Keep manual booking active when the bounded human-attention probe times out. Preserve the alert and retry the passive probe on the next tick instead of aborting the search.

- Match booking-radio names case-insensitively and keep the generic notification when no valid explicit offers are recognized. Strengthen the handoff regression so swallowed callback assertions cannot hide a failure.

- Preserve the selected office/address and explicitly offered appointment dates/times on the original detected-booking event in local statistics, independently of diagnostics. Enrichment keeps the detection timestamp and counters unchanged.
- Show these details in booking marker hover and selection cards, and in the user journal and session timeline log. Each office stays associated with its own offers; old history without details remains unchanged. CAPTCHA, hidden identifiers, applicant data and confirmation numbers are excluded.

- Refresh the timings screenshot from the current profile editor while keeping the gallery’s shared dimensions.
- Redraw the English, Russian and Spanish light/dark workflow diagrams with a straight main path, blue option-labelled return arrows, a shared Salir/Aceptar restart and manual booking ownership. Both search choices are retained as typed session context.
- Keep applicant-warning fill/submission and confirmed transitions accurate in the user journal when continue mode is enabled.
- Configure two independent profile choices: wait on the applicant form while it reports no appointments, or fill/copy applicant data and continue; separately follow Salir or reload an ordinary result. Existing profiles keep their saved result mode and default to waiting on the applicant form. Intervals, recovery cooldowns and ambiguous-POST safeguards remain in force.
- Telegram appointment alerts can include the current office/address and explicit visible appointment date/time offers from the known ICP radio layout. Time filters, applicant data, CAPTCHA, hidden identifiers and confirmation numbers are excluded. Unsupported layouts and failed reads fall back to the general notification; webhook payloads remain unchanged.
- Hand control and sound alerts to the user before the bounded passive read, without scrolling, focusing or changing booking fields. Recognize the official rdbCita radio control regardless of letter case.

## 0.2.2 — Search modes and automation statistics

### Search and profiles

- Default ordinary no-appointment checks to the website’s real **Salir** control after the configured pause, following recognized pages including the request menu at `acOpcDirect`. Profiles can still select result reloads; explicitly saved choices are preserved. A missing or ambiguous Salir button requires review instead of an automatic fallback.
- If the applicant form already reports no appointments, wait and reload that form without entering or copying identity data. Resume filling only after the warning disappears. No daily document-input threshold is assumed, and a reduction in submissions or blocking risk is not claimed.
- Never replay an applicant-form POST. Reload confirmed GET forms; after the acEntrada authentication return, refresh the recognized no-appointments form with a separate GET and the configured pause, without filling identity fields or requesting manual attention. Other POST and unknown origins still require review.
- Recover entry-page Bad Gateway/server failures after the configured pause using a fresh GET, preserving protection against ambiguous contact submissions.
- Keep evidence translatable when switching languages without a new bot state. Destroy notification-test windows between cases to prevent accumulating Qt objects and Intel CI timeouts.
- Remember the last working profile and initially show its statistics. The statistics selector remains independent while exploring other profiles.
- Keep startup usable if saving the last-profile preference fails, exclude certificate-attention pages from automated effort, localize applicant evidence, and tolerate malformed script/base URLs in diagnostics.
- Read notification credentials only when opening the Notifications tab. Saving other profile fields preserves notification settings and secret references without accessing the system credential store.

### Statistics and timeline

- Add confirmed automated actions and unique recognized pages to Today/All time statistics, selected-period summaries, achievements, posts and PNG cards. Focus, individual keystrokes, failed actions, repeated recognition and manual work are excluded. Historic unmeasured counters stay unknown; the tracking-start date is available in the metric tooltip.
- Place both effort counters in a compact Automation section with distinct icons below availability and site-error statistics.
- Keep Cod. Oper. periods through intermediate navigation until a normal availability result is observed, retaining errors, manual work and observation gaps separately.
- Remove the dotted office-count underline; hover previews no longer capture focus and close when the pointer leaves the count and preview. Click/keyboard activation retains an interactive popup.
- Clear transient statistics read errors after a successful refresh and log only the failing component and exception type, without profile values or raw error text.
- Remove automatic first/last-clip selection. Cleared selection stays cleared through refresh and zoom; Escape clears a selection, and an explicitly selected valid clip remains selected.
- Record applicant-form submissions separately from availability checks without storing submitted values, and retain the selected repeat mode in typed session context.

### Diagnostics, updates and maintenance

- Passively archive already-loaded first-party ICP JavaScript when diagnostics are enabled. Gzip objects are deduplicated by content hash, with sanitized session manifests, exact-reference links from snapshots, bounded capture and reference-aware retention. External authentication scripts are excluded; capture makes no extra network requests and does not touch manual browser control.
- Accept numbered local development versions in native metadata and stable-update comparisons, while keeping development releases outside stable publication.
- Update Sigstore to 4.5.1, fonttools to 4.67.0 and tzdata to 2026.5. Sigstore no longer brings pyOpenSSL or pyasn1 into the locked dependency set.
- Run explicit Python and GitHub Actions CodeQL analyses for Dependabot PRs. PRs run lightweight tests; full native packages run manually or for release tags. Pin updated checkout/setup-uv Actions and temporarily exclude Playwright versions incompatible with Camoufox.
- Make the copy-from-authentication browser regression wait for the rendered fixture before a single processing step, with explicit delayed-render coverage. Production action timeouts and POST replay protection are unchanged.

## 0.2.1 — Search history and security fixes

- Added persistent per-profile SQLite statistics, independent of page diagnostics: Today/All time counters, cumulative active search, office appearances and dates, appointment-selection screens, site errors and repeated passes.
- Existing JSON totals migrate once with the original and backup preserved. Undated totals remain undated; incomplete history is marked rather than converted into invented daily observations. Duplicate event replays no longer add activity days.
- Added a compact timeline of continuous search, Cod. Oper. waits, site failures, session anomalies, manual control and observation gaps. Raised building/calendar/reset markers show availability and browser resets; office-to-date handoff stays one availability episode.
- Hover highlights complete clips, events and empty periods. Drag selects whole clips; additive selection reports union duration and recorded counters without duplicated events. Historical reconstruction remains labelled; period/profile changes and daylight-saving boundaries preserve valid selections.
- Corrected grouped-office selection bounds to fit the counter text. HTTP 429 recognition no longer depends on a body-text branch skipped for rate-limit responses. Sleep/resume records the missing interval from the last worker tick, without extending the previous page through suspended time. Manual form revisits preserve manual state and avoid screenshots.
- Updated certifi, cryptography/PyOpenSSL, Pillow, fonttools and setuptools to patched locked versions. TLS certificate discovery explicitly requires TLS 1.2 or newer; complete certificate verification remains mandatory before caching. Cleaned up URL matching in updater test fixtures flagged by CodeQL.
- Updated contributor guidance, issue forms and all three READMEs for both supported services and their document fields. Calendar preview, Google Calendar and ICS now use the accurate shared title “Extranjería — Cita”; saved appointment data is unchanged. Windows package details mention both services.
- Made the manual-office browser regression separate fixture navigation from the unchanged five-second production handoff bound, avoiding false failures on slow Intel runners.
- Source-built cryptography links OpenSSL statically, preventing a macOS Intel bundle conflict with Python’s older OpenSSL libraries.
- Windows packaging validates the runner-provided .NET 8 SDK/runtime instead of re-downloading them through an unavailable Microsoft redirect; pinned WiX and package checks remain unchanged.
- Appointment selection, CAPTCHA and confirmation remain manual. Statistics stay local, with no automatic upload or diagnostic-archive importer shipped to users.

## 0.2.0 — Autorización de regreso

- Added **Autorización de regreso** alongside Toma de huellas / TIE, selected per profile and shown on the Monitor screen.
- Regreso profiles support NIE, DNI and passport, manual applicant entry or copying authenticated data; citizenship remains specific to fingerprinting profiles.
- Kept separate document drafts when switching services or document types, with document validation and protection against silent passport truncation.
- Corrected keyboard Tab order after switching services; applicant fields and timing values follow their visual order.
- Reduced the default first blocked-session pause to 30 seconds, followed by 60, 120, 300 and 600 seconds for repeated blocks. Existing profiles using the former default schedule migrate automatically; custom schedules are preserved. HTTP 429 keeps its separate 3-minute pause.
- Appointment selection, CAPTCHA and final confirmation remain manual; Camoufox stays open and under user control.

## 0.1.5 — Attested updates

- GitHub tag builds automatically generate Sigstore build provenance for final DMG, MSI, AppImage and portable ZIP packages. Publication verifies every signature, package digest, repository, workflow, tag, hosted runner and source commit before uploading a draft. All Actions are pinned to commit SHAs.
- The desktop updater requires immutable releases and signed build bundles, compares provenance against the tag commit, and rechecks it before installer launch. Missing or invalid proof blocks installation; no GitHub CLI, account, private signing key or manual release approval is required.
- Verification uses only production trust roots shipped in the locked Sigstore dependency, not the mutable user TUF cache. Trust-root changes require a verifier/application update. Packaged smoke tests exercise a real public signature offline.
- About highlights failed provenance verification with a red shield warning and blocks installation. Long update messages remain fully visible in both themes.
- All three README headers show the latest published release dynamically, with download/change links and prominent language selectors. Local metadata checks no longer require a source-version number in README.
- Existing installations retain their previous checksum-only updater until upgraded. Provenance establishes build origin; it is not a guarantee that source code or dependencies are safe.
- Offline expiry-recovery tests wait for rendered fixture text and controls before recognition, avoiding a cold-start Firefox loading race on Windows CI; delayed rendering is covered explicitly.

## 0.1.4 — Profile notifications

- Optional Telegram and HTTPS webhook notifications per profile, with independent authentication, office, appointment and terminal-error events.
- Compact notification settings reuse the profile fields and theme, with themed request-method/body selectors, collapsible event choices and fixed-size inline test results; one-time recipient pairing supports private chats, groups and manual Chat ID entry. Existing Telegram webhooks are preserved.
- GET or POST, custom headers and URL parameters, standard/custom JSON, masked values and test delivery. Tokens and webhook configuration use native system credential storage; profile files contain references only.
- Background delivery with finite retries and stale-event cancellation. No appointment forms, authentication secrets, page captures or confirmation numbers are included. Profile display name and province are included intentionally.

- Added a local pre-push gate for workflow lint, release metadata, privacy, unit and offline browser tests, with an optional native bundle smoke check.
- Windows tag builds include the optional portable ZIP alongside MSI. Offline manual-office tests allow normal page-recognition latency while separately bounding the handoff regression.

## 0.1.3 — Procedure selection across provinces

### Fixed

- Select the configured fingerprint procedure across all visible service groups instead of assuming it is in the first group. Provinces such as Bizkaia separate Extranjería and Policía Nacional services; the office selector remains unchanged.
- Recognize procedure forms even when the first service group is hidden. An absent procedure does not trigger Aceptar or select an office with a similar name.

### Verification

- Audited the public procedure forms for all 52 provinces: 46 place the fingerprint service in the second group, six in the first.
- Before release, verified selection and a simulated Aceptar transition offline in Camoufox for all 52 captured field sets. Added browser regressions to CI for multiple groups, hidden groups, and a missing service. This does not assert live end-to-end booking verification for every province.

## 0.1.2 — Bundled browser startup on clean installations

### Fixed

- Bundled Camoufox reads its Firefox version from its own application metadata, including macOS Contents/Resources, instead of falling back to a user cache. This fixes CamoufoxNotInstalled on clean installations where the bundled browser exists.

## 0.1.1 — Startup diagnostics and copyable error reports

### Added

- Tag pushes automatically publish the GitHub Release after all platform checks/builds and package uploads succeed. Release notes include the matching changelog entry; incomplete uploads remain drafts.

- Automatic local technical reports for browser startup failures, even with page diagnostics disabled: application/OS versions, architecture, executable paths, exception type and sanitized stack frame locations.
- A compact error panel with **Copy report**, a copied confirmation and a retry action in English, Russian and Spanish. Copying preserves the complete structured report; sending it remains a manual action.
- macOS checks comparing the browser paths in the main app and worker, resource symlink targets and resolution, exact filesystem error codes, executable permissions, quarantine presence and App Translocation detection.
- Bounded read-only signature integrity checks for the main app and Camoufox. Results distinguish valid, invalid, timed-out and unavailable checks; signature validity does not assert notarization or Gatekeeper approval.

### Fixed

- macOS bundle tests construct relative symlink targets with native path separators, allowing the same relocation and worker-resource checks to pass on Windows CI.

- Browser startup crashes reported as `TargetClosedError` are no longer mistaken for normal user closure.
- Access denial is distinguished from a missing bundled browser in the error panel.
- Unexpected worker exits retain exception types and missing-module names instead of silently discarding the failure reason.
- The GUI crash fallback skips external diagnostic commands and explicitly marks them as not performed, keeping the interface responsive.

### Privacy and verification

- Reports do not collect profile settings, cookies or page contents. Known applicant values and home paths are masked; stack source lines and arbitrary worker stderr are excluded. Reports are stored locally with a ten-report retention limit and are never uploaded automatically.
- Added regressions for disabled diagnostics, missing files, access denial, broken worker links, bundle relocation, signature timeouts, report copying and worker crashes.
- The originally reported macOS startup failure has not been reproduced. This release improves diagnosis and error handling; it does not claim a confirmed fix for that user's installation.

## 0.1.0 — Initial release

First standalone desktop version of Cita Extranjeria Helper. It assists with **Toma de huellas** appointments for TIE on the official ICP Plus website. Other procedures are outside the supported scope.

### Desktop application

- Marking an appointment as obtained stops the search while preserving Camoufox for viewing or managing the booking.
- Manual office submission hands processing to the contact screen without waiting for the previous office control; its attention alert stops when the page changes.
- Compact profile editor with field icons, inline prompts and labels that remain visible after entry. Authentication data mode hides NIE/name inputs while preserving the manual draft.
- Separate control window with a bundled Camoufox browser; profile selection on Monitor and profile creation/editing in Settings.
- Compact Monitor layout with a profile selector above the status and compact controls below it, fixed rows for the recognized page, bot status and next action, plus a separate countdown column. User participation is highlighted when needed; counters sit below the status block. The timeline keeps four columns, icon-only copying, and selection clearing through empty-space clicks or Escape.
- Timing settings use compact labeled fields in two columns, including retry sequences, with the existing saved values and defaults preserved.
- Start, Pause/Resume, Show Camoufox and session actions. Reset the selected browser profile before starting, or restart it during a run without deleting applicant settings.
- Once date/time booking controls are recognized, automation stays disabled through subsequent manual navigation and errors until an explicit session restart. The panel shows control handed over; screenshots and automatic recovery remain disabled.
- Closing the application ends the worker and closes its browser session. Manual booking screens remain available while the application hands control to the user.
- Application icon with a static status dot: green while searching (including timers and recovery), gray when paused, amber for user participation, and red for worker failure or lost connection. No dot before launch or after a normal stop; update-check errors do not change the search indicator.
- One browser-check chime when the transition after selecting certificate authentication takes more than five seconds, cancelled on completion and deduplicated against a detected approval prompt. This reminder does not assert that a certificate dialog is open.
- English, Russian and Spanish; initial language follows the system locale, falling back to English. Light/dark themes and a toolbar sound toggle; preferences persist locally.
- Compact About with the application icon, GitHub-first links, an update status row and local-data note. Neutral Russian and Spanish interface instructions without mixed forms of address.
- About with version information, manual and optional automatic latest-release checks, verified package downloads and explicit installer launch, localized README, repository and issue links.
- Hidden appointment-catching game, opened by five clicks on the version, with a visual celebration and localized proverbs.

### Profile statistics and achievements

- Appointment notes suggest the profile’s latest observed office when no current selection is available; saved appointments keep their own values. Date/time selections use a contrasting highlight, and saved office/address and confirmation values support text selection and copying.
- Clean profile statistics with paired metric rows, explicit time to first office/appointment selection, and an explanatory action to mark an obtained appointment. Achievements become visible after the first saved appointment.
- Independent Statistics tab with persistent per-profile counters for checks, office appearances and distinct offices, booking screens, server errors, HTTP 429, blocks, passes from the beginning, active search time and first availability. Repeated observations of the same document are deduplicated.
- Manually confirmed appointments with locally stored date, time, office, address and confirmation number; private details can be corrected without changing the achievement statistics.
- Compact appointment note with validated, automatically formatted date/time, a themed popup calendar and separate office/address fields populated from profile office history.
- Calendar choice: open a prefilled Google Calendar event or save an ICS and explicitly open it in the local calendar application. Both exports omit the confirmation number; ICS preserves a stable event identity and mainland/Canary timezone support. Public PNG cards and editable social-post text contain statistics only; Telegram, X, Facebook and Instagram links do not publish automatically.
- Localized sharing dialog with Text/Card modes, complete statistics posts, local PNG preparation and one Prepare and open action. Social posting remains manual; public captions contain the application name without a repository link.
- Compact achievement card, save/copy PNG actions and optional author support through GitHub Sponsors or NOWPayments.

### Profiles and appointment flow

- Automatic restarts default to a 30–60 second cooldown. Longer recovery waits remain in force; completed cooldowns are not duplicated and explicit user restarts stay immediate.
- Recognize @Firma certificate-validation service failures and wait before retrying authentication. Server-error backoff survives a return through province/service/sign-in and resets only after actual progress past authentication.
- Empty loading documents have a bounded wait followed by a delayed new pass without replaying uncertain form submissions. Native browser load-error pages use recovery; manual attention alerts repeat.
- Certificate login alerts only after a detected native approval prompt or five seconds waiting after selecting certificate authentication. Choosing the authentication method itself does not start the reminder.
- Optional certificate-cache network preparation failures no longer prevent Camoufox from opening; HTTPS validation remains enabled.
- Separate applicant profiles with province selection, contact details and either entered NIE/name or values copied from the authenticated page.
- Authentication-data mode uses the official Copy data button, waits for populated NIE/name before submitting, and supports read-only copied fields.
- Three authentication choices: electronic certificate, Cl@ve Móvil and Cl@ve Permanente. The assistant selects the configured method and waits for interactive authentication when required.
- Fill applicant details even when the form includes a no-appointments warning; check the result after requesting an appointment.
- Refresh recognized no-appointments results using profile timings. Follow Aceptar/Salir for results that require another pass instead of repeatedly replaying the previous form submission.
- Select and continue automatically when exactly one valid office is available; wait for user choice when several are offered. Fill contact details after office selection.
- Repeated sound alerts when manual participation is needed. Appointment date/time and final booking confirmation remain manual.
- Per-profile action/typing timings, check intervals, retry pauses, alert intervals and recovery jitter. Check counts include both the current session and a persistent total for the selected profile.

### Recovery and visibility

- Recognize expired-session messages, including messages embedded in an otherwise expected page, and prioritize their own recovery controls.
- Timed recovery for recognized server errors, request limits and blocks. HTTP 429 preserves the session and retries after a cooldown; recognized blocking can reset the browser session.
- Restart an ordinary flow in the existing browser window where possible; distinguish a new pass from a browser-profile reset in the timeline.
- Retry action timeouts and wait for late navigation after an already-sent click without submitting the action twice. Province/procedure buttons that time out before pointer dispatch can recover through trusted Enter activation without raising the browser or changing its configuration.
- Require a recognized outcome before recording a transition as completed; unfamiliar screens can require manual intervention.
- Explicit Show Camoufox requests restore the owned Windows browser window, including minimized windows, with foreground permission passed from the control panel to its worker.
- A server error after Aceptar ends a no-appointments pass is recovered with a delayed entry-page GET instead of replaying the exit POST.
- On a locked macOS desktop, use browser keyboard activation for standard controls instead of compositor-dependent mouse dispatch.
- Remove completed flow listeners so restarts do not multiply timeline events and appointment-check counts.

### Logs, diagnostics and local data

- Page-visit timeline with action, status and confirmed outcome, distinct event colors, compact page addresses and copying selected rows or the complete tab through a button, context menu or Cmd/Ctrl+C.
- Separate office-availability history with detection time and available offices, without repeating one observation as several display rows.
- Optional page diagnostics, **off by default**, stored in a separate directory for each recorded session. Navigation responses, changing documents, nested site screens and popups can be captured.
- Suspend screenshot rendering during manual control, pause and macOS screen lock while preserving HTML/navigation diagnostics.
- The action timeline persists independently of page captures. External authentication pages are excluded from HTML/screenshots; only limited metadata is retained.
- Profiles, preferences, browser sessions, counters and diagnostics stay in local application data. Private files, build output and preparation notes are excluded from the repository.
- Discover and validate the site's current TLS chain, cache verified intermediates and import them into the selected browser profile without disabling HTTPS checks or adding root certificates.

### Packaging and project

- Native application bundles for macOS, Windows and Linux, with an included browser and reduced runtime resources. Legacy Windows font bundles are filtered by Camoufox’s configured Windows faces, preserving matching collections and unknown files.
- Optional Windows portable ZIP uses an adjacent data directory; MSI remains the default release format.
- Native release formats: **macOS DMG, Windows MSI and Linux AppImage**, with SHA256 sidecars. Windows installs for the current user, creates a Start Menu shortcut and supports upgrades while preserving application data.
- GitHub Actions configuration for source/privacy tests, isolated browser regressions, native builds, packaged GUI/browser/worker smoke checks, archives and draft releases.
- Russian, English and Spanish README, a clickable gallery of equal-size Monitor, Profile, Timings and Settings screenshots with fictitious data, localized workflow diagrams, source-run instructions and support links.
- Project code licensed under **PolyForm Noncommercial 1.0.0**; third-party components retain their own licenses.

### Limitations

- On macOS, another application running full-screen in a separate Space can coincide with stalled Camoufox clicks and text entry, action timeouts and repeated passes. Use regular windows with Camoufox open in the same Space as a workaround; the underlying cause remains unconfirmed.
- Browser automation works best with the window open behind other windows; minimizing it can interfere with actions. Locked-desktop keyboard handling is currently implemented for macOS.
- Authentication may require manual certificate approval or Cl@ve confirmation. Unknown site screens may require user intervention.
- macOS uses an ad-hoc signature without notarization; Windows packages do not have an Authenticode publisher signature.
- Website availability, an appointment and uninterrupted operation are not guaranteed.
