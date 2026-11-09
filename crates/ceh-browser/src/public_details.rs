//! Bounded public labels only. These reads must run after handoff, never delay it.
use crate::{BrowserError, bidi::BidiConnection, observer::ObservedPage};
use ceh_core::{
    booking::{PublicBookingDetails, normalize_booking_details, parse_offers},
    model::PageKind,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
const MAX_RAW: usize = 128 * 1024;
fn public_label(raw: &Value) -> Result<String, BrowserError> {
    let text = raw
        .as_str()
        .filter(|s| s.chars().count() <= 320)
        .ok_or(BrowserError::NoObservation)?;
    normalize_booking_details(&json!({"office":text}))
        .office
        .ok_or(BrowserError::NoObservation)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfficeFacts {
    pub offices: Vec<String>,
    pub selected: Option<String>,
}
impl OfficeFacts {
    pub fn decode(raw: &str) -> Result<Self, BrowserError> {
        if raw.len() > MAX_RAW {
            return Err(BrowserError::NoObservation);
        }
        let value: Value = serde_json::from_str(raw).map_err(|_| BrowserError::NoObservation)?;
        let map = value
            .as_object()
            .filter(|m| m.len() == 2 && m.contains_key("offices") && m.contains_key("selected"))
            .ok_or(BrowserError::NoObservation)?;
        let rows = map["offices"]
            .as_array()
            .filter(|v| v.len() <= 128)
            .ok_or(BrowserError::NoObservation)?;
        let mut offices = Vec::new();
        for raw in rows {
            offices.push(public_label(raw)?);
        }
        let selected = if map["selected"].is_null() {
            None
        } else {
            let label = public_label(&map["selected"])?;
            if !offices.contains(&label) {
                return Err(BrowserError::NoObservation);
            }
            Some(label)
        };
        Ok(Self { offices, selected })
    }
}
pub fn decode_offers(
    raw: &str,
    office: Option<&str>,
) -> Result<PublicBookingDetails, BrowserError> {
    if raw.len() > 32768 {
        return Err(BrowserError::NoObservation);
    }
    let labels: Value = serde_json::from_str(raw).map_err(|_| BrowserError::NoObservation)?;
    let rows = labels
        .as_array()
        .filter(|v| v.len() <= 32)
        .ok_or(BrowserError::NoObservation)?;
    if rows
        .iter()
        .any(|v| v.as_str().is_none_or(|s| s.chars().count() > 240))
    {
        return Err(BrowserError::NoObservation);
    }
    let offers = parse_offers(&labels);
    if offers.is_empty() {
        return Ok(PublicBookingDetails::default());
    }
    let office = office.and_then(|s| normalize_booking_details(&json!({"office":s})).office);
    Ok(PublicBookingDetails { office, offers })
}
fn read(
    connection: &mut BidiConnection,
    page: &ObservedPage,
    script: &'static str,
) -> Result<String, BrowserError> {
    if !page.page.snapshot.official {
        return Err(BrowserError::Unsupported);
    }
    let source = page
        .page
        .sources
        .get(page.recognition.source_frame)
        .ok_or(BrowserError::NoObservation)?;
    let result=connection.call_before("script.callFunction",json!({"target":{"realm":source.realm.as_str()},"awaitPromise":false,"functionDeclaration":script}),Instant::now()+Duration::from_millis(750)).map_err(|_|BrowserError::NoObservation)?;
    if result["type"] != "success" || result["result"]["type"] != "string" {
        return Err(BrowserError::NoObservation);
    }
    result["result"]["value"]
        .as_str()
        .filter(|s| s.len() <= MAX_RAW)
        .map(str::to_owned)
        .ok_or(BrowserError::NoObservation)
}
pub fn read_offices(
    connection: &mut BidiConnection,
    page: &ObservedPage,
) -> Result<OfficeFacts, BrowserError> {
    if page.observation.kind != PageKind::Office {
        return Err(BrowserError::Unsupported);
    }
    OfficeFacts::decode(&read(connection, page, include_str!("public-offices.js"))?)
}
/// Invoke only after the core has handed control to the person. Failure keeps
/// the generic alert; it never authorizes reconnecting/closing a manual browser.
/// Office must come from this exact session's public office selection.
pub fn read_booking(
    connection: &mut BidiConnection,
    page: &ObservedPage,
    office: Option<&str>,
) -> Result<PublicBookingDetails, BrowserError> {
    if page.observation.kind != PageKind::Booking {
        return Err(BrowserError::Unsupported);
    }
    decode_offers(
        &read(connection, page, include_str!("public-offers.js"))?,
        office,
    )
}
