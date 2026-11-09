//! Cooldown/exit integration with synthetic time and intercepted offline responses.
use super::{
    Client, Result,
    portal_fixture::{Fixture, seed_observer},
};
use ceh_browser::{
    DispatchGate,
    exit::{ExitAction, activate_exit, control_available},
    observer::PageObserver,
};
use ceh_core::{
    flow::{FlowEffect, FlowEngine, PageFacts},
    model::*,
};
use serde_json::json;
const RESULT: &str = "/icpplus/salirInicio";
const PROVINCE: &str = "<body><select id='form'><option>Seleccionar</option></select><button id='btnAceptar'>Aceptar</button></body>";
pub fn run(client: &mut Client, context: &str) -> Result<()> {
    let mut fixture = Fixture::new(client, context, vec![])?;
    for (index,(path,kind,action,body)) in [
  ("/icpplus/acCitar",PageKind::RequestNoSlotsRestart,ActionKind::NoSlotsRestart,"<body>No hay citas disponibles<p>Cod. Oper.: A43TEST</p><form method='post' action='salirInicio'><button id='btnSubmit' onclick='if(!event.isTrusted)event.preventDefault()'>Aceptar</button></form></body>"),
  ("/icpplus/acCitar",PageKind::RequestNoSlots,ActionKind::NoSlotsSalir,"<body>No hay citas disponibles<form method='post' action='salirInicio'><button onclick='if(!event.isTrusted)event.preventDefault()'>Salir</button></form></body>"),
  ("/icpplus/acOfertarCita",PageKind::PostContactNoSlots,ActionKind::PostContactRestart,"<body>No hay citas disponibles<form method='post' action='salirInicio'><button id='btnSubmit' onclick='if(!event.isTrusted)event.preventDefault()'>Aceptar</button></form></body>")
 ].into_iter().enumerate(){
  fixture.routes=vec![("GET",path,body.into()),("POST",RESULT,PROVINCE.into())];fixture.navigate(client,context,path)?;
  let session=SessionId::new(format!("exit-fixture-{index}"))?;let gate=DispatchGate::new(session.clone());let mut flow=FlowEngine::new(session,SearchOptions::default());let mut observer=PageObserver::new(context).map_err(|_|"exit observer invalid")?;let mut cursor=0;seed_observer(client,&mut observer,&mut cursor)?;
  if index==2 {observer.set_actions(Some(ActionKind::Contact),None);}
  let page=observer.observe(client.bidi.as_mut().ok_or("missing BiDi")?,"manual").map_err(|_|"exit observation failed")?;
  if page.observation.kind!=kind{return Err("exit page kind wrong".into());}
  let exit=ExitAction::from_action(action).map_err(|_|"exit action invalid")?;
  let own_control=control_available(client.bidi.as_mut().ok_or("missing BiDi")?,&page,&exit).map_err(|_|"exit control fact failed")?;
  if !own_control {return Err("exit control unavailable".into());}
  let mut facts=PageFacts::default();facts.recovery.own_control=own_control;
  let current=page.observation.clone();let mut effects=flow.observe_with_facts(page.observation,facts,0).map_err(|_|"exit core failed")?;
  if index<2 {
   let deadline=effects.iter().find_map(|e|if let FlowEffect::WaitUntil(value)=e{Some(*value)}else{None}).ok_or("exit skipped cooldown")?;
   if fixture.posts!=index {return Err("exit before cooldown".into());}
   effects=flow.observe_with_facts(current,facts,deadline).map_err(|_|"exit cooldown failed")?;
  }
  let Some(FlowEffect::Activate{guard,action:actual})=effects.first()else{return Err("core did not activate exit".into());};if *actual!=action{return Err("wrong exit action".into());}
  if index==1{
   client.call("script.evaluate",json!({"expression":"const second=document.createElement('button');second.id='second-exit';second.textContent='Salir';document.body.append(second);true","target":{"context":context},"awaitPromise":false}))?;
   if activate_exit(client.bidi.as_mut().ok_or("missing BiDi")?,&mut observer,&gate,guard,&exit).map_err(|_|"ambiguous exit probe failed")?!=DispatchOutcome::NotDispatched{return Err("ambiguous exit clicked".into());}
   client.call("script.evaluate",json!({"expression":"document.getElementById('second-exit').remove();true","target":{"context":context},"awaitPromise":false}))?;
  }
  let sent=activate_exit(client.bidi.as_mut().ok_or("missing BiDi")?,&mut observer,&gate,guard,&exit).map_err(|e|format!("exit activation failed: {e:?}"))?;
  if sent!=DispatchOutcome::Dispatched{return Err("exit not sent".into());}flow.acknowledge_dispatch(guard,action,sent).map_err(|_|"exit ack failed")?;
  fixture.wait_load(client,context,RESULT)?;seed_observer(client,&mut observer,&mut cursor)?;
  let result=observer.observe(client.bidi.as_mut().ok_or("missing BiDi")?,"manual").map_err(|_|"exit result read failed")?;
  if result.observation.kind!=PageKind::Province{return Err("exit did not reach province".into());}
 }
    if fixture.entry != 3 || fixture.posts != 3 {
        return Err("exit request count wrong".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"no_live_site_requests":true,"clock_advanced_fixture_only":true,"core_cooldown_before_exit":true,"restart_salir_post_contact":true,"ambiguous_exit_refused":true,"trusted_exit_buttons":true,"province_recognized":true,"get_requests":fixture.entry,"post_requests":fixture.posts})
        )?
    );
    Ok(())
}
