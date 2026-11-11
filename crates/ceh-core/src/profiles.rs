//! Validation of normalized profile drafts, preserving the Python config contract.
//! Raw YAML loading and native secret access belong to separate adapters.
use crate::{
    model::ProfileId,
    python_unicode::{decimal, is_digit, is_space},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileDraft {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    pub procedure: String,
    pub province: String,
    pub office: String,
    pub document_type: String,
    pub document_number: String,
    pub full_name: String,
    pub nationality: String,
    pub phone: String,
    pub email: String,
    pub access_mode: String,
    pub clave_method: String,
    pub fill_mode: String,
    pub no_slots_mode: String,
    pub applicant_no_slots_mode: String,
    pub subprocedure: Option<String>,
    pub timezone: String,
    pub browser_session: String,
    #[serde(
        default = "default_browser",
        skip_serializing_if = "is_default_browser"
    )]
    pub browser: String,
}

fn default_browser() -> String {
    "firefox".into()
}
fn is_default_browser(value: &String) -> bool {
    value == "firefox"
}

#[derive(Clone, Deserialize)]
pub struct ProfileCatalog {
    pub provinces: Vec<String>,
    pub nationalities: Vec<String>,
    pub barcelona_offices: Vec<String>,
    /// Supplied by the runtime's IANA timezone provider, not a hardcoded shortlist.
    pub timezones: Vec<String>,
}

#[derive(Serialize)]
pub struct ValidationReport {
    pub normalized: ProfileDraft,
    pub errors: Vec<String>,
}

const LETTERS: &[u8] = b"TRWAGMYFPDXBNJZSQVHLCKE";
fn valid_nie(value: &str) -> bool {
    let chars: Vec<_> = value.chars().collect();
    if chars.len() != 9 || !chars[8].is_ascii_uppercase() {
        return false;
    }
    let mut number = match chars[0] {
        'X' => 0,
        'Y' => 1,
        'Z' => 2,
        _ => return false,
    };
    for &c in &chars[1..8] {
        let Some(digit) = decimal(c) else {
            return false;
        };
        number = number * 10 + digit;
    }
    LETTERS[(number % 23) as usize] as char == chars[8]
}
fn valid_dni(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 9
        || !bytes[..8].iter().all(u8::is_ascii_digit)
        || !bytes[8].is_ascii_uppercase()
    {
        return false;
    }
    let number = bytes[..8]
        .iter()
        .fold(0u32, |total, c| total * 10 + u32::from(c - b'0'));
    LETTERS[(number % 23) as usize] == bytes[8]
}
fn valid_passport(value: &str) -> bool {
    (1..=20).contains(&value.len())
        && value
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}
fn valid_email(value: &str) -> bool {
    if value.chars().any(is_space) {
        return false;
    }
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    if local.is_empty() || domain.contains('@') {
        return false;
    }
    domain
        .rsplit_once('.')
        .is_some_and(|(head, tail)| !head.is_empty() && !tail.is_empty())
}

