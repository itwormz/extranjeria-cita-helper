use ceh_browser::launch::{LaunchIdentity, TargetOs};
use serde_json::json;
fn seed() -> String {
    json!({"schema":1,"sdk":"0.5.7","browser_major":156,"target_os":"macos","config":{"navigator.userAgent":"Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:156.0) Gecko/20100101 Firefox/156.0","screen.width":1440,"screen.height":900,"humanize":false,"timezone":"UTC"},"preferences":{"security.enterprise_roots.enabled":false,"some.pref":"á🗓"}}).to_string()
}
#[test]
fn sdk_identity_preserves_fields_but_applies_required_browser_policy() {
    let identity = LaunchIdentity::decode(&seed(), 156, TargetOs::Macos).unwrap();
    let env = identity
        .environment("Europe/Madrid", TargetOs::Macos, &[])
        .unwrap();
    let config: String = env
        .iter()
        .filter(|(k, _)| k.starts_with("CAMOU_CONFIG_"))
        .map(|(_, v)| v.as_str())
        .collect();
    let prefs: String = env
        .iter()
        .filter(|(k, _)| k.starts_with("CAMOU_PREFS_"))
        .map(|(_, v)| v.as_str())
        .collect();
    let config: serde_json::Value = serde_json::from_str(&config).unwrap();
    let prefs: serde_json::Value = serde_json::from_str(&prefs).unwrap();
    assert_eq!(config["screen.width"], 1440);
    assert_eq!(config["timezone"], "Europe/Madrid");
    assert_eq!(config["humanize"], true);
    assert_eq!(config["humanize:maxTime"], 1.5);
    assert_eq!(config["allowMainWorld"], true);
    for name in [
        "security.enterprise_roots.enabled",
        "security.osclientcerts.autoload",
        "dom.disable_window_flip",
        "browser.tabs.loadDivertedInBackground",
    ] {
        assert_eq!(prefs[name], true);
    }
    assert_eq!(prefs["intl.locale.requested"], "es-ES");
    assert_eq!(prefs["some.pref"], "á🗓");
}
#[test]
fn wrong_sdk_browser_os_and_unsafe_payloads_fail_closed() {
    for (field, value) in [
        ("schema", json!(2)),
        ("sdk", json!("0.5.6")),
        ("browser_major", json!(150)),
        ("target_os", json!("windows")),
        ("config", json!([])),
        ("preferences", json!({"not-a-pref":{}})),
    ] {
        let mut data: serde_json::Value = serde_json::from_str(&seed()).unwrap();
        data[field] = value;
        assert!(
            LaunchIdentity::decode(&data.to_string(), 156, TargetOs::Macos).is_err(),
            "{field}"
        );
    }
    assert!(LaunchIdentity::decode(&seed(), 155, TargetOs::Macos).is_err());
    let identity = LaunchIdentity::decode(&seed(), 156, TargetOs::Macos).unwrap();
    assert!(
        identity
            .environment("bad\nzone", TargetOs::Macos, &[])
            .is_err()
    );
    assert!(
        identity
            .environment("Europe/Madrid", TargetOs::Windows, &[])
            .is_err()
    );
}
#[test]
fn windows_chunks_are_ascii_bounded_and_reassemble_without_loss() {
    let mut data: serde_json::Value = serde_json::from_str(&seed()).unwrap();
    data["target_os"] = json!("windows");
    data["config"]["navigator.userAgent"] =
        json!("Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:156.0) Gecko/20100101 Firefox/156.0");
    data["config"]["fonts"] = json!(["á🗓".repeat(1300)]);
    let identity = LaunchIdentity::decode(&data.to_string(), 156, TargetOs::Windows).unwrap();
    let env = identity
        .environment("Europe/Madrid", TargetOs::Windows, &[])
        .unwrap();
    assert!(env.len() > 3);
    assert!(env.iter().all(|(_, v)| v.is_ascii() && v.len() <= 2047));
    let all: String = env
        .iter()
        .filter(|(k, _)| k.starts_with("CAMOU_CONFIG_"))
        .map(|(_, v)| v.as_str())
        .collect();
    let config: serde_json::Value = serde_json::from_str(&all).unwrap();
    assert_eq!(config["fonts"], data["config"]["fonts"]);
}
