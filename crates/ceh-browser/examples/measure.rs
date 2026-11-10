//! Measures what a page sees from the app-launched Firefox and which trusted-input method selects an
//! option. Only talks to a local 127.0.0.1 page served from the fixture; changes nothing outside a
//! temporary profile. Usage: cargo run -p ceh-browser --example measure --features bidi [-- --headed]
use ceh_browser::firefox::{FirefoxInstallation, OwnedFirefox, ProfileLease};
use serde_json::{Value, json};
use std::{io::{Read, Write}, net::TcpListener, sync::atomic::AtomicBool, time::Duration};

const PAGE: &str = include_str!("../tests/fixtures/trusted-events.html");
const DOWN: &str = "\u{E015}";
const ENTER: &str = "\u{E007}";
const TAB: &str = "\u{E004}";
const ESCAPE: &str = "\u{E00C}";

struct Lab {
    browser: OwnedFirefox,
    url: String,
}
impl Lab {
    fn call(&mut self, method: &str, params: Value) -> Value {
        let result = self.browser.connection().call(method, params).unwrap_or_else(|e| panic!("{method}: {e:?}"));
        // The page produces many protocol events; nothing here consumes them.
        let _ = self.browser.connection().take_events();
        result
    }
    fn chord(&mut self, modifier: &str, key: &str) {
        let context = self.browser.context().to_owned();
        self.call("input.performActions", json!({"context":context,"actions":[{"type":"key","id":"k","actions":[
            {"type":"keyDown","value":modifier},{"type":"keyDown","value":key},{"type":"keyUp","value":key},{"type":"keyUp","value":modifier}]}]}));
        std::thread::sleep(Duration::from_millis(120));
    }
    fn eval(&mut self, expression: &str) -> Value {
        let context = self.browser.context().to_owned();
        let result = self.call("script.evaluate", json!({"expression":expression,"target":{"context":context},"awaitPromise":true,"resultOwnership":"none"}));
        if result["type"] != "success" {
            panic!("script failed: {result}");
        }
        result["result"].clone()
    }
    fn json(&mut self, expression: &str) -> Value {
        let value = self.eval(&format!("JSON.stringify({expression})"));
        serde_json::from_str(value["value"].as_str().unwrap_or("null")).unwrap_or(Value::Null)
    }
    fn reload(&mut self) {
        let context = self.browser.context().to_owned();
        let url = self.url.clone();
        self.call("browsingContext.navigate", json!({"context":context,"url":url,"wait":"complete"}));
        std::thread::sleep(Duration::from_millis(300));
    }
    fn element(&mut self, selector: &str) -> String {
        let context = self.browser.context().to_owned();
        let node = self.call("script.evaluate", json!({"expression":format!("document.querySelector({})", json!(selector)),"target":{"context":context},"awaitPromise":false}));
        node["result"]["sharedId"].as_str().unwrap_or_else(|| panic!("no node {selector}: {node}")).to_owned()
    }
    fn click(&mut self, selector: &str) {
        let shared = self.element(selector);
        let context = self.browser.context().to_owned();
        self.call("input.performActions", json!({"context":context,"actions":[{"type":"pointer","id":"m","parameters":{"pointerType":"mouse"},"actions":[
            {"type":"pointerMove","duration":120,"x":0,"y":0,"origin":{"type":"element","element":{"sharedId":shared}}},
            {"type":"pointerDown","button":0},{"type":"pause","duration":60},{"type":"pointerUp","button":0}]}]}));
        std::thread::sleep(Duration::from_millis(150));
    }
    fn keys(&mut self, values: &[&str]) {
        let context = self.browser.context().to_owned();
        let actions: Vec<Value> = values.iter().flat_map(|v| [json!({"type":"keyDown","value":v}), json!({"type":"keyUp","value":v})]).collect();
        self.call("input.performActions", json!({"context":context,"actions":[{"type":"key","id":"k","actions":actions}]}));
        std::thread::sleep(Duration::from_millis(120));
    }
    fn state(&mut self) -> Value {
        self.json("({selected: document.getElementById('txtPaisNac').selectedOptions[0].label, events: window.__events.filter(e=>e.type==='change'||e.type==='input').map(e=>({type:e.type,id:e.id,trusted:e.trusted})), untrusted: window.__events.filter(e=>!e.trusted).length, active: document.activeElement && document.activeElement.id, keys: window.__events.filter(e=>e.type==='keydown').map(e=>e.id+':'+e.key)})")
    }
}

fn serve() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buffer = [0u8; 2048];
            let _ = stream.read(&mut buffer);
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", PAGE.len(), PAGE);
        }
    });
    format!("http://{address}/")
}

