//! Read-only evidence for the diagnostics timeline: the page environment (is the window visible and
//! focused, what does the page see of the browser) and the NAMES of the cookies the browser holds for the
//! site. Page scripts only read; values of cookies, fields and storage are never returned.
use crate::bidi::BidiConnection;
use serde_json::{Value, json};

const SITE_DOMAIN: &str = "administracionelectronica.gob.es";
const ENVIRONMENT_SCRIPT: &str = "JSON.stringify({visibility:document.visibilityState,has_focus:document.hasFocus(),\
inner:[window.innerWidth,window.innerHeight],outer:[window.outerWidth,window.outerHeight],dpr:window.devicePixelRatio,\
webdriver:navigator.webdriver,languages:navigator.languages,timezone:Intl.DateTimeFormat().resolvedOptions().timeZone,\
user_agent:navigator.userAgent,platform:navigator.platform})";
const ENVIRONMENT_KEYS: [&str; 10] = [
    "visibility", "has_focus", "inner", "outer", "dpr", "webdriver", "languages", "timezone", "user_agent", "platform",
];

/// Keep only the known environment fields of the script's JSON text.
pub fn summarize_environment(raw: &str) -> Option<Value> {
    let parsed: Value = serde_json::from_str(raw).ok()?;
    let object = parsed.as_object()?;
    Some(Value::Object(
        ENVIRONMENT_KEYS.iter().filter_map(|key| object.get(*key).map(|v| ((*key).to_owned(), v.clone()))).collect(),
    ))
}
/// `[{name, domain, path, expiry, secure, http_only, same_site}]` for the site's cookies, never a value.
pub fn summarize_cookies(result: &Value) -> Value {
    Value::Array(
        result["cookies"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|cookie| cookie["domain"].as_str().is_some_and(|d| d.trim_start_matches('.').ends_with(SITE_DOMAIN)))
            .map(|cookie| {
                json!({
                    "name": cookie["name"], "domain": cookie["domain"], "path": cookie["path"],
                    "expiry": cookie["expiry"], "secure": cookie["secure"], "http_only": cookie["httpOnly"],
                    "same_site": cookie["sameSite"],
                })
            })
            .collect(),
    )
}
/// The page environment of `context`; `None` when the page cannot be read (it never fails the search).
pub fn read_environment(connection: &mut BidiConnection, context: &str) -> Option<Value> {
    let result = connection
        .call("script.evaluate", json!({"expression":ENVIRONMENT_SCRIPT,"target":{"context":context},"awaitPromise":false}))
        .ok()?;
    summarize_environment(result["result"]["value"].as_str()?)
}
pub fn read_cookie_names(connection: &mut BidiConnection) -> Option<Value> {
    let result = connection.call("storage.getCookies", json!({})).ok()?;
    Some(summarize_cookies(&result))
}
