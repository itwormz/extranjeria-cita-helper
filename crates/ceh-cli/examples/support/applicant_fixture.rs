//! All data synthetic; official-origin responses are provided before networking.
use super::{
    Client, Result,
    portal_fixture::{Fixture, seed_observer},
};
use ceh_browser::{
    BrowserError, DispatchGate,
    applicant::{ApplicantInput, activate_applicant, prepare_applicant},
    observer::PageObserver,
    request::activate_request,
};
use ceh_core::{
    flow::{FlowEffect, FlowEngine, PreparationOutcome},
    model::{
        ApplicantNoSlotsMode, DispatchOutcome, DocumentGuard, PageKind, SearchOptions, SessionId,
    },
    profiles::ProfileDraft,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
const ENTRY: &str = "/icpplus/rust-fixture-applicant";
const RESULT: &str = "/icpplus/acValidarEntrada";
const REQUESTED: &str = "/icpplus/rust-fixture-requested";
const SUCCESS: &str = "<body><form method='post' action='/icpplus/rust-fixture-requested'><button id='btnEnviar' value='Solicitar cita' onclick='if(!event.isTrusted)event.preventDefault()'>Solicitar cita</button></form></body>";
const EMPTY: &str = "<body>No hay citas disponibles</body>";
fn profile() -> Result<ProfileDraft> {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../../contracts/fixtures/profile-validation.json"
    ))?;
    Ok(serde_json::from_value(
        corpus["cases"][0]["expected"]["normalized"].clone(),
    )?)
}
fn page(regreso: bool, copy: bool, banner: bool, broken_max: bool) -> String {
    let auth_kind = if regreso { "Dni" } else { "Nie" };
    let auth_number = if regreso { "00000000T" } else { "X1234567L" };
    let radios = if regreso {
        format!(
            "<input type='radio' name='rdbTipoDoc' id='rdbTipoDocNie' value='N.I.E.' checked onclick=\"document.getElementById('txtIdCitado').maxLength=9\"><input type='radio' name='rdbTipoDoc' id='rdbTipoDocDni' value='D.N.I.' onclick=\"document.getElementById('txtIdCitado').maxLength=9\"><input type='radio' name='rdbTipoDoc' id='rdbTipoDocPas' value='PASAPORTE' onclick=\"document.getElementById('txtIdCitado').maxLength={}\">",
            if broken_max { 9 } else { 20 }
        )
    } else {
        "<input type='radio' name='rdbTipoDoc' id='rdbTipoDocNie' value='N.I.E.' checked>"
            .to_owned()
    };
    let copy_button = if copy {
        format!(
            "<button type='button' id='btnCopiar' onclick=\"window.copyClicks=(window.copyClicks||0)+1;setTimeout(()=>{{document.getElementById('rdbTipoDoc{auth_kind}').checked=true;for(const [id,value] of [['txtIdCitado','{auth_number}'],['txtDesCitado','AUTH EXAMPLE']]){{const field=document.getElementById(id);field.value=value;field.readOnly=true;}}}},50)\">Copiar</button>"
        )
    } else {
        String::new()
    };
    let country = if regreso {
        String::new()
    } else {
        "<select id='txtPaisNac' name='txtPaisNac'><option>Seleccionar</option><option value='test-country'>RUSIA</option></select>".to_owned()
    };
    format!(
        "<body style='min-height:2000px'><input id='manualField' value='synthetic'>{}<form method='post' action='{RESULT}'>{radios}<input id='txtIdCitado' name='txtIdCitado' maxlength='9'><input id='txtDesCitado' name='txtDesCitado' onchange='this.value=this.value.toUpperCase()'>{copy_button}{country}<button id='btnEnviar' onclick='if(!event.isTrusted)event.preventDefault()'>Aceptar</button></form></body>",
        if banner {
            "<p>No hay citas disponibles</p>"
        } else {
            ""
        }
    )
}
pub fn run(client: &mut Client, context: &str) -> Result<()> {
    let mut fixture = Fixture::new(client, context, vec![])?;
    let cases = [
        ("huellas", "NIE", false, false),
        ("huellas", "NIE", true, false),
        ("regreso", "NIE", false, false),
        ("regreso", "DNI", false, false),
        ("regreso", "PASSPORT", false, false),
        ("regreso", "PASSPORT", true, false),
        ("regreso", "PASSPORT", false, true),
    ];
    for (index, (service, kind, copy, banner)) in cases.into_iter().enumerate() {
        let regreso = service == "regreso";
        fixture.routes = vec![
            ("GET", ENTRY, page(regreso, copy, banner, false)),
            ("POST", RESULT, SUCCESS.to_owned()),
            ("POST", REQUESTED, EMPTY.to_owned()),
        ];
        fixture.navigate(client, context, ENTRY)?;
        client.call("script.evaluate",json!({"expression":"document.getElementById('manualField').focus({preventScroll:true});window.scrollTo(0,200);true","target":{"context":context},"awaitPromise":false}))?;
        let mut p = profile()?;
        p.procedure = service.into();
        p.document_type = kind.into();
        p.fill_mode = if copy { "copy" } else { "manual" }.into();
        p.access_mode = "certificate".into();
        p.full_name = "Alex Example".into();
        p.nationality = if regreso { "" } else { "RUSIA" }.into();
        p.document_number = match kind {
            "NIE" => "X1234567L",
            "DNI" => "00000000T",
            _ => "PA123456789012",
        }
        .into();
        let session = SessionId::new(format!("applicant-fixture-{index}"))?;
        let gate = DispatchGate::new(session.clone());
        let mut observer = PageObserver::new(context).map_err(|_| "invalid applicant observer")?;
        let mut cursor = 0;
        seed_observer(client, &mut observer, &mut cursor)?;
        if banner {
            p.applicant_no_slots_mode = "wait".into();
            let wait = ApplicantInput::from_profile(&p).map_err(|_| "invalid wait profile")?;
            let current = observer
                .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
                .map_err(|_| "wait observation failed")?;
            let guard = DocumentGuard {
                session: session.clone(),
                epoch: 0,
                document: current.observation.document,
                expected_page: PageKind::ApplicantNoSlots,
            };
            if !matches!(
                prepare_applicant(
                    client.bidi.as_mut().ok_or("missing BiDi")?,
                    &mut observer,
                    &gate,
                    &guard,
                    &wait,
                    Duration::ZERO,
            Instant::now() + Duration::from_secs(2)
                ),
                Err(BrowserError::Unsupported)
            ) {
                return Err("wait mode did not block filling".into());
            }
            let empty=client.call("script.evaluate",json!({"expression":"document.getElementById('txtIdCitado').value==='' && document.getElementById('txtDesCitado').value===''","target":{"context":context},"awaitPromise":false}))?["result"]["value"]==true;
            if !empty {
                return Err("wait mode changed applicant data".into());
            }
        }
        p.applicant_no_slots_mode = if banner { "continue" } else { "wait" }.into();
        let input = ApplicantInput::from_profile(&p).map_err(|_| "invalid applicant profile")?;
        let options = SearchOptions {
            applicant_no_slots_mode: if banner {
                ApplicantNoSlotsMode::Continue
            } else {
                ApplicantNoSlotsMode::Wait
            },
            ..Default::default()
        };
        let mut flow = FlowEngine::new(session, options);
        let current = observer
            .observe(client.bidi.as_mut().ok_or("missing BiDi")?, &p.fill_mode)
            .map_err(|_| "applicant observation failed")?;
        let effects = flow
            .observe(current.observation, 0)
            .map_err(|_| "applicant core failed")?;
        let Some(FlowEffect::Prepare { guard, action }) = effects.first() else {
            return Err("core did not prepare applicant".into());
        };
        let prepared = prepare_applicant(
            client.bidi.as_mut().ok_or("missing BiDi")?,
            &mut observer,
            &gate,
            guard,
            &input,
            Duration::ZERO,
            Instant::now() + Duration::from_secs(3),
        )
        .map_err(|e| format!("applicant prepare failed: {e:?}"))?;
        if prepared != PreparationOutcome::Completed {
            return Err("applicant preparation incomplete".into());
        }
        if copy {
            let repeated = prepare_applicant(
                client.bidi.as_mut().ok_or("missing BiDi")?,
                &mut observer,
                &gate,
                guard,
                &input,
                Duration::ZERO,
            Instant::now() + Duration::from_secs(2),
            )
            .map_err(|_| "known copied data recheck failed")?;
            if repeated != PreparationOutcome::Completed {
                return Err("copied data could not be rechecked".into());
            }
        }
        flow.complete_preparation(guard, *action, prepared)
            .map_err(|_| "applicant preparation ack failed")?;
        let expected_number = if copy && regreso {
            "00000000T"
        } else {
            p.document_number.as_str()
        };
        let expected_name = if copy { "AUTH EXAMPLE" } else { "ALEX EXAMPLE" };
        let expression = format!(
            "document.getElementById('txtIdCitado').value==={} && document.getElementById('txtDesCitado').value==={} && document.activeElement.id==='manualField' && window.scrollY===200 && (window.copyClicks||0)==={} && new FormData(document.querySelector('form')).has('txtIdCitado') && new FormData(document.querySelector('form')).has('txtDesCitado')",
            serde_json::to_string(expected_number)?,
            serde_json::to_string(expected_name)?,
            usize::from(copy)
        );
        if client.call(
            "script.evaluate",
            json!({"expression":expression,"target":{"context":context},"awaitPromise":false}),
        )?["result"]["value"]
            != true
        {
            return Err("applicant data/focus/scroll/copy count wrong".into());
        }
        if index == 0 {
            client.call("script.evaluate",json!({"expression":"document.getElementById('rdbTipoDocNie').checked=false;true","target":{"context":context},"awaitPromise":false}))?;
            if activate_applicant(
                client.bidi.as_mut().ok_or("missing BiDi")?,
                &mut observer,
                &gate,
                guard,
                &input,
            )
            .map_err(|_| "changed type guard failed")?
                != DispatchOutcome::NotDispatched
            {
                return Err("changed huellas document type submitted".into());
            }
            client.call("script.evaluate",json!({"expression":"document.getElementById('rdbTipoDocNie').checked=true;true","target":{"context":context},"awaitPromise":false}))?;
        }
        let current = observer
            .observe(client.bidi.as_mut().ok_or("missing BiDi")?, &p.fill_mode)
            .map_err(|_| "prepared applicant observation failed")?;
        let effects = flow
            .observe(current.observation, 1)
            .map_err(|_| "applicant activation core failed")?;
        let Some(FlowEffect::Activate { guard, action }) = effects.first() else {
            return Err("core did not activate applicant".into());
        };
        let outcome = activate_applicant(
            client.bidi.as_mut().ok_or("missing BiDi")?,
            &mut observer,
            &gate,
            guard,
            &input,
        )
        .map_err(|e| format!("applicant activation failed: {e:?}"))?;
        if outcome != DispatchOutcome::Dispatched {
            return Err("applicant submit unacknowledged".into());
        }
        flow.acknowledge_dispatch(guard, *action, outcome)
            .map_err(|_| "applicant submit ack failed")?;
        fixture.wait_load(client, context, RESULT)?;
        seed_observer(client, &mut observer, &mut cursor)?;
        let current = observer
            .observe(client.bidi.as_mut().ok_or("missing BiDi")?, &p.fill_mode)
            .map_err(|_| "applicant result observation failed")?;
        if current.observation.kind != PageKind::RequestMenu {
            return Err("applicant result was not request menu".into());
        }
        let effects = flow
            .observe(current.observation, 2)
            .map_err(|_| "request core observation failed")?;
        let Some(FlowEffect::Activate { guard, action }) = effects.first() else {
            return Err("core did not request appointment lookup".into());
        };
        let sent = activate_request(
            client.bidi.as_mut().ok_or("missing BiDi")?,
            &mut observer,
            &gate,
            guard,
        )
        .map_err(|e| format!("request activation failed: {e:?}"))?;
        if sent != DispatchOutcome::Dispatched {
            return Err("request not dispatched".into());
        }
        flow.acknowledge_dispatch(guard, *action, sent)
            .map_err(|_| "request acknowledgement failed")?;
        fixture.wait_load(client, context, REQUESTED)?;
        seed_observer(client, &mut observer, &mut cursor)?;
        let result = observer
            .observe(client.bidi.as_mut().ok_or("missing BiDi")?, &p.fill_mode)
            .map_err(|_| "request result observation failed")?;
        if result.observation.kind != PageKind::RequestNoSlots {
            return Err("request result not recognized".into());
        }
    }
    // A changed radio must not cause truncation if the page still imposes NIE length.
    fixture.routes = vec![
        ("GET", ENTRY, page(true, false, false, true)),
        ("POST", RESULT, SUCCESS.to_owned()),
    ];
    fixture.navigate(client, context, ENTRY)?;
    let mut p = profile()?;
    p.procedure = "regreso".into();
    p.fill_mode = "manual".into();
    p.document_type = "PASSPORT".into();
    p.document_number = "PA123456789012".into();
    p.full_name = "ALEX EXAMPLE".into();
    p.nationality.clear();
    let input = ApplicantInput::from_profile(&p).map_err(|_| "invalid long document profile")?;
    let mut observer = PageObserver::new(context).map_err(|_| "invalid length observer")?;
    let mut cursor = 0;
    seed_observer(client, &mut observer, &mut cursor)?;
    let current = observer
        .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
        .map_err(|_| "length observation failed")?;
    let session = SessionId::new("document-length-fixture")?;
    let gate = DispatchGate::new(session.clone());
    let guard = DocumentGuard {
        session,
        epoch: 0,
        document: current.observation.document,
        expected_page: PageKind::Applicant,
    };
    let prepared = prepare_applicant(
        client.bidi.as_mut().ok_or("missing BiDi")?,
        &mut observer,
        &gate,
        &guard,
        &input,
        Duration::ZERO,
            Instant::now() + Duration::from_millis(150),
    )
    .map_err(|_| "length preparation failed")?;
    if prepared != PreparationOutcome::Failed {
        return Err("overlong document was not refused".into());
    }
    let empty=client.call("script.evaluate",json!({"expression":"document.getElementById('txtIdCitado').value==='' && document.getElementById('txtDesCitado').value===''","target":{"context":context},"awaitPromise":false}))?["result"]["value"]==true;
    if !empty {
        return Err("overlong document was truncated or partially filled".into());
    }
    if activate_applicant(
        client.bidi.as_mut().ok_or("missing BiDi")?,
        &mut observer,
        &gate,
        &guard,
        &input,
    )
    .map_err(|_| "length activation failed")?
        != DispatchOutcome::NotDispatched
    {
        return Err("invalid applicant was submitted".into());
    }
    // An incomplete asynchronous copy must time out without another click or submit.
    let incomplete = page(true, true, false, false)
        .replace("['txtDesCitado','AUTH EXAMPLE']", "['txtDesCitado','']");
    fixture.routes = vec![
        ("GET", ENTRY, incomplete),
        ("POST", RESULT, SUCCESS.to_owned()),
    ];
    fixture.navigate(client, context, ENTRY)?;
    p.fill_mode = "copy".into();
    p.access_mode = "certificate".into();
    p.document_number.clear();
    p.full_name.clear();
    let input = ApplicantInput::from_profile(&p).map_err(|_| "invalid incomplete copy profile")?;
    let mut observer = PageObserver::new(context).map_err(|_| "invalid copy observer")?;
    let mut cursor = 0;
    seed_observer(client, &mut observer, &mut cursor)?;
    let current = observer
        .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "copy")
        .map_err(|_| "incomplete copy observation failed")?;
    let session = SessionId::new("incomplete-copy-fixture")?;
    let gate = DispatchGate::new(session.clone());
    let guard = DocumentGuard {
        session,
        epoch: 0,
        document: current.observation.document,
        expected_page: PageKind::Applicant,
    };
    if prepare_applicant(
        client.bidi.as_mut().ok_or("missing BiDi")?,
        &mut observer,
        &gate,
        &guard,
        &input,
        Duration::ZERO,
            Instant::now() + Duration::from_millis(350),
    )
    .map_err(|_| "incomplete copy preparation failed")?
        != PreparationOutcome::Failed
    {
        return Err("incomplete copy not stopped".into());
    }
    if client.call("script.evaluate",json!({"expression":"window.copyClicks===1 && document.getElementById('txtDesCitado').value===''","target":{"context":context},"awaitPromise":false}))?["result"]["value"]!=true {return Err("incomplete copy was clicked again".into());}
    if activate_applicant(
        client.bidi.as_mut().ok_or("missing BiDi")?,
        &mut observer,
        &gate,
        &guard,
        &input,
    )
    .map_err(|_| "incomplete copy activation failed")?
        != DispatchOutcome::NotDispatched
    {
        return Err("incomplete copy was submitted".into());
    }
    if fixture.posts != cases.len() * 2 || fixture.entry != cases.len() + 2 {
        return Err("applicant fixture request count wrong".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"synthetic_cases":cases.len(),"no_live_site_requests":true,"get_requests":fixture.entry,"post_requests":fixture.posts,"core_prepare_activate_native":true,"huellas_nationality_regreso_without_nationality":true,"nie_dni_passport_and_copy":true,"copy_clicked_once":true,"wait_blocks_fields_continue_works":true,"input_focus_scroll_preserved":true,"request_menu_recognized":true,"request_core_activation_and_no_slots_recognized":true,"trusted_request_click_required":true,"overlong_document_not_truncated_or_submitted":true,"named_form_fields_verified":true,"changed_document_type_refused":true,"copy_recheck_idempotent_and_timeout_does_not_reclick":true})
        )?
    );
    Ok(())
}