fn main() {
    let headed = std::env::args().any(|a| a == "--headed");
    let installation = FirefoxInstallation::discover().expect("Firefox is not installed");
    let runtime = tempfile::tempdir().unwrap();
    let lease = ProfileLease::temporary(runtime.path()).unwrap();
    let browser = OwnedFirefox::start(&installation, lease, !headed, &AtomicBool::new(false)).unwrap();
    let mut lab = Lab { browser, url: serve() };
    println!("# measurement — Firefox {} ({})", installation.version(), if headed { "headed" } else { "headless" });
    lab.reload();
    println!("\n## what the page sees\n{}", lab.json("({webdriver:navigator.webdriver,languages:navigator.languages,timezone:Intl.DateTimeFormat().resolvedOptions().timeZone,plugins:navigator.plugins.length,hardwareConcurrency:navigator.hardwareConcurrency,platform:navigator.platform,userAgent:navigator.userAgent})"));

    lab.reload();
    lab.click("#rdbTipoDocNie");
    println!("\n## trusted pointer on a radio\n{}", lab.json("({checked:document.getElementById('rdbTipoDocNie').checked,events:window.__events.map(e=>[e.type,e.trusted])})"));

    lab.reload();
    lab.click("#txtIdCitado");
    let text: Vec<String> = "Y1234567X".chars().map(String::from).collect();
    lab.keys(&text.iter().map(String::as_str).collect::<Vec<_>>());
    println!("\n## trusted keys in a text field\n{}", lab.json("({value:document.getElementById('txtIdCitado').value,input:window.__events.filter(e=>e.type==='input').map(e=>e.trusted),untrusted:window.__events.filter(e=>!e.trusted).length})"));
    lab.chord("\u{E03D}", "a");
    lab.keys(&["\u{E003}"]);
    println!("Meta+A then Backspace leaves: {}", lab.json("document.getElementById('txtIdCitado').value"));

    // option indexes: 0 Seleccione, 1 ALEMANIA, 2 ESPAÑA, 3 UCRANIA, 4 UCRANIA (EX)
    let methods: [(&str, &str, Box<dyn Fn(&mut Lab)>); 9] = [
        ("click opens list, ArrowDown x3, Enter", "UCRANIA", Box::new(|lab| { lab.click("#txtPaisNac"); lab.keys(&[DOWN, DOWN, DOWN, ENTER]); })),
        ("click opens list, Escape, ArrowDown x3 (closed)", "UCRANIA", Box::new(|lab| { lab.click("#txtPaisNac"); lab.keys(&[ESCAPE]); lab.keys(&[DOWN, DOWN, DOWN]); })),
        ("Tab from previous field, ArrowDown x3 (closed)", "UCRANIA", Box::new(|lab| { lab.click("#txtDesCitado"); lab.keys(&[TAB]); lab.keys(&[DOWN, DOWN, DOWN]); })),
        ("Tab from previous field, type-ahead 'UCRANIA'", "UCRANIA", Box::new(|lab| { lab.click("#txtDesCitado"); lab.keys(&[TAB]); lab.keys(&["U", "C", "R", "A", "N", "I", "A"]); })),
        ("Tab from previous field, type-ahead 'ESPAÑA' (accent)", "ESPAÑA", Box::new(|lab| { lab.click("#txtDesCitado"); lab.keys(&[TAB]); lab.keys(&["E", "S", "P", "A", "Ñ", "A"]); })),
        ("Tab from previous field, type-ahead 'UCRANIA (EX)' (longer sibling)", "UCRANIA (EX)", Box::new(|lab| { lab.click("#txtDesCitado"); lab.keys(&[TAB]); lab.keys(&["U", "C", "R", "A", "N", "I", "A", " ", "(", "E", "X", ")"]); })),
        ("click opens list, Escape, type-ahead 'UCRANIA'", "UCRANIA", Box::new(|lab| { lab.click("#txtPaisNac"); lab.keys(&[ESCAPE]); lab.keys(&["U", "C", "R", "A", "N", "I", "A"]); })),
        ("click opens list, type-ahead 'UCRANIA', Enter", "UCRANIA", Box::new(|lab| { lab.click("#txtPaisNac"); lab.keys(&["U", "C", "R", "A", "N", "I", "A", ENTER]); })),
        ("Tab from previous field, Space, ArrowDown x3, Enter", "UCRANIA", Box::new(|lab| { lab.click("#txtDesCitado"); lab.keys(&[TAB]); lab.keys(&[" "]); lab.keys(&[DOWN, DOWN, DOWN, ENTER]); })),
    ];
    for (name, expected, method) in &methods {
        let mut results = Vec::new();
        for _ in 0..20 {
            lab.reload();
            method(&mut lab);
            results.push(lab.state());
        }
        let ok = results.iter().filter(|r| r["selected"] == *expected).count();
        let trusted_changes = results.iter().filter(|r| r["events"].as_array().is_some_and(|e| e.iter().any(|x| x["type"] == "change" && x["trusted"] == true))).count();
        let untrusted = results.iter().filter(|r| r["untrusted"].as_u64().unwrap_or(0) > 0).count();
        println!("\n## select: {name}\nselected {expected} {ok}/20, trusted change {trusted_changes}/20, runs with untrusted events {untrusted}/20\nsample: {}", results[0]);
    }
    let _ = lab.browser.stop();
}
