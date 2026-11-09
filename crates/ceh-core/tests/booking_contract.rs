use ceh_core::booking::{booking_lines, normalize_booking_details, parse_offers};
use serde_json::Value;
#[test]
fn rust_public_booking_data_matches_python_goldens() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/fixtures/booking-offers.json"
    ))
    .unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let actual = match case["operation"].as_str().unwrap() {
            "parse" => serde_json::to_value(parse_offers(&case["input"])).unwrap(),
            "normalize" => serde_json::to_value(normalize_booking_details(&case["input"])).unwrap(),
            "lines" => serde_json::to_value(booking_lines(&case["input"])).unwrap(),
            _ => panic!("Unknown fixture operation"),
        };
        assert_eq!(actual, case["expected"], "fixture {}", case["id"]);
    }
}
