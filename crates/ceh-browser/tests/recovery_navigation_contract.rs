use ceh_browser::{
    BrowserError,
    recovery_navigation::{ENTRY_URL, NavigationRecovery, plan_navigation},
};
use ceh_core::{model::*, recognition::PageSnapshot, recovery::RecoveryAction};
use serde_json::json;

fn page(kind: PageKind, method: RequestMethod, official: bool, endpoint: &str, last: Option<&str>) -> (Observation, PageSnapshot) {
    let observation = Observation { document: DocumentId::new("doc").unwrap(), kind, method, operation_code: false, variant: String::new() };
    let snapshot = serde_json::from_value(json!({"visible":[],"frames":[{"text":""}],"official":official,"endpoint":endpoint,"last_action":last})).unwrap();
    (observation, snapshot)
}
const CURRENT: &str = "https://icp.administracionelectronica.gob.es/icpplustieb/acEntrada";

#[test]
fn get_entry_opens_the_fixed_start_page_on_any_page_but_a_booking() {
    for (kind, method, official) in [(PageKind::RateLimited, RequestMethod::Post, true), (PageKind::ServerError, RequestMethod::Post, false), (PageKind::Blocked, RequestMethod::Get, true)] {
        let (o, s) = page(kind, method, official, "acInfo", None);
        assert_eq!(plan_navigation(RecoveryAction::GetEntry, &o, &s, None), Ok(NavigationRecovery::Get(ENTRY_URL.to_owned())));
    }
    for kind in [PageKind::Booking, PageKind::Captcha] {
        let (o, s) = page(kind, RequestMethod::Get, true, "", None);
        assert_eq!(plan_navigation(RecoveryAction::GetEntry, &o, &s, None), Err(BrowserError::Unsupported));
    }
}

#[test]
fn get_current_repeats_one_get_of_an_official_page_and_never_a_post() {
    let (o, s) = page(PageKind::RateLimited, RequestMethod::Get, true, "acEntrada", None);
    assert_eq!(plan_navigation(RecoveryAction::GetCurrent, &o, &s, Some(CURRENT)), Ok(NavigationRecovery::Get(CURRENT.to_owned())));
    let (post, ps) = page(PageKind::RateLimited, RequestMethod::Post, true, "acEntrada", None);
    assert_eq!(plan_navigation(RecoveryAction::GetCurrent, &post, &ps, Some(CURRENT)), Err(BrowserError::Unsupported));
    for url in [None, Some("https://example.com/icpplus"), Some("http://icp.administracionelectronica.gob.es/x"), Some("https://icp.administracionelectronica.gob.es.evil.test/x")] {
        assert_eq!(plan_navigation(RecoveryAction::GetCurrent, &o, &s, url), Err(BrowserError::Unsupported), "{url:?}");
    }
    let (foreign, fs) = page(PageKind::RateLimited, RequestMethod::Get, false, "", None);
    assert_eq!(plan_navigation(RecoveryAction::GetCurrent, &foreign, &fs, Some(CURRENT)), Err(BrowserError::Unsupported));
}

#[test]
fn reload_follows_python_get_pages_and_the_repeatable_availability_search_only() {
    let (get, gs) = page(PageKind::ServerError, RequestMethod::Get, true, "acEntrada", None);
    assert_eq!(plan_navigation(RecoveryAction::Reload, &get, &gs, None), Ok(NavigationRecovery::Reload));
    let (search, ss) = page(PageKind::ServerError, RequestMethod::Post, true, "acCitar", Some("request"));
    assert_eq!(plan_navigation(RecoveryAction::Reload, &search, &ss, None), Ok(NavigationRecovery::Reload));
    let (contact, cs) = page(PageKind::ServerError, RequestMethod::Post, true, "acEntrada", Some("contact"));
    assert_eq!(plan_navigation(RecoveryAction::Reload, &contact, &cs, None), Err(BrowserError::Unsupported));
}

#[test]
fn actions_that_are_not_navigations_are_not_planned_here() {
    let (o, s) = page(PageKind::Blocked, RequestMethod::Get, true, "", None);
    for action in [RecoveryAction::Hold, RecoveryAction::ActivateOwnControl, RecoveryAction::Restart { clear_profile: true, reopen: false }] {
        assert_eq!(plan_navigation(action, &o, &s, None), Err(BrowserError::Unsupported));
    }
}
