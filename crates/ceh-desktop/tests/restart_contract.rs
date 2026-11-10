use ceh_core::{model::PageKind, recovery::RecoveryAction};
use ceh_desktop::restart::{RestartKind, RestartRequest};

#[test]
fn only_restart_actions_make_a_request_and_their_kind_follows_python() {
    let pass = RestartRequest::from_action(&RecoveryAction::Restart { clear_profile: false, reopen: false }, PageKind::Loading).unwrap();
    assert_eq!(pass.kind(), RestartKind::NewPass);
    let blocked = RestartRequest::from_action(&RecoveryAction::Restart { clear_profile: true, reopen: false }, PageKind::Blocked).unwrap();
    assert_eq!(blocked.kind(), RestartKind::NewSession);
    assert!(blocked.clear_profile);
    let clave = RestartRequest::from_action(&RecoveryAction::Restart { clear_profile: false, reopen: true }, PageKind::ServerError).unwrap();
    assert_eq!(clave.kind(), RestartKind::NewSession);
    assert!(!clave.clear_profile);
    for other in [RecoveryAction::Hold, RecoveryAction::Reload, RecoveryAction::GetCurrent, RecoveryAction::GetEntry, RecoveryAction::ActivateOwnControl] {
        assert_eq!(RestartRequest::from_action(&other, PageKind::Blocked), None);
    }
}

#[test]
fn the_journal_reasons_are_pythons() {
    let reason = |action, kind| RestartRequest::from_action(&action, kind).unwrap().reason;
    assert_eq!(reason(RecoveryAction::Restart { clear_profile: true, reopen: false }, PageKind::Blocked), "блокировка сайта");
    assert_eq!(reason(RecoveryAction::Restart { clear_profile: false, reopen: true }, PageKind::ServerError), "ошибка портала Cl@ve — профиль и cookies сохранены");
    assert_eq!(reason(RecoveryAction::Restart { clear_profile: false, reopen: false }, PageKind::Loading), "таймаут ответа сайта — новый проход");
}

#[test]
fn the_journal_note_tells_whether_the_window_is_kept() {
    let pass = RestartRequest::from_action(&RecoveryAction::Restart { clear_profile: false, reopen: false }, PageKind::Loading).unwrap();
    assert_eq!(pass.note(), "Новый проход: таймаут ответа сайта — новый проход — окно и профиль сохранены");
    let blocked = RestartRequest::from_action(&RecoveryAction::Restart { clear_profile: true, reopen: false }, PageKind::Blocked).unwrap();
    assert_eq!(blocked.note(), "Новая сессия: блокировка сайта");
}
