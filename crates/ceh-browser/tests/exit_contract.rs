use ceh_browser::{BrowserError, exit::ExitAction};
use ceh_core::model::ActionKind;
#[test]
fn only_known_exit_actions_are_constructible() {
    for action in [
        ActionKind::NoSlotsRestart,
        ActionKind::NoSlotsSalir,
        ActionKind::PostContactRestart,
    ] {
        assert!(ExitAction::from_action(action).is_ok());
    }
    for action in [
        ActionKind::Applicant,
        ActionKind::Request,
        ActionKind::RequestReload,
        ActionKind::Contact,
    ] {
        assert!(matches!(
            ExitAction::from_action(action),
            Err(BrowserError::Unsupported)
        ));
    }
}
