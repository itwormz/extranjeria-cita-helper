#![cfg(feature = "shell")]
use ceh_desktop::search::{Control, SearchController};
#[test]
fn idle_controller_is_read_only_and_rejects_controls_without_owned_browser() {
    let controller = SearchController::default();
    let state = controller.snapshot().unwrap();
    assert!(!state.active);
    assert!(state.browser_pid.is_none());
    assert!(state.rows.is_empty());
    for command in [Control::Pause, Control::Resume, Control::Stop] {
        assert!(controller.control(command).is_err());
    }
    assert!(!controller.snapshot().unwrap().active);
}
