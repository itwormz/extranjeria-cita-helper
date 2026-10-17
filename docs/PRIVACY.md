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

Appointment-site captures may contain NIE, names, phone/email and session
identifiers. Prefer a timeline line when reporting a problem. Inspect and redact
any files before sharing them. The application does not upload diagnostics to
GitHub or an issue service.

## Statistics and achievements

Per-profile statistics retain counters, search dates and public office labels locally. They do not upload telemetry. Manually confirmed achievements also store the appointment date, time, office, address and confirmation number locally.

Public PNG cards and prepared social posts exclude private appointment details and profile names. Calendar exports intentionally contain the appointment date/time and office/address, but exclude the confirmation number. Explicit Google Calendar actions transmit the same event fields to Google; importing an event into a cloud calendar can share those details with its provider. Social buttons open the selected service and prepare text, but never publish posts or upload images automatically.

## Updates and certificates

Automatic update checks can be disabled in Settings. Checks contact GitHub
Releases; package downloads use GitHub release assets and are verified against
the published SHA256 checksums. No applicant data is sent to GitHub. Downloading
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
