use ceh_core::{
    flow::FlowEffect,
    model::{ActionKind, DispatchOutcome, DocumentGuard, DocumentId, PageKind, RequestMethod, SessionId},
    recovery::RecoveryAction,
};
use ceh_desktop::session_timeline::*;
use serde_json::json;

fn guard() -> DocumentGuard {
    DocumentGuard { session: SessionId::new("secret-session").unwrap(), epoch: 3, document: DocumentId::new("secret-document-id").unwrap(), expected_page: PageKind::Procedure }
}

#[test]
fn a_step_row_names_effects_actions_delay_and_receipts_but_no_document_or_session_ids() {
    let effects = vec![
        FlowEffect::Prepare { guard: guard(), action: ActionKind::Procedure },
        FlowEffect::Activate { guard: guard(), action: ActionKind::Procedure },
        FlowEffect::Recover { guard: guard(), action: RecoveryAction::Restart { clear_profile: true, reopen: false } },
        FlowEffect::WaitUntil(12_345),
    ];
    let row = step_row(&effects, Some(612), &[(ActionKind::Procedure, DispatchOutcome::Dispatched)], 2_900);
    assert_eq!(row["effects"], json!(["prepare:procedure", "activate:procedure", "recover:restart(clear_profile)", "wait_until"]));
    assert_eq!(row["delay_ms"], 612);
    assert_eq!(row["elapsed_ms"], 2_900);
    assert_eq!(row["receipts"], json!([{"action":"procedure","outcome":"dispatched"}]));
    let text = row.to_string();
    for secret in ["secret-session", "secret-document-id", "12345"] {
        assert!(!text.contains(secret), "{secret}: {text}");
    }
}

#[test]
fn a_page_row_identifies_the_document_by_a_short_hash_only() {
    let row = page_row(PageKind::Procedure, RequestMethod::Post, &DocumentId::new("secret-document-id").unwrap());
    assert_eq!(row["page"], "procedure");
    assert_eq!(row["method"], "post");
    assert_eq!(row["doc"].as_str().unwrap().len(), 8);
    assert!(!row.to_string().contains("secret-document-id"));
    assert_eq!(row["doc"], page_row(PageKind::Procedure, RequestMethod::Post, &DocumentId::new("secret-document-id").unwrap())["doc"], "stable");
}

#[test]
fn a_header_row_holds_the_session_facts_and_nothing_else() {
    let row = header_row(&SessionHeader {
        app_version: "0.0.0-dev.1".into(), firefox_version: "159.0".into(), os: "macos".into(), profile_persistent: true,
        profile_age_days: Some(3), certificate_cache: "ready".into(),
        action_delay_ms: [300, 800], first_bootstrap_ms: 6_100, typing_ms: 91, contact_typing_ms: 48,
    });
    assert_eq!(row["profile"], json!({"persistent":true,"age_days":3}));
    assert_eq!(row["certificate_cache"], "ready");
    assert_eq!(row["timings"]["action_delay_ms"], json!([300, 800]));
    assert_eq!(row["firefox"], "159.0");
}

#[test]
fn command_rows_get_a_madrid_time_and_keep_only_the_sanitized_summary() {
    let raw = json!({"t_ms": 1_794_338_551_000_i64, "method":"input.performActions", "summary":{"key_down":4}, "ok":true, "duration_ms": 40});
    let row = command_row(&raw).unwrap();
    assert_eq!(row["kind"], "cmd");
    assert_eq!(row["t"], "2026-11-10T20:22:31.000+01:00");
    assert_eq!(row["summary"], json!({"key_down":4}));
    assert!(command_row(&json!({"no":"time"})).is_none());
}

#[test]
fn a_block_row_has_the_marker_and_the_screenshot_result_and_never_page_text() {
    let row = block_row(PageKind::Blocked, RequestMethod::Post, Some("request rejected"), "block-1.png");
    assert_eq!(row["marker"], "request rejected");
    assert_eq!(row["screenshot"], "block-1.png");
    assert_eq!(row["page"], "blocked");
    assert_eq!(block_row(PageKind::RateLimited, RequestMethod::Get, None, "console_locked")["marker"], json!(null));
}

#[test]
fn rows_are_appended_as_private_json_lines_and_nothing_is_written_without_a_directory() {
    let dir = tempfile::tempdir().unwrap();
    let timeline = SessionTimeline::new(Some(dir.path()));
    timeline.append("page", json!({"page":"province"}));
    timeline.append("page", json!({"page":"procedure"}));
    let text = std::fs::read_to_string(dir.path().join("timeline.jsonl")).unwrap();
    let rows: Vec<serde_json::Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r["t"].as_str().is_some_and(|t| t.ends_with("+01:00") || t.ends_with("+02:00")) && r["kind"] == "page"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(dir.path().join("timeline.jsonl")).unwrap().permissions().mode() & 0o777, 0o600);
    }
    let off = SessionTimeline::new(None);
    off.append("page", json!({}));
    assert!(!off.is_enabled());
}

#[test]
fn a_failed_step_keeps_its_effects_and_delay_with_the_error_code() {
    let effects = vec![FlowEffect::Activate { guard: guard(), action: ActionKind::Request }];
    let row = step_error_row(&effects, Some(400), 1_200, "browser_action_failed");
    assert_eq!(row["effects"], json!(["activate:request"]));
    assert_eq!(row["delay_ms"], 400);
    assert_eq!(row["error"], "browser_action_failed");
    assert_eq!(row["receipts"], json!([]));
}
