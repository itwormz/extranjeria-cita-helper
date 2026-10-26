# Privacy and local data

Profiles, settings, browser sessions, appointment-check counters and the action
timeline are stored on your computer. The assistant has no usage telemetry and
does not automatically upload profiles, logs or page captures.

Packaged applications keep these files separately from the installation:

- macOS: `~/Library/Application Support/Cita Extranjeria Helper/`
- Windows: `%LOCALAPPDATA%\Cita Extranjeria Helper\`
- Linux: `$XDG_DATA_HOME/Cita Extranjeria Helper/`, or
  `~/.local/share/Cita Extranjeria Helper/` when that variable is unset.

Authentication takes place on the official website in Camoufox. The application
does not intercept or store passwords, PINs or authentication confirmation data.
Browser profiles can retain cookies and other session data; treat them as private.

## Diagnostics and sharing

Page diagnostics are **off by default**. When enabled, they save local HTML,
screenshots and error details in a separate folder for each recorded session.
During manual control, pause and macOS screen lock, only HTML and navigation responses are captured;
screenshots are suspended so capture does not interfere with the browser.
The action timeline is recorded independently. External authentication pages are
excluded from HTML and screenshot captures; only limited navigation metadata is
retained.

Appointment-site captures may contain NIE, DNI or passport numbers, names,
phone/email and session identifiers. Prefer a timeline line when reporting a problem. Inspect and redact
any files before sharing them. The application does not upload diagnostics to
GitHub or an issue service.

Startup failures also save a small technical report in `startup-errors/` inside
the application data directory, independently of page diagnostics. Reports contain
application/OS versions, architecture, browser executable availability and a sanitized
exception with stack frame locations (without source lines). For unexpected worker
exits, up to 32 KiB of stderr is inspected in memory only to extract the exception
type and a missing Python module name; arbitrary stderr text is not saved. They do not collect profile settings, cookies or page
contents. Known applicant values and home-directory paths are masked. Only the
latest ten reports are retained. On macOS, reports also contain sanitized bundle
paths and resource-link targets, filesystem error codes, quarantine presence
(without its metadata), and bounded read-only code-signature verification results.
Signature integrity does not imply notarization or Gatekeeper approval. If the
worker exits before it can report an error, the GUI fallback skips external
commands and marks those checks as not performed to keep the interface responsive. **Copy report** copies the report to the clipboard;
it does not send it anywhere. Review the text before sharing it.

## Statistics and achievements

Per-profile statistics are collected locally in `data/profile-statistics.sqlite3` even when page diagnostics are disabled. They retain session/version/platform context, public province/service/authentication categories, typed page/action/error observations, active-time intervals and public office appearances. Document identities and unknown-page structure fingerprints are hashed; arbitrary page text, URLs, form values, NIE/DNI/passport, contacts, passwords, tokens, CAPTCHA, operation codes and appointment confirmation numbers are not included in this database. Observation gaps remain explicit. The database and any SQLite sidecar files are private local data; they are not a public sharing format.

Existing JSON totals migrate transactionally once. The original remains unchanged and `profile-statistics.json.bak` preserves a backup. Undated historical totals do not imply exact daily or page history. Statistics are not uploaded automatically; diagnostic archive conversion is not a product feature. Local history is currently retained without automatic eviction. Manually confirmed achievements separately store the appointment date, time, office, address and confirmation number locally.

Public PNG cards and prepared social posts exclude private appointment details and profile names. Calendar exports intentionally contain the appointment date/time and office/address, but exclude the confirmation number. Explicit Google Calendar actions transmit the same event fields to Google; importing an event into a cloud calendar can share those details with its provider. Social buttons open the selected service and prepare text, but never publish posts or upload images automatically.

## Optional notifications

Telegram and outgoing webhooks are off by default and configured per profile. When enabled, selected events send the profile display name, province, event time and a short action message to the configured recipients. Avoid personal details in profile names if they should not be shared. Applicant documents, contacts, passwords, PINs, page captures and appointment confirmation numbers are not sent.

Bot tokens and the complete webhook configuration (URL, headers, parameters and body template) are stored in the native credential service: macOS Keychain, Windows Credential Manager or Linux Secret Service. Profile YAML contains opaque references; copying it does not transfer secrets. An unavailable credential service causes an explicit error, without a plaintext fallback. Header values are excluded from diagnostics even when visually unmasked. All header values are visually masked by default; the lock affects display only. Custom webhook fields are explicitly transmitted to the chosen endpoint.

Telegram pairing temporarily reads bot updates to match an expiring, random code. Only the matching chat is shown for explicit approval. The application never deletes an existing Telegram webhook. A pairing link grants an opportunity to request connection, not access to the bot token or browser session. Test buttons intentionally contact the selected provider before settings are saved. Delivery is best effort; success means the provider accepted the request, not that the recipient read it or heard a sound.

## Updates and certificates

Automatic update checks can be disabled in Settings. Checks contact GitHub
Releases; package downloads use GitHub release assets and are verified against
the published SHA256 checksums and signed build provenance. The updater also reads the public tag commit from GitHub. Sigstore verification is offline against roots shipped with the application; no signing credentials, GitHub token or account are required. No applicant data is sent to GitHub. Downloading
and installation require a button press; installation ends the active browser
session and preserves profiles and settings.

At browser startup, the application checks the appointment site's TLS chain.
Verified intermediate certificates are cached locally and imported only into the
selected Camoufox profile. It does not install root certificates, change the
system certificate store or disable browser HTTPS verification.

## Public repository

The repository contains application code, synthetic tests, public dictionaries,
build scripts and documentation. Personal profiles, credentials, certificates,
browser profiles, captures, logs and build output are excluded. Build scripts
bundle explicit public resources rather than the project directory.

Automated effort counters retain only typed completion events and hashed document/event identities: unique recognized pages and completed activations, option selections, field fills or reloads. They do not store selectors, field contents or individual keystrokes, and do not count manual work. A local tracking-start timestamp distinguishes newly collected metrics from unmeasured historical activity.

When diagnostics are enabled, already-loaded JavaScript responses under the official `/icpplus/` and `/icpplustieb/` contexts can be saved privately. Identical bytes share a gzip object in `debug/resources/`; session `resources.json` manifests and snapshot resource references identify the captured versions. Query strings, fragments and path session parameters are omitted from saved resource URLs; exact request references are hashed. Authentication-domain scripts, cookies, authorization headers and request bodies are not archived by this mechanism. Only status and JavaScript content type are recorded from response metadata. It makes no additional requests. Individual scripts are limited to 2 MiB, with bounded background capture and a response-body timeout; skipped or not-yet-captured resources are marked missing. Retention removes unreferenced script objects after old sessions are removed. JavaScript and saved HTML remain private diagnostic material and must be reviewed before sharing.
