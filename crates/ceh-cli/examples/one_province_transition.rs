//! Manual province selection followed by one guarded browser click.
use ceh_browser::{
    DispatchGate,
    firefox::{FirefoxInstallation, OwnedFirefox, ProfileLease},
    observer::PageObserver,
    selection::{Selection, activate_selection},
};
use ceh_core::model::{DispatchOutcome, DocumentGuard, PageKind, SessionId};
use serde_json::json;
use std::{io, path::Path, sync::atomic::AtomicBool, thread, time::{Duration, Instant}};

fn main() -> Result<(), String> {
    let installation = FirefoxInstallation::discover().map_err(|e| format!("{e:?}"))?;
    let runtime = Path::new(".local/builds/runtime/province-probe");
    std::fs::create_dir_all(runtime).map_err(|e| e.to_string())?;
    let runtime = runtime.canonicalize().map_err(|e| e.to_string())?;
    let lease = ProfileLease::temporary(&runtime)
        .map_err(|e| format!("{e:?}"))?;
    let mut browser = OwnedFirefox::start(&installation, lease, false, &AtomicBool::new(false))
        .map_err(|e| format!("{e:?}"))?;
    let result = run(&mut browser);
    browser.handoff();
    result
}

fn run(browser: &mut OwnedFirefox) -> Result<(), String> {
    let context = browser.context().to_owned();
    let mut observer = PageObserver::new(&context).map_err(|e| format!("{e:?}"))?;
    let connection = browser.connection();
    connection.call("browsingContext.navigate", json!({
        "context": context,
        "url": "https://icp.administracionelectronica.gob.es/icpplus/index.html",
        "wait": "none"
    })).map_err(|e| format!("{e:?}"))?;
    println!("Выбрать провинцию в Firefox, затем подтвердить тест строкой go.");
    let mut answer = String::new();
    io::stdin().read_line(&mut answer).map_err(|e| e.to_string())?;
    if answer.trim() != "go" {
        return Ok(());
    }
    let page = observer.observe(connection, "manual").map_err(|e| format!("{e:?}"))?;
    if page.observation.kind != PageKind::Province || !page.page.snapshot.official {
        return Err(format!("No click. Observed {:?}", page.observation.kind));
    }
    let document = page.observation.document;
    let option = connection.call("script.evaluate", json!({
        "target":{"realm":document.as_str()},
        "awaitPromise":false,
        "expression":"(()=>{const s=document.querySelector('select#form');const o=s?.selectedOptions[0];return o&&s.selectedIndex>0&&!o.disabled&&o.value.trim()&&!['0','-1'].includes(o.value.trim())?o.label:null})()"
    })).map_err(|e| format!("{e:?}"))?;
    let label = option["result"]["value"].as_str().ok_or("No province selected. No click.")?;
    let selection = Selection::province(label).map_err(|e| format!("{e:?}"))?;
    let session = SessionId::new("one-province-probe").map_err(|e| format!("{e:?}"))?;
    let gate = DispatchGate::new(session.clone());
    let guard = DocumentGuard { session, epoch: 0, document, expected_page: PageKind::Province };
    let result = activate_selection(connection, &mut observer, &gate, &guard, &selection)
        .map_err(|e| format!("{e:?}"))?;
    gate.interrupt().map_err(|e| format!("{e:?}"))?;
    println!("Click receipt: {result:?}");
    if result == DispatchOutcome::NotDispatched {
        return Ok(());
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if let Ok(page) = observer.observe(connection, "manual")
            && (page.observation.document != guard.document || page.observation.kind != PageKind::Province)
            && !matches!(page.observation.kind, PageKind::Loading | PageKind::Unknown)
        {
            println!("Result: {:?}", page.observation.kind);
            return Ok(());
        }
        thread::sleep(Duration::from_millis(500));
    }
    println!("Result: unconfirmed. No retry.");
    Ok(())
}
