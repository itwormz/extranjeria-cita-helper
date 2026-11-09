//! Public labels only; no manual form mutations, focus changes or scroll.
use super::{
    Client, Result,
    portal_fixture::{Fixture, seed_observer},
};
use ceh_browser::{
    observer::PageObserver,
    public_details::{read_booking, read_offices},
};
use ceh_core::{
    flow::{FlowEffect, FlowEngine},
    model::*,
};
use serde_json::json;
const PATH: &str = "/icpplus/acCitar";
pub fn run(client: &mut Client, context: &str) -> Result<()> {
    let mut fixture=Fixture::new(client,context,vec![("GET",PATH,"<body><select id='idSede'><option value='-1'>Seleccione</option><option value='PRIVATE-ID-A'>Office A, Calle Uno, 1</option><option value='PRIVATE-ID-B' selected>Office B, Calle Dos, 2</option><optgroup disabled><option value='PRIVATE-ID-C'>Disabled office</option></optgroup></select></body>".into())])?;
    fixture.navigate(client, context, PATH)?;
    let mut observer = PageObserver::new(context).map_err(|_| "public observer invalid")?;
    let mut cursor = 0;
    seed_observer(client, &mut observer, &mut cursor)?;
    let page = observer
        .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
        .map_err(|_| "office observation failed")?;
    let offices = read_offices(client.bidi.as_mut().ok_or("missing BiDi")?, &page)
        .map_err(|e| format!("public offices failed: {e:?}"))?;
    if offices.offices != ["Office A, Calle Uno, 1", "Office B, Calle Dos, 2"]
        || offices.selected.as_deref() != Some("Office B, Calle Dos, 2")
    {
        return Err("public office labels mismatch".into());
    }
    fixture.routes=vec![("GET",PATH,"<body style='min-height:2000px'><label for='offer'>Día: 02/11/2026 Hora: 09:30</label><input type='radio' name='rdbCita' id='offer' value='PRIVATE-SLOT-ID'><label for='hidden' style='display:none'>Día: 03/11/2026 Hora: 10:30</label><input type='radio' name='rdbCita' id='hidden' style='display:none'><fieldset disabled><label for='disabled'>Día: 04/11/2026 Hora: 11:30</label><input type='radio' name='rdbCita' id='disabled'></fieldset><select name='horaFiltro'><option>12:00</option></select><input id='captcha' value='PRIVATE-CAPTCHA'><input id='manual' value='PRIVATE-MANUAL'><input type='hidden' value='PRIVATE-TOKEN'><p>Confirmation PRIVATE-CONFIRMATION</p></body>".into())];
    fixture.navigate(client, context, PATH)?;
    seed_observer(client, &mut observer, &mut cursor)?;
    client.call("script.evaluate",json!({"expression":"document.getElementById('manual').focus({preventScroll:true});window.scrollTo(0,400);true","target":{"context":context},"awaitPromise":false}))?;
    let page = observer
        .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
        .map_err(|_| "booking observation failed")?;
    let mut core = FlowEngine::new(SessionId::new("public-details")?, SearchOptions::default());
    if core
        .observe(page.observation.clone(), 0)
        .map_err(|_| "handoff failed")?
        != vec![FlowEffect::Handoff, FlowEffect::Alert]
    {
        return Err("booking not handed off first".into());
    }
    let details = read_booking(
        client.bidi.as_mut().ok_or("missing BiDi")?,
        &page,
        offices.selected.as_deref(),
    )
    .map_err(|e| format!("public booking failed: {e:?}"))?;
    let value = serde_json::to_value(&details)?;
    if value
        != json!({"office":"Office B, Calle Dos, 2","offers":[{"date":"2026-11-02","time":"09:30"}]})
    {
        return Err("public offers/filter privacy mismatch".into());
    }
    let unchanged=client.call("script.evaluate",json!({"expression":"document.activeElement.id==='manual' && window.scrollY===400 && document.getElementById('manual').value==='PRIVATE-MANUAL' && document.getElementById('captcha').value==='PRIVATE-CAPTCHA' && !document.getElementById('offer').checked","target":{"context":context},"awaitPromise":false}))?;
    if unchanged["result"]["value"] != true {
        return Err("manual page changed during public read".into());
    }
    if fixture.posts != 0 {
        return Err("public detail read submitted a form".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"public_offices":offices.offices.len(),"offered_slots":details.offers.len(),"manual_handoff_before_read":true,"manual_input_focus_scroll_preserved":true,"filters_hidden_disabled_excluded":true,"private_fields_excluded":true,"no_live_site_requests":true,"post_requests":fixture.posts})
        )?
    );
    Ok(())
}
