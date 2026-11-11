#[test]
fn selected_intervals_match_python_union_counts_and_evidence() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/fixtures/selection.json"
    ))
    .unwrap();
    let timelines: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/fixtures/timeline.json"
    ))
    .unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let input = timelines["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == case["timeline_case"])
            .unwrap();
        let rows = case["indices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|index| input["expected"][index.as_u64().unwrap() as usize].clone())
            .collect::<Vec<_>>();
        let actual = ceh_store::selection::selection_summary(
            &rows,
            input["input"]["rows"].as_array().unwrap(),
        )
        .unwrap();
        assert_eq!(actual, case["expected"], "{}", case["id"]);
    }
}
