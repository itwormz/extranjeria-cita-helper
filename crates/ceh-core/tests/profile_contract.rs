use ceh_core::profiles::{ProfileCatalog, ProfileDraft, validate_profile};
use serde_json::Value;

#[test]
fn rust_profile_validation_matches_python_golden_cases() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/fixtures/profile-validation.json"
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
