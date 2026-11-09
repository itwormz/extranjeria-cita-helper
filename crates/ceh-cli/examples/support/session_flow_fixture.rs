//! Continuous native flow over synthetic intercepted pages, never the ICP network.
use super::{Client, Result, portal_fixture::Fixture};
use ceh_browser::{BrowserError, DispatchGate, session_driver::SessionDriver};
use ceh_core::{model::*, profiles::ProfileDraft, timings::AssistTimings};
use serde_json::{Value, json};
use std::time::Duration;
const ENTRY: &str = "/icpplus/index.html";
const PROCEDURE: &str = "/icpplus/procedure-fixture";
const AUTH: &str = "/icpplus/auth-fixture";
const PROVIDER: &str = "https://pasarela.clave.gob.es/auth-fixture";
const APPLICANT: &str = "/icpplus/acEntrada";
const REQUEST: &str = "/icpplus/acValidarEntrada";
const OFFICES: &str = "/icpplus/acCitar";
const CONTACT: &str = "/icpplus/acVerificaSede";
const BOOKING: &str = "/icpplus/acOfertarCita";
fn guarded() -> &'static str {
    "onclick='if(!event.isTrusted)event.preventDefault()'"
}
pub fn run(client: &mut Client, context: &str) -> Result<()> {
    let mut fixture = Fixture::new(client, context, vec![])?;
    let mut completed = 0;
    let runtime = std::path::Path::new(".local/builds/work/session-flow");
    std::fs::create_dir_all(runtime)?;
    let directory = tempfile::tempdir_in(runtime)?;
    let database = directory.path().join("statistics.sqlite3");
    ceh_store::migrate::migrate_paths(
        &ceh_store::migrate::MigrationOptions {
            database: database.clone(),
            legacy_json: None,
            backup_directory: directory.path().join("backups"),
        },
        1793592000000,
    )?;
    let mut db = rusqlite::Connection::open(database)?;

    for service in ["huellas", "regreso"] {
        let option = if service == "huellas" {
            "POLICÍA-TOMA DE HUELLAS (EXPEDICIÓN DE TARJETA) INICIAL, RENOVACIÓN, DUPLICADO Y LEY 14/2013"
        } else {
            "AUTORIZACIÓN DE REGRESO"
        };
        let nationality = if service == "huellas" {
            "<select id='txtPaisNac'><option>Seleccionar</option><option>RUSIA</option></select>"
        } else {
            ""
        };
        let radios = if service == "regreso" {
            "<input type='radio' name='rdbTipoDoc' id='rdbTipoDocNie' value='N.I.E.' checked><input type='radio' name='rdbTipoDoc' id='rdbTipoDocPas' value='PASAPORTE' onclick=\"document.getElementById('txtIdCitado').maxLength=20\">"
        } else {
            "<input type='radio' name='rdbTipoDoc' id='rdbTipoDocNie' value='N.I.E.' checked>"
        };
        fixture.routes=vec![
   ("GET",ENTRY,format!("<body><form method='post' action='{PROCEDURE}'><select id='form'><option>Seleccionar</option><option>Barcelona</option></select><button id='btnAceptar' {}>Aceptar</button></form></body>",guarded())),
   ("POST",PROCEDURE,format!("<body><form method='post' action='{AUTH}'><select id='extra'><option>Ignore</option></select><select id='tramiteGrupo[0]'><option>Seleccionar</option><option>{option}</option></select><button id='btnAceptar' {}>Aceptar</button></form></body>",guarded())),
   ("POST",AUTH,format!("<body><button id='btnAccesoClave' onclick=\"if(event.isTrusted)location.href='{PROVIDER}'\">Acceder con Cl@ve</button></body>")),
   ("GET",PROVIDER,format!("<body><button onclick=\"if(event.isTrusted)location.href='https://icp.administracionelectronica.gob.es{APPLICANT}'\">Certificado electrónico</button></body>")),
   ("GET",APPLICANT,format!("<body><form method='post' action='{REQUEST}'>{radios}<input id='txtIdCitado' maxlength='9'><input id='txtDesCitado'>{nationality}<button id='btnEnviar' {}>Aceptar</button></form></body>",guarded())),
   ("POST",REQUEST,format!("<body><form method='post' action='{OFFICES}'><button id='btnEnviar' value='Solicitar cita' {}>Solicitar cita</button></form></body>",guarded())),
   ("POST",OFFICES,format!("<body><form method='post' action='{CONTACT}'><select id='idSede'><option value='-1'>Seleccionar</option><option value='synthetic-office'>CNP TEST, CALLE EJEMPLO 1</option></select><button id='btnSiguiente' {}>Siguiente</button></form></body>",guarded())),
   ("POST",CONTACT,format!("<body><form method='post' action='{BOOKING}'><input id='txtTelefonoCitado' type='tel'><input id='emailUNO' type='email'><input id='emailDOS' type='email'><button id='btnSiguiente' {}>Siguiente</button></form><script>for(const el of document.querySelectorAll('input'))el.oninput=e=>{{if(!e.isTrusted)el.value='';}};</script></body>",guarded())),
   ("POST",BOOKING,"<body style='min-height:1800px'><label for='offer'>Día: 12/11/2026 Hora: 09:30</label><input id='offer' name='rdbCita' type='radio' value='PRIVATE-SLOT'><input id='captcha' value='PRIVATE-CAPTCHA'><input id='manual' value='PRIVATE-MANUAL'><script>window.onload=()=>{document.getElementById('manual').focus({preventScroll:true});window.scrollTo(0,400);};</script></body>".into()),
  ];
        fixture.navigate(client, context, ENTRY)?;
        let cases: Value = serde_json::from_str(include_str!(
            "../../../../tests/contracts/fixtures/profile-validation.json"
        ))?;
        let mut profile: ProfileDraft =
            serde_json::from_value(cases["cases"][0]["expected"]["normalized"].clone())?;
        profile.province = "Barcelona".into();
        profile.procedure = service.into();
        profile.access_mode = "certificate".into();
        profile.fill_mode = "manual".into();
        profile.full_name = "Alex Example".into();
        profile.nationality = "RUSIA".into();
        profile.phone = "600123456".into();
        profile.email = "alex@example.invalid".into();
        profile.document_type = if service == "huellas" {
            "NIE"
        } else {
            "PASSPORT"
        }
        .into();
        profile.document_number = if service == "huellas" {
            "X1234567L"
        } else {
            "PA123456789012"
        }
        .into();
        let session = SessionId::new(format!("continuous-{service}"))?;
        let gate = DispatchGate::new(session.clone());
        let mut driver = SessionDriver::new(
            context,
            session.clone(),
            profile,
            AssistTimings::default(),
            gate,
        )
        .map_err(|e| format!("driver init {e:?}"))?;
        let scope = ceh_core::statistics::EventScope {
            profile: ProfileId::new("fixture")?,
            session,
        };
        let wall = 1793592000000 + completed * 100000;
        let mut stats = ceh_store::live::LiveStatistics::start(
            &mut db,
            scope,
            &json!({"procedure":service,"browser":"firefox"}),
            wall,
            0,
        )?;
        let mut cursor = 0;
        let mut handed = false;

        for tick in 0..80 {
            fixture.pump(client)?;
            for event in &client.events[cursor..] {
                driver.ingest(event).map_err(|_| "fixture event")?;
            }
            cursor = client.events.len();
            let step = match driver.plan(client.bidi.as_mut().ok_or("missing BiDi")?, tick * 500) {
                Ok(step) => step,
                Err(BrowserError::NoObservation)
                    if client.bidi.as_ref().is_some_and(|c| c.available()) =>
                {
                    client
                        .events
                        .extend(client.bidi.as_mut().ok_or("missing BiDi")?.take_events());
                    std::thread::sleep(Duration::from_millis(20));
                    continue;
                }
                Err(e) => return Err(format!("continuous {service} plan{tick}: {e:?}").into()),
            };
            if step.update().preparing{stats.preparation_page(&mut db,&step.update().observation,ceh_core::control::ControlMode::Automatic,wall+tick as i64*500,tick*500)}else{stats.page(&mut db,&step.update().observation,&step.update().offices,ceh_core::control::ControlMode::Automatic,wall+tick as i64*500,tick*500)}?;
            let update = driver
                .execute(client.bidi.as_mut().ok_or("missing BiDi")?, step)
                .map_err(|e| format!("continuous {service} execute{tick}: {e:?}"))?;
            for (index, receipt) in update
                .receipts
                .iter()
                .filter(|r| r.outcome == DispatchOutcome::Dispatched)
                .enumerate()
            {
                let id = OperationId::new(format!("operation-{tick}-{index}"))?;
                stats.operation(
                    &mut db,
                    ceh_core::statistics::CounterKind::AutomationAction,
                    id.clone(),
                    ceh_core::control::ControlMode::Automatic,
                    wall + tick as i64 * 500,
                )?;
                if receipt.action == ActionKind::Request {
                    stats.operation(
                        &mut db,
                        ceh_core::statistics::CounterKind::Check,
                        id,
                        ceh_core::control::ControlMode::Automatic,
                        wall + tick as i64 * 500,
                    )?;
                }
            }
            client
                .events
                .extend(client.bidi.as_mut().ok_or("missing BiDi")?.take_events());
            if update.control.mode() == ceh_core::control::ControlMode::Manual {
                if update.observation.kind != PageKind::Booking
                    || update.details.offers.len() != 1
                    || update.details.office.as_deref() != Some("CNP TEST, CALLE EJEMPLO 1")
                {
                    return Err("handoff details mismatch".into());
                }
                let kept=client.call("script.evaluate",json!({"expression":"document.activeElement.id==='manual'&&window.scrollY===400&&document.getElementById('captcha').value==='PRIVATE-CAPTCHA'&&!document.getElementById('offer').checked","target":{"context":context},"awaitPromise":false}))?;
                if kept["result"]["value"] != true {
                    return Err("handoff changed manual page".into());
                }
                if !matches!(
                    driver.tick(client.bidi.as_mut().ok_or("missing BiDi")?, tick * 500 + 1),
                    Err(BrowserError::ManualOwnership)
                ) {
                    return Err("driver resumed booking automation".into());
                }
                stats.enrich_booking(
                    &mut db,
                    &update.observation.document,
                    &serde_json::to_value(&update.details)?,
                )?;
                stats.set_phase(
                    &mut db,
                    "booking_handoff",
                    wall + tick as i64 * 500,
                    tick * 500,
                )?;
                stats.finish(&mut db, wall + tick as i64 * 500, tick * 500, "completed")?;
                handed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if !handed {
            return Err(format!("continuous {service} did not reach booking").into());
        }
        completed += 1;
    }
    let summary = ceh_store::statistics::StatisticsReader::new(&db).summary(
        "fixture",
        ceh_store::periods::Period::All,
        chrono::DateTime::from_timestamp_millis(1793592300000).ok_or("fixture clock")?,
    )?;
    if summary["checks"] != 2
        || summary["booking_screens"] != 2
        || summary["offices_seen"] != 2
        || summary["actions_performed"] != 18
    {
        return Err(format!("live SQLite counters mismatch: {summary}").into());
    }
    if fixture.posts != 12 {
        return Err("continuous flow POST count mismatch".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"completed_services":completed,"live_sqlite_counters_accumulate":true,"checks":summary["checks"],"actions":summary["actions_performed"],"trusted_clicks_and_contact_keys":true,"booking_handoff_before_public_read":true,"manual_page_unchanged":true,"no_live_site_requests":true,"post_requests":fixture.posts})
        )?
    );
    Ok(())
}
