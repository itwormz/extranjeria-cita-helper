# Privacy and local data

SantaCita stores profiles, statistics, browser sessions and journals on your computer. It has no usage telemetry and does not automatically upload profiles, logs or captures.

## Data locations and migration

- macOS: `~/Library/Application Support/SantaCita/`.
- Windows installed app: `%LOCALAPPDATA%\SantaCita\`.
- Linux: `$XDG_DATA_HOME/SantaCita/` or `~/.local/share/SantaCita/`.
- Windows portable: `data` beside the program when `portable.flag` is present.
- Development: `CEH_DATA_DIR` can select a separate runtime, preferably under `.local/builds/`.

The first-run importer reads the installed Python application's `Cita Extranjeria Helper` folder and copies supported data into SantaCita's separate folder. The original remains in place. Repeated imports add missing profiles, statistics and achievements and back up changed destination files. Close Python before importing its statistics. Browser profiles, certificates and screenshots are not imported. An arbitrary old portable location is not searched automatically.

Authentication takes place on the official website in the selected installed browser. The application does not save authentication passwords or PINs. App-owned browser profiles retain session data and must be treated as private.

## Diagnostics and reports

Diagnostics are optional and local. Saved HTML, screenshots and browser/site resources may contain document numbers, names, contacts, cookies or session identifiers. Screenshots stop during manual control and pause. Review and redact any diagnostic material before sharing it; prefer a short timeline excerpt when reporting a problem.

Startup reports contain bounded technical error information and code locations, not full applicant profiles. **Copy report** copies to the clipboard and does not submit it. Inspect reports before posting.

Statistics and journals persist across searches. They contain counters, timing, page/action outcomes and recognized public office/appointment offers. The runtime also contains private profile fields and, for manually confirmed bookings, appointment details. Treat the entire data directory and SQLite sidecars as private, not as a public sharing format.

Public achievement images and prepared social posts contain statistics rather than private booking details or profile names. Calendar exports intentionally include appointment time and office/address, excluding the confirmation number. Opening Google Calendar can transmit these event fields to Google. Social actions prepare content and open a service; publication and attaching an image remain manual.

## Notifications

Telegram and webhooks are optional and configured per profile. Events can send the profile display name, province and action message; Telegram office/booking alerts can include recognized public office/address and offered dates/times. Avoid personal details in display names if they should not be shared. Applicant documents, contact fields, passwords, CAPTCHA and booking confirmation numbers are not notification payloads.

Tokens and webhook connection settings stay in macOS Keychain, Windows Credential Manager or Linux Secret Service. Profile YAML stores opaque references. A file copy between computers does not transfer vault secrets. Notification credentials are prepared before searching; if the vault is inaccessible, restore access or disable the affected channel. There is no plaintext-secret fallback.

Telegram pairing reads updates to match an expiring random code and requires approval of the recipient. It never deletes an existing bot webhook. Test actions explicitly contact the configured provider. Delivery success means provider acceptance, not that the recipient read the message.

## Updates and HTTPS

Update checks contact GitHub Releases, revealing ordinary network metadata to GitHub, but no applicant profiles. Both exact project repository names are allowed during migration. Downloading and installation require user action.

Packages must match the release checksum and signed Sigstore provenance for the exact repository, build workflow, tag, source commit and filename. Verification runs offline against the trust root embedded in the pinned library, without GitHub credentials or a mutable trust cache. Packages are checked again before installation; failed verification blocks installation.

The browser's HTTPS verification remains enabled. Public intermediate certificate preparation is restricted to the application's browser configuration; the application does not install root certificates into the system store.

## Repository hygiene

Only source, public resources, licences and synthetic/public verification fixtures belong in Git. Never commit runtime folders, real applicant data, vault values, browser profiles, diagnostic captures or build output. A privacy scan is useful evidence, not proof that no unknown secret exists.
