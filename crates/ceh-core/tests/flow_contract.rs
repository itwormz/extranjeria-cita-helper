use ceh_core::{flow::*, model::*};
fn observation(kind: PageKind) -> Observation {
    Observation {
        document: DocumentId::new("doc").unwrap(),
        kind,
        method: RequestMethod::Get,
        operation_code: false,
        variant: String::new(),
    }
}
fn controls() -> PageFacts {
    PageFacts {
        recovery: ceh_core::recovery::RecoveryFacts {
            own_control: true,
            ..Default::default()
        },
        ..Default::default()
    }
}
fn engine() -> FlowEngine {
    FlowEngine::new(SessionId::new("session").unwrap(), SearchOptions::default())
}
#[test]
fn filling_requires_fresh_observation_before_submit() {
    let mut e = engine();
    let o = observation(PageKind::Applicant);
    let effects = e.observe(o.clone(), 0).unwrap();
    let FlowEffect::Prepare { guard, action } = &effects[0] else {
        panic!("missing preparation")
    };
    e.complete_preparation(guard, *action, PreparationOutcome::Completed)
        .unwrap();
    assert!(matches!(
        &e.observe(o.clone(), 1).unwrap()[0],
        FlowEffect::Activate {
            action: ActionKind::Applicant,
            ..
        }
    ));
    assert_eq!(e.observe(o, 2).unwrap(), vec![FlowEffect::Hold]);
}
#[test]
fn stage_change_cancels_prepared_submit() {
    let mut e = engine();
    e.observe(observation(PageKind::Applicant), 0).unwrap();
    assert_eq!(
        e.observe(observation(PageKind::Booking), 1).unwrap(),
        vec![FlowEffect::Handoff, FlowEffect::Alert]
    );
    assert!(
        e.observe(observation(PageKind::Province), 2)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn waits_do_not_repeat_or_fire_early() {
    let mut e = engine();
    let o = observation(PageKind::ApplicantNoSlots);
    assert_eq!(
        e.observe(o.clone(), 0).unwrap(),
        vec![FlowEffect::WaitUntil(30_000)]
    );
    assert!(e.observe(o.clone(), 29_999).unwrap().is_empty());
    assert!(matches!(
        &e.observe(o.clone(), 30_000).unwrap()[0],
        FlowEffect::Reload { .. }
    ));
    assert_eq!(e.observe(o, 30_001).unwrap(), vec![FlowEffect::Hold]);
}
#[test]
fn continue_option_is_independent_from_result_option() {
    let mut e = FlowEngine::new(
        SessionId::new("s").unwrap(),
        SearchOptions {
            applicant_no_slots_mode: ApplicantNoSlotsMode::Continue,
            no_slots_mode: NoSlotsMode::Salir,
        },
    );
    assert!(matches!(
        &e.observe(observation(PageKind::ApplicantNoSlots), 0)
            .unwrap()[0],
        FlowEffect::Prepare { .. }
    ));
}
#[test]
fn changed_document_does_not_submit_old_preparation() {
    let mut e = engine();
    let o = observation(PageKind::Contact);
    e.observe(o.clone(), 0).unwrap();
    let mut next = o;
    next.document = DocumentId::new("new").unwrap();
    assert!(e.observe(next.clone(), 1).unwrap().is_empty());
    assert!(matches!(
        &e.observe(next, 2).unwrap()[0],
        FlowEffect::Prepare { .. }
    ));
}
#[test]
fn unknown_method_is_not_safe_for_applicant_reload() {
    let mut e = engine();
    let mut o = observation(PageKind::ApplicantNoSlots);
    o.method = RequestMethod::Unknown;
    assert_eq!(e.observe(o, 0).unwrap(), vec![FlowEffect::Hold]);
}

#[test]
fn returning_to_already_submitted_document_does_not_refill() {
    let mut e = engine();
    let o = observation(PageKind::Applicant);
    let effects = e.observe(o.clone(), 0).unwrap();
    let FlowEffect::Prepare { guard, action } = &effects[0] else {
        panic!("missing preparation")
    };
    e.complete_preparation(guard, *action, PreparationOutcome::Completed)
        .unwrap();
    e.observe(o.clone(), 1).unwrap();
    e.observe(observation(PageKind::RequestMenu), 2).unwrap();
    assert_eq!(e.observe(o, 3).unwrap(), vec![FlowEffect::Hold]);
}
#[test]
fn method_changed_during_wait_must_not_replay_post() {
    let mut e = engine();
    let mut o = observation(PageKind::ApplicantNoSlots);
    e.observe(o.clone(), 0).unwrap();
    o.method = RequestMethod::Post;
    assert_eq!(e.observe(o, 30_000).unwrap(), vec![FlowEffect::Hold]);
}

#[test]
fn result_salir_and_reload_follow_independent_setting() {
    let o = observation(PageKind::RequestNoSlots);
    let mut e = engine();
    assert_eq!(
        e.observe(o.clone(), 0).unwrap(),
        vec![FlowEffect::WaitUntil(30_000)]
    );
    assert!(matches!(
        &e.observe_with_facts(o.clone(), controls(), 30_000).unwrap()[0],
        FlowEffect::Activate {
            action: ActionKind::NoSlotsSalir,
            ..
        }
    ));
    let mut e = FlowEngine::new(
        SessionId::new("s").unwrap(),
        SearchOptions {
            no_slots_mode: NoSlotsMode::Reload,
            ..Default::default()
        },
    );
    e.observe(o.clone(), 0).unwrap();
    assert!(matches!(
        &e.observe(o, 30_000).unwrap()[0],
        FlowEffect::Reload { .. }
    ));
}
#[test]
fn reload_delay_respects_profile_range() {
    let timings =
        ceh_core::timings::parse_timings(&serde_json::json!({"reload_seconds":[45,90]})).unwrap();
    let mut e = FlowEngine::with_timings(
        SessionId::new("s").unwrap(),
        SearchOptions::default(),
        timings.clone(),
        60.0,
    )
    .unwrap();
    assert_eq!(
        e.observe(observation(PageKind::ApplicantNoSlots), 100)
            .unwrap(),
        vec![FlowEffect::WaitUntil(60_100)]
    );
    assert!(
        FlowEngine::with_timings(
            SessionId::new("s").unwrap(),
            SearchOptions::default(),
            timings,
            30.0
        )
        .is_err()
    );
}

#[test]
fn observation_alone_cannot_claim_fill_completed() {
    let mut e = engine();
    let o = observation(PageKind::Applicant);
    e.observe(o.clone(), 0).unwrap();
    assert!(e.observe(o, 1).unwrap().is_empty());
}

#[test]
fn failed_or_unknown_fill_cannot_submit() {
    for outcome in [PreparationOutcome::Failed, PreparationOutcome::Unknown] {
        let mut e = engine();
        let o = observation(PageKind::Contact);
        let effects = e.observe(o.clone(), 0).unwrap();
        let FlowEffect::Prepare { guard, action } = &effects[0] else {
            panic!("missing preparation")
        };
        e.complete_preparation(guard, *action, outcome).unwrap();
        assert_eq!(e.observe(o, 1).unwrap(), vec![FlowEffect::Hold]);
    }
}
#[test]
fn completion_from_old_page_is_rejected() {
    let mut e = engine();
    let effects = e.observe(observation(PageKind::Applicant), 0).unwrap();
    let FlowEffect::Prepare { guard, action } = &effects[0] else {
        panic!("missing preparation")
    };
    e.observe(observation(PageKind::Booking), 1).unwrap();
    assert_eq!(
        e.complete_preparation(guard, *action, PreparationOutcome::Completed),
        Err(FlowError::StaleCompletion)
    );
}

#[test]
fn normal_flow_intents_match_existing_python_fixtures() {
    use serde_json::{Value, json};
    let data: Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/fixtures/flow-actions.json"
    ))
    .unwrap();
    let cases = data["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["options"]["operation"] != "recover");
    assert_eq!(cases.clone().count(), 31);
    for case in cases {
        let options = &case["options"];
        let kind: PageKind = serde_json::from_value(case["kind"].clone()).unwrap();
        let mut e = FlowEngine::new(
            SessionId::new("golden").unwrap(),
            SearchOptions {
                applicant_no_slots_mode: if options["applicant_mode"] == "continue" {
                    ApplicantNoSlotsMode::Continue
                } else {
                    ApplicantNoSlotsMode::Wait
                },
                no_slots_mode: if options["result_mode"] == "reload" {
                    NoSlotsMode::Reload
                } else {
                    NoSlotsMode::Salir
                },
            },
        );
        let mut o = observation(kind);
        if options["method"] == "POST" {
            o.method = RequestMethod::Post;
        }
        let facts = PageFacts {
            recovery: ceh_core::recovery::RecoveryFacts {
                official_site: true,
                own_control: options["button_missing"] != true,
                endpoint: if options["endpoint"] == "acEntrada" {
                    ceh_core::recovery::RecoveryEndpoint::ApplicantEntry
                } else {
                    ceh_core::recovery::RecoveryEndpoint::Availability
                },
                last_action: options["last_action"]
                    .as_str()
                    .map(|a| serde_json::from_value(json!(a)).unwrap()),
                ..Default::default()
            },
            office_count: options["office_count"].as_u64().unwrap_or(0) as usize,
            authentication: if options["access_mode"] == "clave" {
                AuthenticationMode::Clave
            } else {
                AuthenticationMode::Certificate
            },
            ..Default::default()
        };
        if options["attempted"].is_string() {
            let first = e.observe_with_facts(o.clone(), facts, 0).unwrap();
            if let [FlowEffect::Prepare { guard, action }] = first.as_slice() {
                e.complete_preparation(guard, *action, PreparationOutcome::Completed)
                    .unwrap();
                e.observe_with_facts(o.clone(), facts, 0).unwrap();
            }
        }
        let mut queue = e.observe_with_facts(o.clone(), facts, 0).unwrap();
        let mut actual = vec![];
        let mut steps = 0;
        while !queue.is_empty() {
            steps += 1;
            assert!(steps < 5, "unbounded fixture execution");
            let mut follow = vec![];
            for effect in queue {
                match effect {
                    FlowEffect::Silence
                    | FlowEffect::SelectClaveMethod { .. }
                    | FlowEffect::AcceptCookies { .. }
                    | FlowEffect::SelectCertificate { .. } => {
                        panic!("unexpected auth-wait effect in fixture")
                    }
                    FlowEffect::FetchApplicant { .. } => actual.push(json!(["goto", "applicant"])),
                    FlowEffect::ReobserveAt(deadline) => {
                        // Python reference intentionally omits wait_for_timeout polling.
                        // Unknown performs its bounded reread; loading returns to outer loop.
                        if kind == PageKind::Unknown {
                            follow = e.observe_with_facts(o.clone(), facts, deadline).unwrap();
                        }
                    }
                    FlowEffect::Recover { .. } => {
                        panic!("unexpected recovery in normal flow fixture")
                    }
                    FlowEffect::EnterAuthentication { mode, .. } => actual.push(json!([
                        "auth",
                        if mode == AuthenticationMode::Certificate {
                            "certificate"
                        } else {
                            "clave"
                        }
                    ])),
                    FlowEffect::Prepare { guard, action } => {
                        let procedure = options["procedure"].as_str().unwrap_or("huellas");
                        actual.push(match action {
                            ActionKind::Province => json!(["select_province"]),
                            ActionKind::Procedure => json!(["select_procedure", procedure]),
                            ActionKind::Applicant => json!([
                                "fill_applicant",
                                options["fill_mode"].as_str().unwrap_or("manual"),
                                procedure
                            ]),
                            ActionKind::Contact => json!(["fill_contact"]),
                            ActionKind::Office => json!(["select_office", "1"]),
                            _ => panic!("unexpected preparation"),
                        });
                        // The synthetic browser confirms only preparation; the following
                        // observation is still required before any activation is issued.
                        e.complete_preparation(&guard, action, PreparationOutcome::Completed)
                            .unwrap();
                        let mut next = o.clone();
                        if options["document_changed"] == true {
                            next.document = DocumentId::new("changed").unwrap();
                            next.kind = PageKind::RequestMenu;
                        }
                        follow = e.observe_with_facts(next, facts, 1).unwrap();
                    }
                    FlowEffect::Activate { action, .. } => actual.push(match action {
                        ActionKind::NoSlotsSalir | ActionKind::PostContactRestart => {
                            json!(["activate", "salir"])
                        }
                        ActionKind::Office => json!(["activate", "continue"]),
                        _ => json!(["click", serde_json::to_value(action).unwrap()]),
                    }),
                    FlowEffect::WaitUntil(deadline) => {
                        actual.push(json!(["wait", deadline / 1000]));
                        if options["wait_completed"] != false {
                            follow = e.observe_with_facts(o.clone(), facts, deadline).unwrap();
                        }
                    }
                    FlowEffect::Reload { .. } => actual.push(json!(["reload"])),
                    FlowEffect::Hold => actual.push(json!(["hold"])),
                    FlowEffect::Handoff => actual.push(json!(["handoff"])),
                    FlowEffect::Alert => actual.push(json!(["alert", true])),
                }
            }
            queue = follow;
        }
        assert_eq!(json!(actual), case["expected"], "{}", case["id"]);
    }
}

#[test]
fn office_choice_is_temporary_but_booking_handoff_is_permanent() {
    let mut e = engine();
    let o = observation(PageKind::Office);
    assert_eq!(
        e.observe_with_facts(
            o.clone(),
            PageFacts {
                office_count: 2,
                ..Default::default()
            },
            0
        )
        .unwrap(),
        vec![FlowEffect::Alert]
    );
    assert!(
        e.observe_with_facts(
            o,
            PageFacts {
                office_count: 2,
                ..Default::default()
            },
            1
        )
        .unwrap()
        .is_empty()
    );
    assert!(matches!(
        e.observe(observation(PageKind::Contact), 2)
            .unwrap()
            .as_slice(),
        [
            FlowEffect::Silence,
            FlowEffect::Prepare {
                action: ActionKind::Contact,
                ..
            }
        ]
    ));
}
#[test]
fn single_office_requires_selection_completion_and_fresh_observation() {
    let mut e = engine();
    let o = observation(PageKind::Office);
    let effects = e
        .observe_with_facts(
            o.clone(),
            PageFacts {
                office_count: 1,
                ..Default::default()
            },
            0,
        )
        .unwrap();
    let FlowEffect::Prepare {
        guard,
        action: ActionKind::Office,
    } = &effects[0]
    else {
        panic!("missing office selection")
    };
    e.complete_preparation(guard, ActionKind::Office, PreparationOutcome::Completed)
        .unwrap();
    assert!(matches!(
        &e.observe_with_facts(
            o,
            PageFacts {
                office_count: 1,
                ..Default::default()
            },
            1
        )
        .unwrap()[0],
        FlowEffect::Activate {
            action: ActionKind::Office,
            ..
        }
    ));
}
#[test]
fn changing_office_options_after_selection_cancels_automatic_activation() {
    let mut e = engine();
    let o = observation(PageKind::Office);
    let effects = e
        .observe_with_facts(
            o.clone(),
            PageFacts {
                office_count: 1,
                ..Default::default()
            },
            0,
        )
        .unwrap();
    let FlowEffect::Prepare { guard, action } = &effects[0] else {
        panic!("missing office selection")
    };
    e.complete_preparation(guard, *action, PreparationOutcome::Completed)
        .unwrap();
    assert_eq!(
        e.observe_with_facts(
            o,
            PageFacts {
                office_count: 2,
                ..Default::default()
            },
            1
        )
        .unwrap(),
        vec![FlowEffect::Alert]
    );
}

#[test]
fn authentication_intent_uses_explicit_profile_method() {
    for mode in [AuthenticationMode::Certificate, AuthenticationMode::Clave] {
        let mut e = engine();
        let o = observation(PageKind::Auth);
        assert!(
            matches!(&e.observe_with_facts(o,PageFacts{authentication:mode,..Default::default()},0).unwrap()[0],FlowEffect::EnterAuthentication{mode:actual,..} if *actual==mode)
        );
    }
}

#[test]
fn recovery_wait_is_cancelled_by_booking_and_never_resumes() {
    let mut e = engine();
    let o = observation(PageKind::Blocked);
    assert_eq!(
        e.observe(o, 0).unwrap(),
        vec![FlowEffect::WaitUntil(30_000)]
    );
    assert_eq!(
        e.observe(observation(PageKind::Booking), 10).unwrap(),
        vec![FlowEffect::Handoff, FlowEffect::Alert]
    );
    assert!(
        e.observe(observation(PageKind::Blocked), 30_000)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn blocked_restart_requires_full_cooldown() {
    let mut e = engine();
    let o = observation(PageKind::Blocked);
    e.observe(o.clone(), 0).unwrap();
    assert!(e.observe(o.clone(), 29_999).unwrap().is_empty());
    assert!(matches!(
        &e.observe(o, 30_000).unwrap()[0],
        FlowEffect::Recover {
            action: ceh_core::recovery::RecoveryAction::Restart {
                clear_profile: true,
                reopen: false
            },
            ..
        }
    ));
}

#[test]
fn changed_response_during_recovery_cannot_replay_personal_post() {
    let mut e = engine();
    let mut o = observation(PageKind::ServerError);
    let facts = PageFacts {
        recovery: ceh_core::recovery::RecoveryFacts {
            official_site: true,
            endpoint: ceh_core::recovery::RecoveryEndpoint::Availability,
            ..Default::default()
        },
        ..Default::default()
    };
    e.observe_with_facts(o.clone(), facts, 0).unwrap();
    o.method = RequestMethod::Post;
    assert_eq!(
        e.observe_with_facts(o, facts, 30_000).unwrap(),
        vec![FlowEffect::Hold]
    );
}
#[test]
fn expired_retries_advance_schedule_instead_of_looping_immediately() {
    let mut e = engine();
    let o = observation(PageKind::Expired);
    let facts = PageFacts {
        recovery: ceh_core::recovery::RecoveryFacts {
            own_control: true,
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(
        e.observe_with_facts(o.clone(), facts, 0).unwrap(),
        vec![FlowEffect::WaitUntil(0)]
    );
    assert!(matches!(
        &e.observe_with_facts(o.clone(), facts, 0).unwrap()[0],
        FlowEffect::Recover {
            action: ceh_core::recovery::RecoveryAction::ActivateOwnControl,
            ..
        }
    ));
    assert_eq!(
        e.observe_with_facts(o.clone(), facts, 1).unwrap(),
        vec![FlowEffect::WaitUntil(30_001)]
    );
    assert!(e.observe_with_facts(o, facts, 30_000).unwrap().is_empty());
}

#[test]
fn post_auth_applicant_wait_uses_get_instead_of_replaying_auth_post() {
    let mut e = engine();
    let mut o = observation(PageKind::ApplicantNoSlots);
    o.method = RequestMethod::Post;
    let facts = PageFacts {
        recovery: ceh_core::recovery::RecoveryFacts {
            official_site: true,
            endpoint: ceh_core::recovery::RecoveryEndpoint::ApplicantEntry,
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(
        e.observe_with_facts(o.clone(), facts, 0).unwrap(),
        vec![FlowEffect::WaitUntil(30_000)]
    );
    assert!(matches!(
        &e.observe_with_facts(o, facts, 30_000).unwrap()[0],
        FlowEffect::FetchApplicant { .. }
    ));
}
#[test]
fn aceptar_restart_waits_profile_cooldown() {
    let mut e = engine();
    let o = observation(PageKind::RequestNoSlotsRestart);
    assert_eq!(
        e.observe(o.clone(), 0).unwrap(),
        vec![FlowEffect::WaitUntil(180_000)]
    );
    assert!(e.observe(o.clone(), 179_999).unwrap().is_empty());
    assert!(matches!(
        &e.observe_with_facts(o, controls(), 180_000).unwrap()[0],
        FlowEffect::Activate {
            action: ActionKind::NoSlotsRestart,
            ..
        }
    ));
}
#[test]
fn result_salir_requires_current_control_after_wait() {
    let mut e = engine();
    let o = observation(PageKind::RequestNoSlots);
    e.observe(o.clone(), 0).unwrap();
    assert_eq!(e.observe(o, 30_000).unwrap(), vec![FlowEffect::Hold]);
}

#[test]
fn unknown_page_has_one_bounded_reread_before_hold() {
    let mut e = engine();
    let o = observation(PageKind::Unknown);
    assert_eq!(
        e.observe(o.clone(), 0).unwrap(),
        vec![FlowEffect::ReobserveAt(500)]
    );
    assert!(e.observe(o.clone(), 499).unwrap().is_empty());
    assert_eq!(e.observe(o, 500).unwrap(), vec![FlowEffect::Hold]);
}
#[test]
fn loading_timeout_waits_then_restarts_without_replaying_form() {
    let mut e = engine();
    let o = observation(PageKind::Loading);
    assert_eq!(
        e.observe(o.clone(), 0).unwrap(),
        vec![FlowEffect::ReobserveAt(500)]
    );
    assert_eq!(
        e.observe(o.clone(), 45_000).unwrap(),
        vec![FlowEffect::WaitUntil(75_000)]
    );
    assert!(e.observe(o.clone(), 74_999).unwrap().is_empty());
    assert!(matches!(
        &e.observe(o, 75_000).unwrap()[0],
        FlowEffect::Recover {
            action: ceh_core::recovery::RecoveryAction::Restart {
                clear_profile: false,
                reopen: false
            },
            ..
        }
    ));
}
#[test]
fn auth_wait_does_not_raise_an_alarm_for_a_slow_page() {
    let mut e = engine();
    assert_eq!(
        e.observe(observation(PageKind::AuthWait), 0).unwrap(),
        vec![FlowEffect::ReobserveAt(500)]
    );
}

#[test]
fn custom_recovery_jitter_range_is_supported_by_engine() {
    let timings =
        ceh_core::timings::parse_timings(&serde_json::json!({"recovery_jitter_seconds":[5,10]}))
            .unwrap();
    let mut e = FlowEngine::with_timings(
        SessionId::new("s").unwrap(),
        SearchOptions::default(),
        timings,
        30.0,
    )
    .unwrap();
    assert_eq!(
        e.observe(observation(PageKind::Blocked), 0).unwrap(),
        vec![FlowEffect::WaitUntil(35_000)]
    );
}

#[test]
fn clave_method_is_selected_once_before_calling_for_human() {
    let mut e = engine();
    let o = observation(PageKind::AuthWait);
    let facts = PageFacts {
        authentication: AuthenticationMode::Clave,
        clave_method: ClaveMethod::Permanente,
        clave_method_available: true,
        ..Default::default()
    };
    assert!(matches!(
        &e.observe_with_facts(o.clone(), facts, 0).unwrap()[0],
        FlowEffect::SelectClaveMethod {
            method: ClaveMethod::Permanente,
            ..
        }
    ));
    assert_eq!(
        e.observe_with_facts(o, facts, 1).unwrap(),
        vec![FlowEffect::ReobserveAt(501)]
    );
}
#[test]
fn clave_alarm_stops_on_input_and_on_leaving_authentication() {
    let mut e = engine();
    let o = observation(PageKind::AuthWait);
    let mut facts = PageFacts {
        authentication: AuthenticationMode::Clave,
        clave_requires_attention: true,
        ..Default::default()
    };
    assert_eq!(
        e.observe_with_facts(o.clone(), facts, 0).unwrap(),
        vec![FlowEffect::Alert, FlowEffect::ReobserveAt(500)]
    );
    facts.clave_input_started = true;
    assert_eq!(
        e.observe_with_facts(o.clone(), facts, 1).unwrap(),
        vec![FlowEffect::Silence, FlowEffect::ReobserveAt(501)]
    );
    facts.clave_input_started = false;
    assert_eq!(
        e.observe_with_facts(o, facts, 2).unwrap(),
        vec![FlowEffect::ReobserveAt(502)]
    );
    let mut e = engine();
    e.observe_with_facts(observation(PageKind::AuthWait), facts, 0)
        .unwrap();
    let effects = e.observe(observation(PageKind::Applicant), 1).unwrap();
    assert!(matches!(
        &effects[..],
        [FlowEffect::Silence, FlowEffect::Prepare { .. }]
    ));
}

#[test]
fn confirmed_normal_page_resets_recovery_streak() {
    let mut e = engine();
    let o = observation(PageKind::Expired);
    let facts = controls();
    e.observe_with_facts(o.clone(), facts, 0).unwrap();
    e.observe_with_facts(o.clone(), facts, 0).unwrap();
    assert_eq!(
        e.observe_with_facts(o.clone(), facts, 1).unwrap(),
        vec![FlowEffect::WaitUntil(30_001)]
    );
    e.observe(observation(PageKind::Applicant), 2).unwrap();
    assert_eq!(
        e.observe_with_facts(o, facts, 3).unwrap(),
        vec![FlowEffect::WaitUntil(3)]
    );
}

#[test]
fn pause_cancels_preparation_ack_and_requires_fresh_observation() {
    let mut e = engine();
    let o = observation(PageKind::Applicant);
    let effects = e.observe(o.clone(), 0).unwrap();
    let FlowEffect::Prepare { guard, action } = &effects[0] else {
        panic!("missing prepare")
    };
    e.pause(10).unwrap();
    assert!(e.observe(o.clone(), 20).unwrap().is_empty());
    assert_eq!(
        e.complete_preparation(guard, *action, PreparationOutcome::Completed),
        Err(FlowError::StaleCompletion)
    );
    let resumed = e.resume(30).unwrap();
    let effects = e.observe(o, 31).unwrap();
    let FlowEffect::Prepare { guard: fresh, .. } = &effects[0] else {
        panic!("missing fresh prepare")
    };
    assert_eq!(fresh.epoch, resumed.epoch());
    assert_ne!(fresh.epoch, guard.epoch);
}
#[test]
fn paused_duration_does_not_shorten_pending_wait() {
    let mut e = engine();
    let o = observation(PageKind::ApplicantNoSlots);
    e.observe(o.clone(), 0).unwrap();
    e.pause(1000).unwrap();
    e.resume(11_000).unwrap();
    assert!(e.observe(o.clone(), 39_999).unwrap().is_empty());
    assert!(matches!(
        &e.observe(o, 40_000).unwrap()[0],
        FlowEffect::Reload { .. }
    ));
}
#[test]
fn resume_cannot_reclaim_booking_ownership() {
    let mut e = engine();
    e.observe(observation(PageKind::Booking), 0).unwrap();
    assert!(e.resume(1).is_err());
    assert!(e.pause(1).is_err());
    assert!(
        e.observe(observation(PageKind::Applicant), 2)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn unknown_dispatch_ack_never_replays_applicant_post_after_pause() {
    let mut e = engine();
    let o = observation(PageKind::Applicant);
    let effects = e.observe(o.clone(), 0).unwrap();
    let FlowEffect::Prepare { guard, action } = &effects[0] else {
        panic!("missing prepare")
    };
    e.complete_preparation(guard, *action, PreparationOutcome::Completed)
        .unwrap();
    let effects = e.observe(o.clone(), 1).unwrap();
    let FlowEffect::Activate { guard, action } = &effects[0] else {
        panic!("missing submit")
    };
    e.acknowledge_dispatch(guard, *action, DispatchOutcome::Unknown)
        .unwrap();
    e.pause(2).unwrap();
    e.resume(3).unwrap();
    assert_eq!(e.observe(o, 4).unwrap(), vec![FlowEffect::Hold]);
    assert_eq!(
        e.dispatch_record(guard, *action).unwrap().outcome,
        Some(DispatchOutcome::Unknown)
    );
}
#[test]
fn dispatch_ack_is_not_transition_confirmation_and_duplicates_are_idempotent() {
    let mut e = engine();
    let effects = e.observe(observation(PageKind::RequestMenu), 0).unwrap();
    let FlowEffect::Activate { guard, action } = &effects[0] else {
        panic!("missing request")
    };
    e.acknowledge_dispatch(guard, *action, DispatchOutcome::Dispatched)
        .unwrap();
    e.acknowledge_dispatch(guard, *action, DispatchOutcome::Dispatched)
        .unwrap();
    assert_eq!(
        e.dispatch_record(guard, *action).unwrap().outcome,
        Some(DispatchOutcome::Dispatched)
    );
    assert!(
        e.acknowledge_dispatch(guard, *action, DispatchOutcome::NotDispatched)
            .is_err()
    );
    assert_eq!(
        e.observe(observation(PageKind::RequestMenu), 1).unwrap(),
        vec![FlowEffect::Hold]
    );
}

#[test]
fn redundant_resume_does_not_cancel_inflight_preparation() {
    let mut e = engine();
    let o = observation(PageKind::Contact);
    let effects = e.observe(o.clone(), 0).unwrap();
    let FlowEffect::Prepare { guard, action } = &effects[0] else {
        panic!("missing prepare")
    };
    e.resume(1).unwrap();
    e.complete_preparation(guard, *action, PreparationOutcome::Completed)
        .unwrap();
    assert!(matches!(
        &e.observe(o, 2).unwrap()[0],
        FlowEffect::Activate {
            action: ActionKind::Contact,
            ..
        }
    ));
}

#[test]
fn office_options_reducing_to_one_resume_preparation_and_cancel_attention() {
    let mut e = engine();
    let page = observation(PageKind::Office);
    e.observe_with_facts(
        page.clone(),
        PageFacts {
            office_count: 2,
            ..Default::default()
        },
        0,
    )
    .unwrap();
    let effects = e
        .observe_with_facts(
            page,
            PageFacts {
                office_count: 1,
                ..Default::default()
            },
            1,
        )
        .unwrap();
    assert!(matches!(
        effects.as_slice(),
        [
            FlowEffect::Silence,
            FlowEffect::Prepare {
                action: ActionKind::Office,
                ..
            }
        ]
    ));
}
#[test]
fn leaving_captcha_cancels_its_attention_without_releasing_booking_ownership() {
    let mut e = engine();
    assert_eq!(
        e.observe(observation(PageKind::Captcha), 0).unwrap(),
        vec![FlowEffect::Alert]
    );
    assert!(matches!(
        e.observe(observation(PageKind::Applicant), 1)
            .unwrap()
            .as_slice(),
        [
            FlowEffect::Silence,
            FlowEffect::Prepare {
                action: ActionKind::Applicant,
                ..
            }
        ]
    ));
}

#[test]
fn appointment_deadline_delays_applicant_and_request_without_consuming_the_action() {
    for kind in [PageKind::Applicant, PageKind::RequestMenu] {
        let mut e = engine();
        let facts = PageFacts { appointment_ready_at: Some(15_000), ..Default::default() };
        let page = observation(kind);
        assert_eq!(e.observe_with_facts(page.clone(), facts, 0).unwrap(), vec![FlowEffect::WaitUntil(15_000)]);
        let effect = e.observe_with_facts(page, facts, 15_000).unwrap();
        assert!(matches!(effect.as_slice(), [FlowEffect::Prepare { action: ActionKind::Applicant, .. }] | [FlowEffect::Activate { action: ActionKind::Request, .. }]));
    }
}

#[test]
fn applicant_banner_requires_a_new_get_before_filling() {
    let mut e = engine();
    let mut facts = PageFacts { confirm_appointment_round: true, appointment_ready_at: Some(0), ..Default::default() };
    facts.recovery.official_site = true;
    e.observe_with_facts(observation(PageKind::ApplicantNoSlots), facts, 0).unwrap();
    let page = observation(PageKind::Applicant);
    assert!(e.observe_with_facts(page.clone(), facts, 1).unwrap().is_empty());
    assert!(matches!(e.observe_with_facts(page, facts, 2).unwrap().as_slice(), [FlowEffect::Recover { action: ceh_core::recovery::RecoveryAction::GetEntry, .. }]));
}

fn applicant_entry_facts() -> PageFacts {
    PageFacts {
        recovery: ceh_core::recovery::RecoveryFacts {
            own_control: true,
            official_site: true,
            endpoint: ceh_core::recovery::RecoveryEndpoint::ApplicantEntry,
            ..Default::default()
        },
        ..Default::default()
    }
}
fn rate_limited(document: &str, method: RequestMethod) -> Observation {
    Observation {
        document: DocumentId::new(document).unwrap(),
        kind: PageKind::RateLimited,
        method,
        operation_code: false,
        variant: String::new(),
    }
}
#[test]
fn rate_limit_on_applicant_get_adds_thirty_seconds_once_per_document() {
    let mut e = engine();
    assert_eq!(e.applicant_reload_penalty(), 0.0);
    e.observe_with_facts(rate_limited("first", RequestMethod::Get), applicant_entry_facts(), 0).unwrap();
    e.observe_with_facts(rate_limited("first", RequestMethod::Get), applicant_entry_facts(), 1).unwrap();
    assert_eq!(e.applicant_reload_penalty(), 30.0);
    e.observe_with_facts(rate_limited("second", RequestMethod::Get), applicant_entry_facts(), 2).unwrap();
    assert_eq!(e.applicant_reload_penalty(), 60.0);
    let effects = e
        .observe_with_facts(observation(PageKind::ApplicantNoSlots), applicant_entry_facts(), 1_000)
        .unwrap();
    assert_eq!(effects, vec![FlowEffect::WaitUntil(1_000 + 30_000 + 60_000)]);
}
#[test]
fn rate_limit_on_post_or_other_endpoint_adds_no_penalty() {
    let mut e = engine();
    e.observe_with_facts(rate_limited("post", RequestMethod::Post), applicant_entry_facts(), 0).unwrap();
    let mut other = applicant_entry_facts();
    other.recovery.endpoint = ceh_core::recovery::RecoveryEndpoint::Availability;
    e.observe_with_facts(rate_limited("availability", RequestMethod::Get), other, 1).unwrap();
    assert_eq!(e.applicant_reload_penalty(), 0.0);
}
#[test]
fn rate_limit_penalty_is_not_applied_when_flag_is_disabled() {
    let timings = ceh_core::timings::parse_timings(&serde_json::json!({"adaptive_reload_on_429": false})).unwrap();
    let mut e = FlowEngine::with_timings(SessionId::new("s").unwrap(), SearchOptions::default(), timings, 30.0).unwrap();
    e.observe_with_facts(rate_limited("first", RequestMethod::Get), applicant_entry_facts(), 0).unwrap();
    assert_eq!(e.applicant_reload_penalty(), 0.0);
}

#[test]
fn applied_timings_change_next_wait_and_keep_rate_limit_penalty() {
    let mut e = engine();
    e.observe_with_facts(rate_limited("first", RequestMethod::Get), applicant_entry_facts(), 0).unwrap();
    let slower = ceh_core::timings::parse_timings(&serde_json::json!({"reload_seconds": [40, 40]})).unwrap();
    e.apply_timings(slower, 40.0).unwrap();
    assert_eq!(e.applicant_reload_penalty(), 30.0);
    // The first observation after a pending recovery only cancels it; the next one schedules the wait.
    assert!(e.observe_with_facts(observation(PageKind::ApplicantNoSlots), applicant_entry_facts(), 500).unwrap().is_empty());
    let effects = e
        .observe_with_facts(observation(PageKind::ApplicantNoSlots), applicant_entry_facts(), 1_000)
        .unwrap();
    assert_eq!(effects, vec![FlowEffect::WaitUntil(1_000 + 40_000 + 30_000)]);
}

#[test]
fn applied_timings_reject_a_sample_outside_the_new_range_without_change() {
    let mut e = engine();
    let range = ceh_core::timings::parse_timings(&serde_json::json!({"reload_seconds": [40, 50]})).unwrap();
    assert!(e.apply_timings(range, 30.0).is_err());
    let effects = e.observe(observation(PageKind::ApplicantNoSlots), 0).unwrap();
    assert_eq!(effects, vec![FlowEffect::WaitUntil(30_000)]);
}

#[test]
fn rate_limit_penalty_stops_applying_when_the_flag_is_switched_off_mid_run() {
    let mut e = engine();
    e.observe_with_facts(rate_limited("first", RequestMethod::Get), applicant_entry_facts(), 0).unwrap();
    e.observe_with_facts(rate_limited("second", RequestMethod::Get), applicant_entry_facts(), 1).unwrap();
    assert_eq!(e.applicant_reload_penalty(), 60.0);
    let off = ceh_core::timings::parse_timings(&serde_json::json!({"adaptive_reload_on_429": false})).unwrap();
    e.apply_timings(off, 30.0).unwrap();
    let effects = e
        .observe_with_facts(observation(PageKind::ApplicantNoSlots), applicant_entry_facts(), 1_000)
        .unwrap();
    assert_eq!(effects, vec![FlowEffect::WaitUntil(1_000 + 30_000)], "Python adds the penalty only while the flag is on");
}
