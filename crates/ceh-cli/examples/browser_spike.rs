#[path = "support/applicant_fixture.rs"]
mod applicant_fixture;
#[path = "support/auth_fixture.rs"]
mod auth_fixture;
#[path = "support/auth_method_fixture.rs"]
mod auth_method_fixture;
#[path = "support/contact_fixture.rs"]
mod contact_fixture;
#[path = "support/cookie_fixture.rs"]
mod cookie_fixture;
#[path = "support/exit_fixture.rs"]
mod exit_fixture;
#[path = "support/guard_probe.rs"]
mod guard_probe;
#[path = "support/passive_fixture.rs"]
mod passive_fixture;
#[path = "support/portal_fixture.rs"]
mod portal_fixture;
#[path = "support/public_details_fixture.rs"]
mod public_details_fixture;
#[path = "support/recovery_facts_fixture.rs"]
mod recovery_facts_fixture;
#[path = "support/refresh_fixture.rs"]
mod refresh_fixture;
#[path = "support/refresh_tls_fixture.rs"]
mod refresh_tls_fixture;
#[path = "support/session_flow_fixture.rs"]
mod session_flow_fixture;
mod support;
#[path = "support/tls_fixture.rs"]
mod tls_fixture;
#[path = "support/trusted_input_fixture.rs"]
mod trusted_input_fixture;
use support::Server;
// Opt-in localhost-only protocol experiment, not a production browser adapter.
use serde_json::{Value, json};
use std::{
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Client {
    socket: Option<WebSocket<TcpStream>>,
    bidi: Option<ceh_browser::bidi::BidiConnection>,
    ticket: Option<ceh_browser::bidi::Ticket>,
    id: u64,
    events: Vec<Value>,
}
impl Client {
    fn connect(port: u16, path: &str) -> Result<Self> {
        if path == "/session" {
            let address = format!("127.0.0.1:{port}").parse()?;
            let bidi = ceh_browser::bidi::BidiConnection::connect(address, Duration::from_secs(5))
                .map_err(|_| "BiDi connection unavailable")?;
            return Ok(Self {
                socket: None,
                bidi: Some(bidi),
                ticket: None,
                id: 0,
                events: vec![],
            });
        }
        let stream = TcpStream::connect_timeout(
            &format!("127.0.0.1:{port}").parse()?,
            Duration::from_secs(1),
        )?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let (socket, _) = tungstenite::client(format!("ws://127.0.0.1:{port}{path}"), stream)?;
        Ok(Self {
            socket: Some(socket),
            bidi: None,
            ticket: None,
            id: 0,
            events: vec![],
        })
    }
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        if std::env::var_os("CEH_SPIKE_TRACE").is_some() {
            eprintln!("protocol: {method}");
        }
        self.send(method, params)?;
        self.receive()
    }
    fn send(&mut self, method: &str, params: Value) -> Result<()> {
        if let Some(bidi) = &mut self.bidi {
            self.ticket = Some(
                bidi.send(method, params)
                    .map_err(|_| "BiDi command delivery unknown")?,
            );
            return Ok(());
        }
        self.id += 1;
        self.socket
            .as_mut()
            .ok_or("missing CDP socket")?
            .send(Message::Text(
                json!({"id":self.id,"method":method,"params":params})
                    .to_string()
                    .into(),
            ))?;
        Ok(())
    }
    fn receive(&mut self) -> Result<Value> {
        if let Some(bidi) = &mut self.bidi {
            let result = bidi.receive(self.ticket.take().ok_or("missing BiDi ticket")?);
            self.events.extend(bidi.take_events());
            return result.map_err(|error| format!("BiDi command failed: {error:?}").into());
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let message = self.socket.as_mut().ok_or("missing CDP socket")?.read()?;
            if let Message::Text(text) = message {
                let value: Value = serde_json::from_str(&text)?;
                if value["id"] == self.id {
                    if value.get("error").is_some() || value["type"] == "error" {
                        return Err("protocol command rejected".to_string().into());
                    }
                    return Ok(value["result"].clone());
                }
                if self.events.len() < 256 {
                    self.events.push(value);
                }
            }
        }
        Err("protocol deadline".into())
    }
}
fn main() -> Result<()> {
    let kind = std::env::args()
        .nth(1)
        .ok_or("choose firefox, camoufox or chrome")?;
    let lost_response = std::env::args().nth(2).as_deref() == Some("lost");
    let isolation = std::env::args().nth(2).as_deref() == Some("isolation");
    let guards = std::env::args().nth(2).as_deref() == Some("guards");
    if guards && kind == "chrome" {
        return Err("guard experiment requires BiDi".into());
    }
    let cdp = kind == "chrome";
    let binary = match kind.as_str() {
        "firefox" => "/Applications/Firefox.app/Contents/MacOS/firefox",
        "camoufox" => {
            "/Applications/Cita Extranjeria Helper.app/Contents/Resources/bundled-browser/Camoufox.app/Contents/MacOS/camoufox"
        }
        "chrome" => "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        _ => return Err("unsupported candidate".into()),
    };
    let root = std::path::Path::new(".local/builds/work/browser-spikes");
    std::fs::create_dir_all(root)?;
    let profile = tempfile::tempdir_in(root)?;
    let tls_mode = std::env::args().nth(2).as_deref() == Some("refresh-tls");
    if tls_mode && kind != "camoufox" {
        return Err("TLS fixture requires its verified Camoufox binary".into());
    }
    let tls = if tls_mode {
        Some(tls_fixture::TlsFixture::new(profile.path(), binary)?)
    } else {
        None
    };
    let proxy_port = tls.as_ref().map_or(9, |fixture| fixture.port);
    let server = Server::new()?;
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let mut command = Command::new(binary);
    if cdp {
        command.args([
            "--headless=new",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-background-networking",
            "--disable-component-update",
            "--disable-sync",
            "--disable-extensions",
            "--proxy-server=http://127.0.0.1:9",
            "--proxy-bypass-list=127.0.0.1;localhost",
        ]);
        command
            .arg(format!("--user-data-dir={}", profile.path().display()))
            .arg(format!("--remote-debugging-port={port}"));
    } else {
        std::fs::write(
            profile.path().join("user.js"),
            format!(
                "user_pref(\"network.proxy.type\",1);user_pref(\"network.proxy.http\",\"127.0.0.1\");user_pref(\"network.proxy.http_port\",{proxy_port});user_pref(\"network.proxy.ssl\",\"127.0.0.1\");user_pref(\"network.proxy.ssl_port\",{proxy_port});user_pref(\"network.proxy.no_proxies_on\",\"localhost,127.0.0.1\");user_pref(\"datareporting.policy.dataSubmissionEnabled\",false);user_pref(\"toolkit.telemetry.enabled\",false);user_pref(\"browser.shell.checkDefaultBrowser\",false);"
            ),
        )?;
        if std::env::args().nth(3).as_deref()!=Some("headed"){command.arg("--headless");}
        command
            .args(["--no-remote", "--new-instance", "--profile"])
            .arg(profile.path())
            .arg("--remote-debugging-port")
            .arg(port.to_string());
    }
    command
        .arg("about:blank")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = OwnedChild(command.spawn()?);
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut ready = false;
    while Instant::now() < deadline {
        if child.0.try_wait()?.is_some() {
            return Err("isolated browser exited before protocol startup".into());
        }
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            ready = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if !ready {
        return Err("browser did not expose requested protocol (no compatibility claim)".into());
    }
    let http = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let path = if cdp {
        let targets: Value = http
            .get(format!("http://127.0.0.1:{port}/json/list"))
            .send()?
            .json()?;
        let url = targets
            .as_array()
            .and_then(|rows| rows.iter().find(|row| row["type"] == "page"))
            .and_then(|row| row["webSocketDebuggerUrl"].as_str())
            .ok_or("missing page websocket")?;
        let parsed = reqwest::Url::parse(url)?;
        if parsed.host_str() != Some("127.0.0.1") && parsed.host_str() != Some("localhost") {
            return Err("nonlocal endpoint".into());
        }
        if parsed.port() != Some(port) {
            return Err("wrong endpoint port".into());
        }
        parsed.path().to_string()
    } else {
        "/session".into()
    };
    let mut client = Client::connect(port, &path)?;
    let context = if cdp {
        client.call("Page.enable", json!({}))?;
        client.call("Network.enable", json!({}))?;
        None
    } else {
        client.bidi.as_mut().ok_or("missing BiDi connection")?
            .call("session.new", json!({"capabilities":{}}))
            .map_err(|error| format!("BiDi session failed: {error:?}"))?;
        client.call("session.subscribe",json!({"events":["network.beforeRequestSent","network.responseCompleted","browsingContext.load","browsingContext.navigationStarted"]}))?;
        let tree = client.call("browsingContext.getTree", json!({}))?;
        Some(
            tree["contexts"][0]["context"]
                .as_str()
                .ok_or("missing context")?
                .to_string(),
        )
    };
    if let Some(fixture) = tls.as_ref() {
        return refresh_tls_fixture::run(
            &mut client,
            context.as_deref().ok_or("missing context")?,
            fixture,
        );
    }
    if std::env::args().nth(2).as_deref() == Some("session-flow") {
        return session_flow_fixture::run(
            &mut client,
            context.as_deref().ok_or("missing context")?,
        );
    }
    if std::env::args().nth(2).as_deref() == Some("recovery-click") {
        return recovery_facts_fixture::run(
            &mut client,
            context.as_deref().ok_or("missing context")?,
            true,
        );
    }
    if std::env::args().nth(2).as_deref() == Some("recovery-facts") {
        return recovery_facts_fixture::run(
            &mut client,
            context.as_deref().ok_or("missing context")?,
            false,
        );
    }
    if std::env::args().nth(2).as_deref() == Some("public-details") {
        return public_details_fixture::run(
            &mut client,
            context.as_deref().ok_or("missing context")?,
        );
    }
    if std::env::args().nth(2).as_deref() == Some("cookies") {
        return cookie_fixture::run(&mut client, context.as_deref().ok_or("missing context")?);
    }
    if std::env::args().nth(2).as_deref() == Some("auth-methods") {
        if client.bidi.is_none() {
            return Err("auth method fixture requires BiDi".into());
        }
        return auth_method_fixture::run(&mut client, context.as_deref().ok_or("missing context")?);
    }
    if std::env::args().nth(2).as_deref() == Some("auth") {
        if client.bidi.is_none() {
            return Err("auth fixture requires BiDi".into());
        }
        return auth_fixture::run(&mut client, context.as_deref().ok_or("missing context")?);
    }
    if std::env::args().nth(2).as_deref() == Some("refresh") {
        if client.bidi.is_none() {
            return Err("refresh fixture requires BiDi".into());
        }
        return refresh_fixture::run(&mut client, context.as_deref().ok_or("missing context")?);
    }
    if std::env::args().nth(2).as_deref() == Some("passive") {
        if client.bidi.is_none() {
            return Err("passive fixture requires BiDi".into());
        }
        return passive_fixture::run(&mut client, context.as_deref().ok_or("missing context")?);
    }
    if std::env::args().nth(2).as_deref() == Some("exit") {
        if client.bidi.is_none() {
            return Err("exit fixture requires BiDi".into());
        }
        return exit_fixture::run(&mut client, context.as_deref().ok_or("missing context")?);
    }
    if std::env::args().nth(2).as_deref() == Some("contact") {
        if client.bidi.is_none() {
            return Err("contact fixture requires BiDi".into());
        }
        return contact_fixture::run(&mut client, context.as_deref().ok_or("missing context")?);
    }
    if std::env::args().nth(2).as_deref() == Some("trusted-input") {
        if client.bidi.is_none() {
            return Err("trusted input fixture requires BiDi".into());
        }
        return trusted_input_fixture::run(
            &mut client,
            context.as_deref().ok_or("missing context")?,
        );
    }
    if std::env::args().nth(2).as_deref() == Some("applicant") {
        if cdp {
            return Err("applicant fixture requires BiDi".into());
        }
        return applicant_fixture::run(&mut client, context.as_deref().ok_or("missing context")?);
    }
    if std::env::args().nth(2).as_deref() == Some("portal") {
        if cdp {
            return Err("portal fixture requires BiDi".into());
        }
        return portal_fixture::run(&mut client, context.as_deref().ok_or("missing context")?);
    }
    let collector = if !cdp {
        client
            .call(
                "network.addDataCollector",
                json!({"dataTypes":["response"],"maxEncodedDataSize":65536}),
            )
            .ok()
            .and_then(|v| v["collector"].as_str().map(str::to_owned))
    } else {
        None
    };
    let url = format!("http://127.0.0.1:{}/", server.port);
    if let Some(context) = &context {
        client.call(
            "browsingContext.navigate",
            json!({"context":context,"url":url,"wait":"complete"}),
        )?;
    } else {
        client.call("Page.navigate", json!({"url":url}))?;
    }
    if guards {
        return guard_probe::run(
            &mut client,
            context.as_deref().ok_or("missing BiDi context")?,
            &url,
            &server,
        );
    }
    let context_isolation = if isolation && !cdp {
        let a = client.call("browser.createUserContext", json!({}))?["userContext"]
            .as_str()
            .ok_or("missing isolated user context")?
            .to_string();
        let b = client.call("browser.createUserContext", json!({}))?["userContext"]
            .as_str()
            .ok_or("missing isolated user context")?
            .to_string();
        let tab_a = client.call(
            "browsingContext.create",
            json!({"type":"tab","userContext":a}),
        )?["context"]
            .as_str()
            .ok_or("missing tab")?
            .to_string();
        let tab_b = client.call(
            "browsingContext.create",
            json!({"type":"tab","userContext":b}),
        )?["context"]
            .as_str()
            .ok_or("missing tab")?
            .to_string();
        for tab in [&tab_a, &tab_b] {
            client.call(
                "browsingContext.navigate",
                json!({"context":tab,"url":url,"wait":"complete"}),
            )?;
        }
        client.call("script.evaluate",json!({"expression":"document.cookie='fixture=A'; true","target":{"context":tab_a},"awaitPromise":false}))?;
        client.call("script.evaluate",json!({"expression":"document.querySelector('input').value='manual-fixture';document.body.style.height='3000px';window.scrollTo(0,200);true","target":{"context":tab_b},"awaitPromise":false}))?;
        client.call("browser.removeUserContext", json!({"userContext":a}))?;
        let preserved=client.call("script.evaluate",json!({"expression":"document.querySelector('input').value==='manual-fixture' && window.scrollY===200 && document.cookie===''","target":{"context":tab_b},"awaitPromise":false}))?["result"]["value"]==true;
        client.call("browser.removeUserContext", json!({"userContext":b}))?;
        Some(preserved)
    } else {
        None
    };
    let evaluate = |client: &mut Client, expression: &str| -> Result<Value> {
        if let Some(context) = &context {
            Ok(client.call(
                "script.evaluate",
                json!({"expression":expression,"target":{"context":context},"awaitPromise":false}),
            )?["result"]["value"]
                .clone())
        } else {
            Ok(client.call(
                "Runtime.evaluate",
                json!({"expression":expression,"returnByValue":true}),
            )?["result"]["value"]
                .clone())
        }
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut dom = Value::Null;
    while Instant::now() < deadline {
        dom = evaluate(
            &mut client,
            "JSON.stringify({script:window.scriptLoaded===true,frames:document.querySelectorAll('iframe').length,frameText:document.querySelector('iframe')?.contentDocument?.getElementById('frame')?.textContent,form:!!document.getElementById('submit')})",
        )?;
        if dom.as_str().is_some_and(|s| {
            s.contains("\"script\":true")
                && s.contains("\"form\":true")
                && s.contains("\"frameText\":\"frame\"")
        }) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let facts: Value = serde_json::from_str(dom.as_str().ok_or("DOM result missing")?)?;
    if facts["script"] != true
        || facts["frames"] != 1
        || facts["form"] != true
        || facts["frameText"] != "frame"
    {
        return Err("DOM/script/frame check failed".into());
    }
    let process_isolation = if std::env::args().nth(2).as_deref() == Some("process-isolation") {
        evaluate(
            &mut client,
            "document.querySelector('input').value='manual-fixture';document.body.style.height='3000px';window.scrollTo(0,200);true",
        )?;
        let mut other = OwnedChild(
            Command::new(std::env::current_exe()?)
                .arg(&kind)
                .arg("baseline")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        );
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut ended = false;
        while Instant::now() < deadline {
            if let Some(status) = other.0.try_wait()? {
                if !status.success() {
                    return Err("isolated second browser probe failed".into());
                }
                ended = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if !ended {
            return Err("isolated second process deadline".into());
        }
        let preserved = evaluate(
            &mut client,
            "document.querySelector('input').value==='manual-fixture' && window.scrollY===200",
        )? == true;
        Some(preserved)
    } else {
        None
    };
    let script_id = client.events.iter().find_map(|e| {
        if cdp
            && e["method"] == "Network.responseReceived"
            && e["params"]["response"]["url"]
                .as_str()
                .is_some_and(|v| v.ends_with("/probe.js"))
        {
            e["params"]["requestId"].as_str().map(str::to_owned)
        } else if !cdp
            && e["method"] == "network.responseCompleted"
            && e["params"]["request"]["url"]
                .as_str()
                .is_some_and(|v| v.ends_with("/probe.js"))
        {
            e["params"]["request"]["request"]
                .as_str()
                .map(str::to_owned)
        } else {
            None
        }
    });
    let script_body = if let Some(request) = script_id {
        if cdp {
            client
                .call("Network.getResponseBody", json!({"requestId":request}))
                .ok()
                .is_some_and(|v| v["body"] == "window.scriptLoaded=true;")
        } else if let Some(collector) = &collector {
            client.call("network.getData",json!({"request":request,"collector":collector,"dataType":"response","disown":true})).ok().is_some_and(|v|v["bytes"]["value"]=="window.scriptLoaded=true;"||v["bytes"]["value"]=="d2luZG93LnNjcmlwdExvYWRlZD10cnVlOw==")
        } else {
            false
        }
    } else {
        false
    };
    let popup =
        evaluate(&mut client, "!!window.open('/frame','fixture-popup')").unwrap_or(json!(false));
    if lost_response {
        evaluate(
            &mut client,
            "document.querySelector('form').action='/lost'; true",
        )?;
    }
    // Exactly one command; no retry even if the POST navigation makes the response ambiguous.
    let _ = evaluate(
        &mut client,
        "document.getElementById('submit').click(); true",
    );
    let deadline = Instant::now() + Duration::from_secs(4);
    while server.posts.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(400));
    let _ = evaluate(&mut client, "document.title");
    let observed_post = client
        .events
        .iter()
        .any(|e| e["params"]["request"]["method"] == "POST");
    let report = json!({"candidate":kind,"protocol":if cdp{"cdp"}else{"bidi"},"python_runtime":false,"node_runtime":false,"isolated_profile":true,"localhost_fixture":true,"dom":facts,"popup_opened":popup,"isolated_context_reset_preserves_other_input_scroll":context_isolation,"separate_process_exit_preserves_other_input_scroll":process_isolation,"passive_script_body":script_body,"bidi_collector_available":collector.is_some(),"submit_commands_sent":1,"lost_response_fixture":lost_response,"post_requests_received":server.posts.load(Ordering::SeqCst),"post_method_event":observed_post,"events_received":client.events.len(),"not_verified":["certificate and Clave","manual/background/lock behavior","production recovery","production adapter cancellation"]});
    println!("{}", serde_json::to_string_pretty(&report)?);
    if server.posts.load(Ordering::SeqCst) != 1 || !observed_post {
        return Err("POST evidence mismatch".into());
    }
    if !cdp {
        let _ = client.call("session.end", json!({}));
    } else {
        let _ = client.call("Browser.close", json!({}));
    }
    Ok(())
}
