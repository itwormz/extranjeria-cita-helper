use ceh_browser::{BrowserError, selection::Selection};
use ceh_core::model::ActionKind;
#[test]
fn selection_data_accepts_only_supported_service_and_bounded_labels() {
    assert_eq!(
        Selection::province("Barcelona").unwrap().action(),
        ActionKind::Province
    );
    assert_eq!(
        Selection::procedure("huellas").unwrap().action(),
        ActionKind::Procedure
    );
    assert_eq!(
        Selection::procedure("regreso").unwrap().action(),
        ActionKind::Procedure
    );
    assert!(matches!(
        Selection::procedure("other"),
        Err(BrowserError::Unsupported)
    ));
    assert!(matches!(
        Selection::province(""),
        Err(BrowserError::Unsupported)
    ));
    assert!(matches!(
        Selection::province("Barcelona\n"),
        Err(BrowserError::Unsupported)
    ));
    assert!(matches!(
        Selection::province(&"x".repeat(257)),
        Err(BrowserError::Unsupported)
    ));
}

#[test]
fn single_office_is_a_distinct_guarded_action() {
    assert_eq!(Selection::single_office().action(), ActionKind::Office);
}
