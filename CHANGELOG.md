# Changelog

## 0.1.1 — Startup error reports

- Browser startup failures now save a small local technical report even when page diagnostics are disabled. Reports include application/OS versions, architecture, bundled-browser availability and a sanitized exception traceback, without collecting profile settings, cookies or page contents.
- The error panel shows a concise explanation, a Copy report button with confirmation, and a retry action in English, Russian and Spanish. Reports are copied only on request; nothing is sent automatically.
- A browser crash during startup is no longer classified as a normal user closure; its failure report is preserved.
- Unexpected worker exits extract exception types and missing-module names from bounded stderr without saving arbitrary stderr text. Only the latest ten startup reports are retained.
- Added regressions for browser startup failure with diagnostics disabled, report sanitizing/copying, worker crashes and macOS bundle relocation. The reported macOS startup failure has not yet been reproduced; this update improves diagnosis rather than claiming a confirmed fix.

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
