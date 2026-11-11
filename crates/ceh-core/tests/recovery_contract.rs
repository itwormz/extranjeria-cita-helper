use ceh_core::{model::*, recovery::*, timings::AssistTimings};
use serde_json::{Value, json};
#[test]
fn recovery_matches_python_fixtures() {
    let data: Value = serde_json::from_str(include_str!(
        "../../../contracts/fixtures/flow-actions.json"
    ))
    .unwrap();
    let cases = data["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["options"]["operation"] == "recover");
    let mut count = 0;
    for case in cases {
        count += 1;
        let options = &case["options"];
        let kind: PageKind = serde_json::from_value(case["kind"].clone()).unwrap();
        let observed = Observation {
            document: DocumentId::new("doc").unwrap(),
            kind,
            method: if options["method"] == "POST" {
                RequestMethod::Post
            } else {
                RequestMethod::Get
            },
            operation_code: false,
            variant: String::new(),
        };
        let facts = RecoveryFacts {
            official_site: true,
            endpoint: RecoveryEndpoint::Availability,
            last_action: options["last_action"]
                .as_str()
                .map(|s| serde_json::from_value(json!(s)).unwrap()),
            own_control: kind == PageKind::Expired && options["button_missing"] != true,
            ..Default::default()
        };
        let plan = plan_recovery(&observed, &facts, &AssistTimings::default(), 0, 0.0).unwrap();
        let seconds = if plan.delay_seconds.fract() == 0.0 {
            json!(plan.delay_seconds as u64)
        } else {
            json!(plan.delay_seconds)
        };
        let mut effects = vec![json!(["wait", seconds])];
        if options["wait_completed"] != false {
            effects.push(match plan.after {
                RecoveryAction::Reload => json!(["reload"]),
                RecoveryAction::Hold => json!(["hold"]),
                RecoveryAction::ActivateOwnControl => json!(["activate", "salir"]),
                RecoveryAction::GetCurrent => json!(["goto", "current"]),
                RecoveryAction::GetEntry => json!(["goto", "entry"]),
                RecoveryAction::Restart {
                    clear_profile,
                    reopen,
                } => json!(["restart", clear_profile, reopen]),
            });
        }
        assert_eq!(json!(effects), case["expected"], "{}", case["id"]);
    }
    assert_eq!(count, 9);
}
#[test]
fn unknown_method_and_personal_posts_fail_closed() {
    for method in [RequestMethod::Unknown, RequestMethod::Post] {
        let observed = Observation {
            document: DocumentId::new("doc").unwrap(),
            kind: PageKind::ServerError,
            method,
            operation_code: false,
            variant: String::new(),
        };
        let facts = RecoveryFacts {
            official_site: true,
            endpoint: RecoveryEndpoint::Availability,
            last_action: Some(ActionKind::Applicant),
            ..Default::default()
        };
        assert_eq!(
            plan_recovery(&observed, &facts, &AssistTimings::default(), 0, 0.0)
                .unwrap()
                .after,
            RecoveryAction::Hold
        );
    }
}

#[test]
fn pending_contact_wins_over_previous_search_action() {
    let o = Observation {
        document: DocumentId::new("d").unwrap(),
        kind: PageKind::ServerError,
        method: RequestMethod::Post,
        operation_code: false,
        variant: String::new(),
    };
    let facts = RecoveryFacts {
        official_site: true,
        endpoint: RecoveryEndpoint::Availability,
        last_action: Some(ActionKind::Request),
        pending_action: Some(ActionKind::Contact),
        ..Default::default()
    };
    assert_eq!(
        plan_recovery(&o, &facts, &AssistTimings::default(), 0, 0.0)
            .unwrap()
            .after,
        RecoveryAction::Hold
    );
}
