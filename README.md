# SantaCita · beta

🇬🇧 **English** · 🇷🇺 [Русский](README.ru.md) · 🇪🇸 [Español](README.es.md)

[![Latest published release](https://img.shields.io/github/v/release/itwormz/extranjeria-cita-helper?include_prereleases&sort=semver&label=release&color=626bdf&logo=github)](https://github.com/itwormz/extranjeria-cita-helper/releases)

An assistant for **Toma de huellas / TIE** and **Autorización de regreso** appointments on ICP Plus. A new application built with Rust, Tauri and Svelte.

SantaCita is released as a beta. The stable 0.3.x line is the previous Python application. The repository is still named `extranjeria-cita-helper`.

## 🚀 Quick start

1. Open [Releases](https://github.com/itwormz/extranjeria-cita-helper/releases) and choose a published SantaCita **Pre-release**.
2. Install the app and select an installed browser: Chrome, Chromium, Brave, Edge or Firefox. A browser is not bundled.
3. Create a profile with the province, service, authentication and applicant details. Start the search and sign in through the browser.
4. When notified, choose an office or complete the booking. **Date, time, CAPTCHA and final confirmation remain manual.**

| System | File |
| --- | --- |
| macOS Apple Silicon | `SantaCita-<version>-macOS-Apple-Silicon.dmg` |
| macOS Intel | `SantaCita-<version>-macOS-Intel.dmg` |
| Windows x64 | `SantaCita-<version>-windows-x64.msi` or `-portable.zip` |
| Linux x64 | `SantaCita-<version>-x86_64.AppImage` |

macOS: drag SantaCita into Applications. The app is not notarized; if blocked, use System Settings → Privacy & Security → Open Anyway. Windows: packages have no publisher signature; SmartScreen may require More info → Run anyway. Linux: make the AppImage executable (`chmod +x file.AppImage`).

## 🔄 Search and controls

- Wait and refresh according to your settings while the applicant form reports no appointments.
- When the message disappears, start a new pass from sign-in and recheck availability.
- Automatically select one available office; ask you to choose when there are several.
- Pause, resume and continue searching after date selection from the app. Profile settings and timings apply during the search.
- Statistics and the journal persist between runs. A confirmed booking can be added to a calendar and its search statistics shared.

## 🔔 Notifications

Profiles support sound, Telegram and webhooks. Set up Telegram with a bot token, recipients and events. A webhook needs an HTTPS address and events; headers and a template are optional.

Secrets stay in the system credential store. macOS may ask for the Keychain password to access the Telegram token.

## 🗂️ Importing data

On first launch, SantaCita looks for the installed Python app and copies its profiles, statistics, achievements and journals. The original data stays in place; SantaCita uses a separate `SantaCita` data folder. Importing again from Settings adds missing data without replacing your current profiles.

Close the Python app before importing. Browser authentication must be completed again. An old portable folder is not discovered automatically: keep its `data` folder and use a separate copy for migration, preserving the original.

**Windows portable:** extract the complete ZIP. `portable.flag` keeps data in the adjacent `data` folder. To bring old portable data over, copy the old `data` folder into the new portable folder before first launch and keep the original.

## ⬆️ Updates

Check in About, or automatically at startup and once a day. **Offer beta versions** includes stable versions and betas; it is off by default and imported from Python 0.3.8.

Download and installation require your action. SHA256 and Sigstore build provenance are verified, and checked again before installation. Failed verification blocks installation. Windows portable replaces the program and restarts while preserving `data`.

## ⚠️ Beta limitations

- An installed supported browser is required. Configured notifications need access to the system credential store.
- Windows uses WebView2. If missing, install [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/).
- Linux packages are built on Ubuntu 22.04 and require a compatible graphical environment and system libraries.
- Website errors, HTTP 429 restrictions and reauthentication can occur. An appointment is not guaranteed.
- [Diagnostics and privacy](docs/PRIVACY.md). Remove personal data before sharing reports.

## 🎯 Purpose

An independent project for obtaining your own appointment, not affiliated with Spanish authorities. Not intended for appointment resale or mass capture.

Provided as is, without a guarantee of operation or an appointment. Users are responsible for following the law and the service rules.

## 📣 A message to the responsible authorities

Access to a mandatory administrative procedure should not depend on a person's ability to refresh a page for weeks, guess when appointments become available or resort to paid intermediaries.

In my view, a system that requires foreign nationals to do this undermines their dignity and creates a practical barrier to public services. A transparent queue, clear waiting times, availability notifications and an accessible alternative to online booking are needed.

Measures against automation can also hinder ordinary users. Service protection should account for false positives and provide a clear process for restoring access. A person trying to complete a mandatory administrative procedure should not have to understand the technical workings of anti-bot protection.

This project exists because the ordinary booking process requires disproportionate effort. I hope improvements to the official service will make assistants like this unnecessary.

### The shadow market and its public cost

An opaque booking process and difficulties with ordinary access create favourable conditions for a shadow market in which access to a free public service becomes a commodity. Spain's Ministry of the Interior has reported investigations into schemes using automation to capture appointments in bulk for resale. Such schemes further hinder people seeking their own appointment.

The consequences extend beyond inconvenience for foreign nationals: combating appointment hoarding and resale requires police investigations, technical expert analysis and judicial involvement. In my view, an accessible and transparent allocation system should address the conditions that enable this market alongside enforcement against abuse.

Public appointments should be allocated according to clear and fair rules, rather than becoming a competition in technical capabilities or applicants' ability to pay.

Official examples of investigations:

- **12 May 2023:** the Ministry reported 69 arrests and a further 25 people under investigation on suspicion of involvement in a criminal organization and computer damage. The investigation involved Investigating Court No. 8 in Valencia; according to police, appointments were captured for resale at €30–200. [Ministry statement](https://www.interior.gob.es/opencms/es/detalle/articulo/La-Policia-Nacional-desmantela-un-entramado-que-bloqueaba-el-sistema-de-citas-online-de-extranjeria-mediante-un-bot-informatico/).
- **27 November 2024:** the Ministry reported 21 arrests in Alicante, Murcia and Valencia in an investigation involving appointment sales, blocking the system with bots and document forgery. [Ministry statement](https://www.interior.gob.es/opencms/es/detalle/articulo/La-Policia-Nacional-desarticula-un-grupo-criminal-dedicado-a-la-venta-fraudulenta-de-citas-de-extranjeria-mediante-bots/).

These are reports of investigations and arrests, not confirmation of final criminal convictions.

### Your rights and where to seek help

- **Legal basis:** Articles 12 and 13 of [Ley 39/2015](https://www.boe.es/eli/es/l/2015/10/01/39) provide for assistance with electronic services and respectful treatment that facilitates exercising rights and fulfilling obligations. These provisions do not authorize automation.
- **Technical errors and blocks:** the [official appointment support page](https://sede.administracionespublicas.gob.es/pagina/index/directorio/ayuda_cita_previa) lists telephone 060 and a technical enquiry form. This channel handles technical issues.
- **Complaints about access to public services:** complaints to the Defensor del Pueblo are free, available to foreign nationals regardless of their legal status and do not require a lawyer. [Who can complain and how](https://www.defensordelpueblo.es/el-defensor/como-te-podemos-ayudar/) · [Submit a complaint](https://portal-sac.defensordelpueblo.es/es/form/queja). Complaints do not suspend administrative deadlines.
- **Official recommendations:** the Defensor del Pueblo has already [called for improved access to appointments](https://www.defensordelpueblo.es/resoluciones/dificultades-para-concertar-cita-previa-para-solicitar-asilo/) and timely booking.

If you face the same problem, record the dates of attempts, error messages and support enquiries. Describe your own experience accurately and submit it through official channels. Documented complaints help show the scale of the problem. Do not publish NIE, DNI or passport numbers, certificates, cookies or other personal data.

## 🧰 Development

Rust, Tauri 2, Svelte 5 and TypeScript. SQLite and compatible YAML profiles. Browser control through CDP / WebDriver BiDi.

[Build and checks](docs/BUILD.md) · [Release through a PR](docs/release-pr.md)

**💜 Support development**

If the app helped you, I would appreciate a donation to support its development and maintenance.

[![Support on GitHub](https://img.shields.io/badge/Support%20on%20GitHub-ea4aaa?logo=githubsponsors&logoColor=white)](https://github.com/sponsors/itwormz)
[![Donate with crypto](https://img.shields.io/badge/Donate%20with%20crypto-5b5bd6)](https://nowpayments.io/donation/itworm)

## 💜 Acknowledgements

Thank you for helping spread the word about the project:

- <a href="https://t.me/bcn_today"><img src="docs/images/bcn_today.png" width="32" height="32" alt=""> BCN Today</a>
- <a href="https://t.me/spain_pass"><img src="docs/images/spain_pass.png" width="32" height="32" alt=""> Испанский Паспорт</a>

## 📄 License

[PolyForm Noncommercial 1.0.0](LICENSE). Noncommercial use, copying, modification and distribution are permitted, provided the license terms and copyright notice are preserved. Commercial use is not permitted. This license applies to this project’s code; third-party components retain their own licenses.