pub fn validate_profile(mut value: ProfileDraft, catalog: &ProfileCatalog) -> ValidationReport {
    value.browser = value.browser.to_lowercase();
    value.access_mode = value.access_mode.to_lowercase();
    value.clave_method = value.clave_method.to_lowercase();
    value.fill_mode = value.fill_mode.to_lowercase();
    value.no_slots_mode = value.no_slots_mode.to_lowercase();
    value.applicant_no_slots_mode = value.applicant_no_slots_mode.to_lowercase();
    value.document_type = value.document_type.to_uppercase();
    value.document_number = value.document_number.to_uppercase().replace(' ', "");
    value.full_name = value.full_name.trim_matches(is_space).to_owned();
    value.phone = value.phone.trim_matches(is_space).replace(' ', "");
    value.email = value.email.trim_matches(is_space).to_owned();
    let mut errors = Vec::new();
    let mut reject = |message: &str| errors.push(message.to_owned());
    if !matches!(value.browser.as_str(), "firefox" | "chrome" | "brave") {
        reject("browser must be firefox, chrome or brave");
    }
    if ProfileId::new(&value.id).is_err() {
        reject("id must match [a-z0-9_-]+");
    }
    if value.label.is_empty() {
        reject("label is required");
    }
    if !matches!(value.no_slots_mode.as_str(), "reload" | "salir") {
        reject("request.no_slots_mode must be reload or salir");
    }
    if !matches!(value.applicant_no_slots_mode.as_str(), "wait" | "continue") {
        reject("request.applicant_no_slots_mode must be wait or continue");
    }
    if !matches!(value.clave_method.as_str(), "movil" | "permanente") {
        reject("request.clave_method must be movil or permanente");
    }
    if !matches!(
        value.access_mode.as_str(),
        "without_clave" | "clave" | "certificate"
    ) {
        reject("request.access_mode must be without_clave, clave or certificate");
    }
    if matches!(value.access_mode.as_str(), "clave" | "certificate")
        && value.browser_session != "persistent"
    {
        reject(&format!(
            "request.access_mode={} requires settings.browser.session=persistent",
            value.access_mode
        ));
    }
    if value.access_mode == "certificate" {
        if !(6..=15).contains(&value.phone.chars().count())
            || !value.phone.chars().all(|c| decimal(c).is_some())
        {
            reject("applicant.contact.phone must be 6-15 digits when access_mode=certificate");
        }
        if !valid_email(&value.email) {
            reject("applicant.contact.email must be a valid address when access_mode=certificate");
        }
    }
    if !catalog.provinces.contains(&value.province) {
        reject("request.province is not present in catalog");
    }
    if !value.office.is_empty()
        && value.province == "Barcelona"
        && !catalog.barcelona_offices.contains(&value.office)
    {
        reject("request.office is not present in Barcelona catalog");
    }
    if !matches!(value.procedure.as_str(), "huellas" | "regreso") {
        reject("request.procedure must be TIE fingerprints or autorización de regreso");
    }
    if value.subprocedure.as_ref().is_some_and(|s| !s.is_empty()) {
        reject("request.subprocedure must be null in the supported flows");
    }
    if value.procedure == "regreso" {
        if !matches!(value.document_type.as_str(), "NIE" | "DNI" | "PASSPORT") {
            reject("applicant.document_type must be NIE, DNI or PASSPORT for regreso");
        }
    } else if value.document_type != "NIE" {
        reject("applicant.document_type must be NIE");
    }
    if !matches!(value.fill_mode.as_str(), "manual" | "copy") {
        reject("applicant.fill_mode must be manual or copy");
    }
    if value.fill_mode == "copy" && !matches!(value.access_mode.as_str(), "clave" | "certificate") {
        reject("applicant.fill_mode=copy requires certificate or Cl@ve");
    }
    if value.fill_mode != "copy" {
        if value.procedure == "regreso" && value.document_type == "DNI" {
            if !valid_dni(&value.document_number) {
                reject("applicant.document_number is not a valid DNI");
            }
        } else if value.procedure == "regreso" && value.document_type == "PASSPORT" {
            if !valid_passport(&value.document_number) {
                reject(
                    "applicant.document_number must be a passport number of 1-20 letters or digits",
                );
            }
        } else if !valid_nie(&value.document_number) {
            reject("applicant.document_number is not a valid NIE");
        }
        if value.full_name.is_empty()
            || value.full_name.chars().count() > 50
            || value.full_name.chars().any(is_digit)
        {
            reject("applicant.full_name must be 1-50 characters without digits");
        }
    }
    if value.procedure != "regreso" && !catalog.nationalities.contains(&value.nationality) {
        reject("applicant.nationality is not present in catalog");
    }
    if !catalog.timezones.contains(&value.timezone) {
        reject("schedule.timezone is invalid");
    }
    ValidationReport {
        normalized: value,
        errors,
    }
}

/// Normalize Python int(string) digits without exposing private profile values.
pub fn legacy_integer_text(raw: &str) -> Option<String> {
    let text = raw.trim_matches(|c| is_space(c) && !matches!(c, '\u{1c}'..='\u{1f}'));
    let mut chars = text.chars().peekable();
    let mut out = String::new();
    if chars.peek().is_some_and(|c| matches!(c, '+' | '-')) {
        out.push(chars.next()?);
    }
    let mut digit = false;
    while let Some(c) = chars.next() {
        if let Some(value) = decimal(c) {
            out.push(char::from_digit(value, 10)?);
            digit = true;
        } else if c == '_' && digit && chars.peek().is_some_and(|c| decimal(*c).is_some()) {
            digit = false;
        } else {
            return None;
        }
    }
    if digit { Some(out) } else { None }
}

/// Desktop ProfileStore additionally requires contact fields for both login methods.
pub fn validate_desktop_profile(value: ProfileDraft, catalog: &ProfileCatalog) -> ValidationReport {
    let mut report = validate_profile(value, catalog);
    if matches!(
        report.normalized.access_mode.as_str(),
        "certificate" | "clave"
    ) {
        if !(6..=15).contains(&report.normalized.phone.chars().count())
            || !report
                .normalized
                .phone
                .chars()
                .all(|c| decimal(c).is_some())
        {
            report
                .errors
                .push("applicant.contact.phone: требуется телефон из 6–15 цифр".into());
        }
        if !valid_email(&report.normalized.email) {
            report
                .errors
                .push("applicant.contact.email: требуется адрес электронной почты".into());
        }
    }
    report
}
