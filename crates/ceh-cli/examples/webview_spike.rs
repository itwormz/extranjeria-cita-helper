//! Isolated hidden WebView localhost experiment, not the trusted desktop UI.
#[cfg(target_os = "macos")]
mod support;
#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use serde_json::{Value, json};
    use std::{
        sync::atomic::Ordering,
        time::{Duration, Instant},
    };
    use tao::{
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoopBuilder},
        platform::run_return::EventLoopExtRunReturn,
        window::WindowBuilder,
    };
    use wry::{PageLoadEvent, WebViewBuilder};
    #[derive(Debug)]
    enum Probe {
        Loaded,
        Facts(Value),
    }
    let server = support::Server::new()?;
    let origin = format!("http://127.0.0.1:{}/", server.port);
    let allowed = origin.clone();
    let mut events = EventLoopBuilder::<Probe>::with_user_event().build();
    let loaded = events.create_proxy();
    let ipc = events.create_proxy();
    let window = WindowBuilder::new()
        .with_title("CEH isolated WebView fixture")
        .with_visible(false)
        .build(&events)?;
    let webview = WebViewBuilder::new()
        .with_incognito(true)
        .with_url(&origin)
        .with_navigation_handler(move |url| url.starts_with(&allowed))
        .with_on_page_load_handler(move |event, _| {
            if matches!(event, PageLoadEvent::Finished) {
                let _ = loaded.send_event(Probe::Loaded);
            }
        })
        .with_ipc_handler(move |request| {
            if request.body().len() <= 4096
                && let Ok(facts) = serde_json::from_str::<Value>(request.body())
            {
                let _ = ipc.send_event(Probe::Facts(facts));
            }
        })
        .build(&window)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut submitted = false;
    let mut done = false;
    let mut facts = Value::Null;
    let mut failure = false;
    events.run_return(|event,_,flow|{
  *flow=ControlFlow::WaitUntil(deadline);
  if Instant::now()>=deadline{failure=true;*flow=ControlFlow::Exit;return;}
  match event{
   Event::UserEvent(Probe::Loaded)=>{
    let script=if !submitted{"window.ipc.postMessage(JSON.stringify({phase:'form',script:window.scriptLoaded===true,frame:document.querySelector('iframe')?.contentDocument?.getElementById('frame')?.textContent,form:!!document.getElementById('submit')}));"}else{"window.ipc.postMessage(JSON.stringify({phase:'result',done:!!document.getElementById('done')}));"};
    if webview.evaluate_script(script).is_err(){failure=true;*flow=ControlFlow::Exit;}
   },
   Event::UserEvent(Probe::Facts(value))=>{
    if value["phase"]=="form"&&!submitted {facts=json!({"script":value["script"],"frame":value["frame"],"form":value["form"]});if facts!=json!({"script":true,"frame":"frame","form":true}){failure=true;*flow=ControlFlow::Exit;}else{submitted=true;if webview.evaluate_script("document.getElementById('submit').click()").is_err(){failure=true;*flow=ControlFlow::Exit;}}}
    else if value["phase"]=="result"{done=value["done"]==true;*flow=ControlFlow::Exit;}
   },
   Event::WindowEvent{event:WindowEvent::CloseRequested,..}=>{failure=true;*flow=ControlFlow::Exit;},_=>{}
  }
 });
    let count = server.posts.load(Ordering::SeqCst);
    let report = json!({"candidate":"system_webview","os":"macOS","isolated_incognito":true,"hidden_window":true,"python_runtime":false,"node_runtime":false,"localhost_fixture":true,"dom":facts,"submit_commands_sent":usize::from(submitted),"server_post_receipts":count,"result_page":done,"test_success":!failure&&done&&count==1,"not_verified":["passive HTTP method/body capture","certificate and Clave","manual/background/lock","popup ownership","production IPC boundary"]});
    println!("{}", serde_json::to_string_pretty(&report)?);
    drop(webview);
    drop(window);
    if failure || !done || count != 1 {
        return Err("WebView fixture acceptance failed".into());
    }
    Ok(())
}
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("This isolated spike currently targets macOS only");
    std::process::exit(2);
}
