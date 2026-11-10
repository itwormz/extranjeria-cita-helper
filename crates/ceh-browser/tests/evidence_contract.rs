use ceh_browser::evidence::{summarize_cookies, summarize_environment};
use serde_json::json;

#[test]
fn cookies_are_reported_by_name_domain_and_expiry_never_by_value() {
    let result = json!({"cookies":[
        {"name":"JSESSIONID","value":{"type":"string","value":"SECRET-SESSION"},"domain":"icp.administracionelectronica.gob.es","path":"/","size":40,"httpOnly":true,"secure":true,"sameSite":"none","expiry":1794400000},
        {"name":"TS01abc","value":{"type":"string","value":"SECRET-TS"},"domain":".administracionelectronica.gob.es","path":"/","httpOnly":false,"secure":true,"sameSite":"lax"},
        {"name":"other","value":{"type":"string","value":"SECRET-OTHER"},"domain":"example.com","path":"/","httpOnly":false,"secure":false,"sameSite":"lax"}]});
    let summary = summarize_cookies(&result);
    assert_eq!(summary.as_array().unwrap().len(), 2, "only the site's domains");
    assert_eq!(summary[0]["name"], "JSESSIONID");
    assert_eq!(summary[0]["expiry"], 1794400000);
    assert_eq!(summary[0]["http_only"], true);
    assert!(!summary.to_string().contains("SECRET"));
}

#[test]
fn the_environment_keeps_only_the_known_fields() {
    let raw = r#"{"visibility":"hidden","has_focus":false,"inner":[1120,700],"outer":[1120,850],"dpr":2,"webdriver":true,"languages":["es-ES","es"],"timezone":"Europe/Madrid","user_agent":"Mozilla/5.0","platform":"MacIntel","cookie":"SECRET","form":{"nie":"SECRET"}}"#;
    let environment = summarize_environment(raw).unwrap();
    assert_eq!(environment["visibility"], "hidden");
    assert_eq!(environment["has_focus"], false);
    assert_eq!(environment["webdriver"], true);
    assert!(!environment.to_string().contains("SECRET"));
    assert!(summarize_environment("not json").is_none());
}

#[test]
fn evidence_scripts_only_read_the_page() {
    let source = include_str!("../src/evidence.rs");
    for forbidden in [".click(", ".submit(", "dispatchEvent", ".value =", ".value=", "location.assign", "location.href =", "localStorage.set", "document.cookie ="] {
        assert!(!source.contains(forbidden), "evidence.rs must not contain {forbidden}");
    }
}
