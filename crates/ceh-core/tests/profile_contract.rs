use ceh_core::profiles::{ProfileCatalog, ProfileDraft, validate_profile};
use serde_json::Value;

#[test]
fn rust_profile_validation_matches_python_golden_cases() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../contracts/fixtures/profile-validation.json"
    ))
    .unwrap();
    let catalog: ProfileCatalog = serde_json::from_value(corpus["catalog"].clone()).unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let draft: ProfileDraft = serde_json::from_value(case["draft"].clone()).unwrap();
        let actual = validate_profile(draft, &catalog);
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            case["expected"],
            "fixture {}",
            case["id"]
        );
    }
}

#[test]
fn every_supported_browser_is_accepted_and_an_unknown_one_is_not() {
    let corpus: Value = serde_json::from_str(include_str!("../../../contracts/fixtures/profile-validation.json")).unwrap();
    let catalog: ProfileCatalog = serde_json::from_value(corpus["catalog"].clone()).unwrap();
    let valid = corpus["cases"].as_array().unwrap().iter().find(|c| c["expected"]["errors"].as_array().is_some_and(Vec::is_empty)).expect("a valid fixture");
    for browser in ["firefox", "chrome", "brave", "edge", "chromium"] {
        let mut draft: ProfileDraft = serde_json::from_value(valid["draft"].clone()).unwrap();
        draft.browser = browser.into();
        let report = serde_json::to_value(validate_profile(draft, &catalog)).unwrap();
        assert!(report["errors"].as_array().unwrap().is_empty(), "{browser}: {report}");
    }
    let mut draft: ProfileDraft = serde_json::from_value(valid["draft"].clone()).unwrap();
    draft.browser = "netscape".into();
    let report = serde_json::to_value(validate_profile(draft, &catalog)).unwrap();
    assert!(report["errors"].to_string().contains("browser must be"), "{report}");
}
