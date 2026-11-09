use ceh_core::model::*;
use serde_json::json;

#[test]
fn search_options_default_to_independent_calm_choices() {
    let defaults: SearchOptions = serde_json::from_value(json!({})).unwrap();
    assert_eq!(defaults.applicant_no_slots_mode, ApplicantNoSlotsMode::Wait);
    assert_eq!(defaults.no_slots_mode, NoSlotsMode::Salir);
    for applicant in ["wait", "continue"] {
        for result in ["salir", "reload"] {
            let raw = json!({"applicant_no_slots_mode": applicant, "no_slots_mode": result});
            let options: SearchOptions = serde_json::from_value(raw.clone()).unwrap();
            assert_eq!(serde_json::to_value(options).unwrap(), raw);
        }
    }
}

#[test]
fn unknown_choices_and_wrong_field_types_are_rejected() {
    for raw in [
        json!({"no_slots_mode":"restart"}),
        json!({"applicant_no_slots_mode":true}),
        json!({"no_slots_mode":null}),
        json!({"typo":"reload"}),
    ] {
        assert!(serde_json::from_value::<SearchOptions>(raw).is_err());
    }
}

#[test]
fn profile_identity_matches_existing_safe_id_contract() {
    for id in ["profile-1", "demo_2", "a"] {
        let profile = ProfileId::new(id).unwrap();
        assert_eq!(profile.as_str(), id);
        assert_eq!(serde_json::to_value(profile).unwrap(), json!(id));
    }
    for id in ["", "Bad", "a/b", "../profile", "a b", "профиль"] {
        assert!(ProfileId::new(id).is_err());
        assert!(serde_json::from_value::<ProfileId>(json!(id)).is_err());
    }
}

#[test]
fn opaque_session_and_document_ids_are_nonempty_and_distinct_types() {
    for id in ["", " ", "\n"] {
        assert!(SessionId::new(id).is_err());
        assert!(DocumentId::new(id).is_err());
        assert!(serde_json::from_value::<DocumentId>(json!(id)).is_err());
    }
    let session = SessionId::new("fixture-session:1").unwrap();
    let document = DocumentId::new("fixture-document:hash").unwrap();
    assert_eq!(session.as_str(), "fixture-session:1");
    assert_eq!(document.as_str(), "fixture-document:hash");
}

#[test]
fn page_types_roundtrip_all_python_states_without_conflating_anomalies() {
    let states = [
        "province",
        "procedure",
        "auth",
        "auth_wait",
        "applicant",
        "applicant_no_slots",
        "request_menu",
        "request_no_slots",
        "request_no_slots_restart",
        "post_contact_no_slots",
        "office",
        "contact",
        "booking",
        "captcha",
        "expired",
        "server_error",
        "blocked",
        "rate_limited",
        "loading",
        "unknown",
    ];
    for state in states {
        let parsed: PageKind = serde_json::from_value(json!(state)).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), json!(state));
    }
    assert!(serde_json::from_value::<PageKind>(json!("invented_page")).is_err());
    assert_ne!(PageKind::Booking, PageKind::Captcha);
    assert_ne!(PageKind::Blocked, PageKind::RateLimited);
}

#[test]
fn decision_roundtrip_preserves_document_guard_and_pending_post() {
    let raw = json!({
        "next_state": {
            "profile":"fixture", "session":"session-1", "ownership":"automatic",
            "current":{"document":"doc-1", "kind":"applicant", "method":"post"},
            "pending":{"document":"doc-1", "action":"applicant"}
        },
        "effects":[{"type":"wait_until", "monotonic_millis":30000}]
    });
    let decision: Decision = serde_json::from_value(raw).unwrap();
    assert_eq!(
        decision.next_state.pending.as_ref().unwrap().action,
        ActionKind::Applicant
    );
    let roundtrip: Decision =
        serde_json::from_value(serde_json::to_value(&decision).unwrap()).unwrap();
    assert_eq!(roundtrip, decision);
}

#[test]
fn unsupported_effects_and_actions_do_not_become_executable_commands() {
    assert!(
        serde_json::from_value::<Effect>(json!({"type":"evaluate", "script":"arbitrary"})).is_err()
    );
    assert!(serde_json::from_value::<ActionKind>(json!("confirm_booking")).is_err());
    assert_eq!(
        serde_json::to_value(Procedure::Huellas).unwrap(),
        json!("huellas")
    );
    assert_eq!(
        serde_json::to_value(Procedure::Regreso).unwrap(),
        json!("regreso")
    );
}

#[test]
fn browser_actions_require_explicit_session_and_document_guards() {
    let raw = json!({"type":"browser_action", "guard":{"session":"session-1","epoch":0,"document":"doc-1","expected_page":"request_menu"}, "action":"request"});
    let effect: Effect = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(serde_json::to_value(effect).unwrap(), raw);
    assert!(
        serde_json::from_value::<Effect>(json!({"type":"browser_action", "action":"request"}))
            .is_err()
    );
    let handoff: Effect =
        serde_json::from_value(json!({"type":"handoff", "document":"doc-1"})).unwrap();
    assert_eq!(
        serde_json::to_value(handoff).unwrap(),
        json!({"type":"handoff", "document":"doc-1"})
    );
}
