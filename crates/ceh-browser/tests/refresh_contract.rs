use ceh_browser::{
    BrowserError,
    refresh::{RefreshAction, plan_refresh},
};
use ceh_core::{model::*, recognition::PageSnapshot};
use serde_json::json;
fn facts(
    kind: PageKind,
    method: RequestMethod,
    endpoint: &str,
    last: Option<&str>,
    pending: Option<&str>,
) -> (Observation, PageSnapshot) {
    let page = Observation {
        document: DocumentId::new("refresh-page").unwrap(),
        kind,
        method,
        operation_code: false,
        variant: String::new(),
    };
    let snapshot=serde_json::from_value(json!({"visible":[],"frames":[{"text":""}],"official":true,"endpoint":endpoint,"last_action":last,"pending_action":pending})).unwrap();
    (page, snapshot)
}
#[test]
fn refresh_requires_actual_method_endpoint_and_prior_action() {
    for (kind, method, endpoint, last, expected) in [
        (
            PageKind::ApplicantNoSlots,
            RequestMethod::Get,
            "acEntrada",
            None,
            RefreshAction::ReloadApplicant,
        ),
        (
            PageKind::ApplicantNoSlots,
            RequestMethod::Post,
            "acEntrada",
            None,
            RefreshAction::FetchApplicant,
        ),
        (
            PageKind::RequestNoSlots,
            RequestMethod::Get,
            "acCitar",
            None,
            RefreshAction::ReloadAvailability,
        ),
        (
            PageKind::RequestNoSlots,
            RequestMethod::Post,
            "acCitar",
            Some("request"),
            RefreshAction::ReloadAvailability,
        ),
    ] {
        let (p, s) = facts(kind, method, endpoint, last, None);
        assert_eq!(plan_refresh(&p, &s), Ok(expected));
    }
}
#[test]
fn uncertain_or_mutating_posts_and_manual_pages_are_not_refreshable() {
    for previous in ["applicant", "contact", "office", "request"] {
        let (p, s) = facts(
            PageKind::ApplicantNoSlots,
            RequestMethod::Post,
            "acEntrada",
            Some(previous),
            None,
        );
        assert_eq!(plan_refresh(&p, &s), Err(BrowserError::Unsupported));
    }
    for (kind, method, endpoint, last, pending) in [
        (
            PageKind::ApplicantNoSlots,
            RequestMethod::Unknown,
            "acEntrada",
            None,
            None,
        ),
        (
            PageKind::ApplicantNoSlots,
            RequestMethod::Post,
            "acValidarEntrada",
            None,
            None,
        ),
        (
            PageKind::RequestNoSlots,
            RequestMethod::Post,
            "acCitar",
            Some("request"),
            Some("contact"),
        ),
        (
            PageKind::RequestNoSlots,
            RequestMethod::Post,
            "acOfertarCita",
            Some("request"),
            None,
        ),
        (PageKind::Booking, RequestMethod::Get, "acCitar", None, None),
        (
            PageKind::PostContactNoSlots,
            RequestMethod::Post,
            "acCitar",
            Some("request"),
            None,
        ),
    ] {
        let (p, s) = facts(kind, method, endpoint, last, pending);
        assert_eq!(plan_refresh(&p, &s), Err(BrowserError::Unsupported));
    }
    let (p, mut s) = facts(
        PageKind::ApplicantNoSlots,
        RequestMethod::Get,
        "acEntrada",
        None,
        None,
    );
    s.official = false;
    assert_eq!(plan_refresh(&p, &s), Err(BrowserError::Unsupported));
}
#[test]
fn bootstrap_reload_is_only_for_a_loaded_official_province_get(){
 use ceh_browser::refresh::entry_reload_allowed;
 let (page,snapshot)=facts(PageKind::Province,RequestMethod::Get,"index.html",None,None);assert!(entry_reload_allowed(&page,&snapshot));
 for kind in [PageKind::Booking,PageKind::Applicant,PageKind::AuthWait,PageKind::Unknown,PageKind::Loading]{let (page,snapshot)=facts(kind,RequestMethod::Get,"index.html",None,None);assert!(!entry_reload_allowed(&page,&snapshot));}
 let (post,mut snapshot)=facts(PageKind::Province,RequestMethod::Post,"index.html",None,None);assert!(!entry_reload_allowed(&post,&snapshot));
 snapshot.official=false;assert!(!entry_reload_allowed(&page,&snapshot));
}
