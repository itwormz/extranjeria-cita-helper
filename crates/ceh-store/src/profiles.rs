//! Read-only desktop profiles; the exact original document remains opaque and unchanged.
use crate::legacy_yaml::{self, LegacyDocument, LegacyKey, RawValue, ReadError};
use ceh_core::{
    profiles::{
        ProfileCatalog, ProfileDraft, ValidationReport, legacy_integer_text,
        validate_desktop_profile,
    },
    timings::{AssistTimings, parse_timings},
};
use num_bigint::{BigInt, ToBigInt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    fmt,
    path::Path,
};
const TIE: &str =
    "POLICÍA-TOMA DE HUELLAS (EXPEDICIÓN DE TARJETA) INICIAL, RENOVACIÓN, DUPLICADO Y LEY 14/2013";
const REGRESO: &str = "AUTORIZACIÓN DE REGRESO";
const MAX_NORMALIZED: usize = 4 * 1024 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileLoadError {
    Read,
    Structure,
    Duplicate,
    Settings,
    Limit,
}
impl ProfileLoadError {
    pub fn code(self) -> &'static str {
        match self {
            Self::Read | Self::Limit => "read",
            Self::Structure => "structure",
            Self::Duplicate => "duplicate",
            Self::Settings => "settings",
        }
    }
}
impl fmt::Display for ProfileLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "legacy profiles: {}", self.code())
    }
}
impl std::error::Error for ProfileLoadError {}
impl From<ReadError> for ProfileLoadError {
    fn from(_: ReadError) -> Self {
        Self::Read
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopPreferences {
    pub dark: bool,
    pub diagnostics: bool,
    pub sound: bool,
    pub language: String,
    pub automatic_updates: bool,
    #[serde(default)]
    pub beta_updates: bool,
}
pub struct LoadedProfile {
    pub validation: ValidationReport,
    pub timings: Option<AssistTimings>,
    pub appointment_policy: Option<ceh_core::appointment_budget::AppointmentPolicy>,
    pub has_notifications: bool,
}
impl fmt::Debug for LoadedProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LoadedProfile([REDACTED])")
    }
}
pub struct ProfileBundle {
    pub profiles: Vec<LoadedProfile>,
    pub preferences: DesktopPreferences,
    pub last_profile_id: Option<String>,
    document: LegacyDocument,
}
impl fmt::Debug for ProfileBundle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ProfileBundle([REDACTED])")
    }
}
impl ProfileBundle {
    pub fn revision(&self) -> &str {
        self.document.revision()
    }
    pub fn raw(&self) -> &LegacyDocument {
        &self.document
    }
}
struct CopyBudget(usize);
impl CopyBudget {
    fn text(&mut self, value: &RawValue) -> Result<String, ProfileLoadError> {
        let size = match value {
            RawValue::String(s) | RawValue::Integer(s) | RawValue::Timestamp(s) => s.len(),
            _ => 64,
        };
        self.0 = self.0.checked_add(size).ok_or(ProfileLoadError::Limit)?;
        if self.0 > MAX_NORMALIZED {
            return Err(ProfileLoadError::Limit);
        }
        value.scalar_text().ok_or(ProfileLoadError::Structure)
    }
    fn field(
        &mut self,
        map: &RawValue,
        name: &str,
        default: &str,
    ) -> Result<String, ProfileLoadError> {
        if let Some(value) = map.get(name) {
            self.text(value)
        } else {
            Ok(default.into())
        }
    }
}
fn mapping<'a>(
    value: Option<&'a RawValue>,
    empty: &'a RawValue,
) -> Result<&'a RawValue, ProfileLoadError> {
    match value {
        None => Ok(empty),
        Some(v) if !v.truthy() => Ok(empty),
        Some(v) if v.is_mapping() => Ok(v),
        _ => Err(ProfileLoadError::Structure),
    }
}
fn bool_field(map: &RawValue, name: &str, default: bool) -> bool {
    map.get(name).map_or(default, RawValue::truthy)
}
fn integer_value(value: Option<&RawValue>, default: i64) -> Result<BigInt, ProfileLoadError> {
    match value {
        None => Ok(BigInt::from(default)),
        Some(RawValue::Bool(v)) => Ok(BigInt::from(u8::from(*v))),
        Some(RawValue::Integer(v)) => {
            BigInt::parse_bytes(v.as_bytes(), 10).ok_or(ProfileLoadError::Settings)
        }
        Some(RawValue::Float(v)) => v.to_bigint().ok_or(ProfileLoadError::Settings),
        Some(RawValue::String(v)) => legacy_integer_text(v)
            .and_then(|s| BigInt::parse_bytes(s.as_bytes(), 10))
            .ok_or(ProfileLoadError::Settings),
        _ => Err(ProfileLoadError::Settings),
    }
}
fn scalar_json(
    value: &RawValue,
    budget: &mut CopyBudget,
) -> Result<Option<Value>, ProfileLoadError> {
    Ok(Some(match value {
        RawValue::Null => Value::Null,
        RawValue::Bool(v) => Value::Bool(*v),
        RawValue::String(_) | RawValue::Timestamp(_) => Value::String(budget.text(value)?),
        RawValue::Integer(_) => {
            let text = budget.text(value)?;
            text.parse::<serde_json::Number>()
                .map(Value::Number)
                .unwrap_or(Value::String(text))
        }
        RawValue::Float(v) => serde_json::Number::from_f64(*v)
            .map(Value::Number)
            .unwrap_or_else(|| Value::String(v.to_string())),
        _ => return Ok(None),
    }))
}
fn timing_json(
    value: &RawValue,
    budget: &mut CopyBudget,
) -> Result<Option<Value>, ProfileLoadError> {
    if let RawValue::Mapping(map) = value {
        let defaults = serde_json::to_value(AssistTimings::default()).expect("fixed timing schema");
        let mut result = serde_json::Map::new();
        for name in defaults.as_object().expect("timing mapping").keys() {
            let Some(value) = map.get(&LegacyKey::String(name.clone())) else {
                continue;
            };
            let converted = if let RawValue::Sequence(values) = value.as_ref() {
                if values.len() > 10 {
                    return Ok(None);
                }
                let mut list = vec![];
                for v in values {
                    let Some(v) = scalar_json(v, budget)? else {
                        return Ok(None);
                    };
                    list.push(v);
                }
                Value::Array(list)
            } else {
                let Some(v) = scalar_json(value, budget)? else {
                    return Ok(None);
                };
                v
            };
            result.insert(name.clone(), converted);
        }
        Ok(Some(Value::Object(result)))
    } else {
        scalar_json(value, budget)
    }
}
fn global_settings(
    raw: &RawValue,
    catalog: &ProfileCatalog,
    empty: &RawValue,
    budget: &mut CopyBudget,
) -> Result<String, ProfileLoadError> {
    let timezone = budget.field(raw, "timezone", "Europe/Madrid")?;
    if !catalog.timezones.contains(&timezone) {
        return Err(ProfileLoadError::Settings);
    }
    let retry = mapping(raw.get("retry"), empty)?;
    let diagnostics = mapping(raw.get("diagnostics"), empty)?;
    let browser = mapping(raw.get("browser"), empty)?;
    let bootstrap = mapping(browser.get("f5_bootstrap"), empty)?;
    mapping(raw.get("assist"), empty)?;
    let number = integer_value(raw.get("navigation_timeout_seconds"), 45)?;
    if number < BigInt::from(5) {
        return Err(ProfileLoadError::Settings);
    }
    let number = integer_value(retry.get("page_reload_attempts"), 1)?;
    if number < BigInt::from(0) || number > BigInt::from(3) {
        return Err(ProfileLoadError::Settings);
    }
    let preset = budget.field(browser, "preset", "localized")?.to_lowercase();
    let engine = budget.field(browser, "engine", "webkit")?.to_lowercase();
    let mode = budget.field(browser, "mode", "headed")?.to_lowercase();
    let session = budget.field(browser, "session", "fresh")?.to_lowercase();
    if !matches!(preset.as_str(), "localized" | "vanilla")
        || !matches!(engine.as_str(), "chromium" | "webkit" | "camoufox")
        || !matches!(mode.as_str(), "headed" | "headless")
        || !matches!(session.as_str(), "fresh" | "persistent")
    {
        return Err(ProfileLoadError::Settings);
    }
    if !budget
        .field(browser, "executable_path", "")?
        .trim()
        .is_empty()
        && engine != "chromium"
    {
        return Err(ProfileLoadError::Settings);
    }
    if budget
        .field(browser, "profile_directory", "data/browser-profiles")?
        .trim()
        .is_empty()
    {
        return Err(ProfileLoadError::Settings);
    }
    for (map, key, field) in [
        (browser, "action_delay_seconds", "action_delay_seconds"),
        (bootstrap, "wait_seconds", "bootstrap_seconds"),
    ] {
        if let Some(value) = map.get(key) {
            // Legacy global Settings takes the first two values; profile timings have their own stricter schema.
            let pair = match value {
                RawValue::Sequence(v) if v.len() >= 2 => vec![v[0].clone(), v[1].clone()],
                RawValue::String(s) if s.chars().count() >= 2 => s
                    .chars()
                    .take(2)
                    .map(|c| std::sync::Arc::new(RawValue::String(c.to_string())))
                    .collect(),
                _ => return Err(ProfileLoadError::Settings),
            };
            let numbers = pair
                .iter()
                .map(|v| {
                    if let RawValue::Bool(v) = v.as_ref() {
                        Ok(json!(u8::from(*v)))
                    } else {
                        scalar_json(v, budget)?
                            .ok_or(ProfileLoadError::Settings)
                            .map_err(|_| ProfileLoadError::Settings)
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut object = serde_json::Map::new();
            object.insert(field.into(), Value::Array(numbers));
            parse_timings(&Value::Object(object)).map_err(|_| ProfileLoadError::Settings)?;
        }
    }
    let number = integer_value(bootstrap.get("block_recovery_attempts"), 1)?;
    if number < BigInt::from(0) || number > BigInt::from(1) {
        return Err(ProfileLoadError::Settings);
    }
    if integer_value(bootstrap.get("block_recovery_delay_seconds"), 30)? < BigInt::from(10) {
        return Err(ProfileLoadError::Settings);
    }
    integer_value(diagnostics.get("retention_days"), 3)?;
    Ok(timezone)
}
fn parse_appointment_policy(
    raw: &RawValue,
    budget: &mut CopyBudget,
) -> Result<ceh_core::appointment_budget::AppointmentPolicy, ProfileLoadError> {
    use ceh_core::appointment_budget::AppointmentPolicy;
    if !raw.is_mapping() {
        return Err(ProfileLoadError::Settings);
    }
    let defaults = serde_json::to_value(AppointmentPolicy::default()).map_err(|_| ProfileLoadError::Settings)?;
    let mut fields = serde_json::Map::new();
    for key in defaults.as_object().ok_or(ProfileLoadError::Settings)?.keys() {
        if let Some(value) = raw.get(key) {
            let value = scalar_json(value, budget)?.ok_or(ProfileLoadError::Settings)?;
            fields.insert(key.clone(), value);
        }
    }
    let policy: AppointmentPolicy = serde_json::from_value(Value::Object(fields)).map_err(|_| ProfileLoadError::Settings)?;
    if !policy.is_valid() {
        return Err(ProfileLoadError::Settings);
    }
    Ok(policy.without_minimum_pause())
}

fn convert(
    raw: &RawValue,
    timezone: &str,
    catalog: &ProfileCatalog,
    empty: &RawValue,
    budget: &mut CopyBudget,
) -> Result<LoadedProfile, ProfileLoadError> {
    let request = mapping(raw.get("request"), empty)?;
    let applicant = mapping(raw.get("applicant"), empty)?;
    let schedule = mapping(raw.get("schedule"), empty)?;
    let contact = mapping(applicant.get("contact"), empty)?;
    let id = budget.field(raw, "id", "")?;
    let label = budget.field(raw, "label", &id)?;
    let procedure = budget.field(request, "procedure", "")?;
    let procedure = match procedure.as_str() {
        TIE => "huellas".into(),
        REGRESO => "regreso".into(),
        _ => procedure,
    };
    let sub = request.get("subprocedure");
    let subprocedure = match sub {
        None | Some(RawValue::Null) => None,
        Some(v) => Some(budget.text(v)?),
    };
    let draft = ProfileDraft {
        id,
        label,
        enabled: bool_field(raw, "enabled", true),
        procedure,
        province: budget.field(request, "province", "")?,
        office: budget.field(request, "office", "")?,
        document_type: budget.field(applicant, "document_type", "")?,
        document_number: budget.field(applicant, "document_number", "")?,
        full_name: budget.field(applicant, "full_name", "")?,
        nationality: budget.field(applicant, "nationality", "")?,
        phone: budget.field(contact, "phone", "")?,
        email: budget.field(contact, "email", "")?,
        access_mode: budget.field(request, "access_mode", "without_clave")?,
        clave_method: budget.field(request, "clave_method", "movil")?,
        fill_mode: budget.field(applicant, "fill_mode", "manual")?,
        no_slots_mode: budget.field(request, "no_slots_mode", "salir")?,
        applicant_no_slots_mode: budget.field(request, "applicant_no_slots_mode", "wait")?,
        subprocedure,
        timezone: budget.field(schedule, "timezone", timezone)?,
        browser_session: "persistent".into(),
        browser: budget.field(request, "browser", "firefox")?,
    };
    let mut validation = validate_desktop_profile(draft, catalog);
    if sub.is_some_and(|v| !v.truthy()) {
        validation.normalized.subprocedure = None;
    }
    let converted = if let Some(v) = raw.get("timings") {
        timing_json(v, budget)?
    } else {
        Some(Value::Null)
    };
    let timings = match converted.as_ref().and_then(|v| parse_timings(v).ok()) {
        Some(v) => Some(v),
        None => {
            validation.errors.push("Invalid profile timings".into());
            None
        }
    };
    let appointment_policy = match request.get("appointment_protection") {
        None => Some(ceh_core::appointment_budget::AppointmentPolicy::default().without_minimum_pause()),
        Some(raw) => parse_appointment_policy(raw, budget).ok(),
    };
    if appointment_policy.is_none() {
        validation.errors.push("Invalid appointment protection settings".into());
    }
    Ok(LoadedProfile {
        validation,
        timings,
        appointment_policy,
        has_notifications: raw.get("notifications").is_some(),
    })
}
fn load_document(
    document: LegacyDocument,
    catalog: &ProfileCatalog,
    system_language: &str,
) -> Result<ProfileBundle, ProfileLoadError> {
    let empty = RawValue::Mapping(BTreeMap::new());
    let root = if document.root().truthy() {
        document.root()
    } else {
        &empty
    };
    if !root.is_mapping() {
        return Err(ProfileLoadError::Read);
    }
    let tasks = if let Some(v) = root.get("tasks") {
        v.sequence().ok_or(ProfileLoadError::Read)?
    } else {
        &[]
    };
    if tasks.iter().any(|v| !v.is_mapping()) {
        return Err(ProfileLoadError::Read);
    }
    for name in ["settings", "desktop"] {
        if root
            .get(name)
            .is_some_and(|v| !matches!(v, RawValue::Null) && !v.is_mapping())
        {
            return Err(ProfileLoadError::Read);
        }
    }
    let settings = mapping(root.get("settings"), &empty)?;
    let desktop = mapping(root.get("desktop"), &empty)?;
    let mut budget = CopyBudget(0);
    let timezone = global_settings(settings, catalog, &empty, &mut budget).map_err(|error| {
        if !tasks.is_empty() && error != ProfileLoadError::Limit {
            ProfileLoadError::Structure
        } else {
            error
        }
    })?;
    let mut profiles = vec![];
    let mut ids = HashSet::new();
    for raw in tasks {
        let profile = convert(raw, &timezone, catalog, &empty, &mut budget)?;
        if !ids.insert(profile.validation.normalized.id.clone()) {
            return Err(ProfileLoadError::Duplicate);
        }
        profiles.push(profile);
    }
    let diagnostics = mapping(settings.get("diagnostics"), &empty)?;
    let system = if matches!(system_language, "ru" | "en" | "es") {
        system_language
    } else {
        "en"
    };
    let language = match desktop.get("language") {
        Some(RawValue::String(s)) if matches!(s.as_str(), "ru" | "en" | "es") => s.clone(),
        _ => system.into(),
    };
    let preferences = DesktopPreferences {
        dark: bool_field(desktop, "dark", false),
        diagnostics: bool_field(
            desktop,
            "diagnostics",
            bool_field(diagnostics, "enabled", false),
        ),
        sound: bool_field(desktop, "sound", true),
        language,
        automatic_updates: bool_field(desktop, "automatic_updates", true),
        beta_updates: matches!(desktop.get("beta_updates"),Some(RawValue::Bool(true))),
    };
    let last_profile_id = match desktop.get("last_profile_id") {
        Some(RawValue::String(s)) => Some(s.clone()),
        _ => None,
    };
    Ok(ProfileBundle {
        profiles,
        preferences,
        last_profile_id,
        document,
    })
}
pub fn decode_profiles(
    bytes: &[u8],
    catalog: &ProfileCatalog,
    system_language: &str,
) -> Result<ProfileBundle, ProfileLoadError> {
    load_document(
        legacy_yaml::parse_document(bytes)?,
        catalog,
        system_language,
    )
}
pub fn read_profiles(
    path: &Path,
    catalog: &ProfileCatalog,
    system_language: &str,
) -> Result<ProfileBundle, ProfileLoadError> {
    match legacy_yaml::read_document(path) {
        Ok(document) => load_document(document, catalog, system_language),
        Err(ReadError::NotFound) => decode_profiles(&[], catalog, system_language),
        Err(error) => Err(error.into()),
    }
}
