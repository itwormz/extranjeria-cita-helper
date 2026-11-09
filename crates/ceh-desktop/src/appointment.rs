//! Validated local appointment note. No browser access and no public export here.
use chrono::{Datelike, NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppointmentDraft {
    pub date: String,
    pub time: String,
    #[serde(default)]
    pub office: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub confirmation: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppointmentError {
    Date,
    Time,
    Field,
    Timezone,
}
impl AppointmentDraft {
    /// The trusted profile/store supplies timezone; it is never accepted from the UI draft.
    pub fn validated(&self, timezone: &str) -> Result<Value, AppointmentError> {
        let date = NaiveDate::parse_from_str(&self.date, "%Y-%m-%d")
            .ok()
            .filter(|value| {
                value.year() > 0
                    && value.year() <= 9999
                    && value.format("%Y-%m-%d").to_string() == self.date
            })
            .ok_or(AppointmentError::Date)?;
        let time = NaiveTime::parse_from_str(&self.time, "%H:%M")
            .ok()
            .filter(|value| value.format("%H:%M").to_string() == self.time)
            .ok_or(AppointmentError::Time)?;
        timezone
            .parse::<chrono_tz::Tz>()
            .map_err(|_| AppointmentError::Timezone)?;
        let field = |text: &str, limit: usize| {
            let value = text.trim();
            if value.chars().count() > limit || value.chars().any(char::is_control) {
                Err(AppointmentError::Field)
            } else {
                Ok(value.to_owned())
            }
        };
        Ok(
            json!({"date":date.format("%Y-%m-%d").to_string(),"time":time.format("%H:%M").to_string(),"office":field(&self.office,2048)?,"address":field(&self.address,4096)?,"confirmation":field(&self.confirmation,512)?,"timezone":timezone}),
        )
    }
}

/// Save a local achievement using a trusted profile and server-side statistics.
/// This adapter has no browser handle: recording cannot close a booking page.
pub fn record_at(
    root: &std::path::Path,
    profile_id: &str,
    draft: &AppointmentDraft,
    at: chrono::DateTime<chrono::Utc>,
) -> Result<Value, &'static str> {
    let profiles =
        ceh_store::profiles::read_profiles(&root.join("tasks.yml"), crate::catalog(), "ru")
            .map_err(|_| "profiles_unavailable")?;
    let profile = profiles
        .profiles
        .iter()
        .find(|profile| profile.validation.normalized.id == profile_id)
        .ok_or("profile_not_found")?;
    let appointment = draft
        .validated(&profile.validation.normalized.timezone)
        .map_err(|error| match error {
            AppointmentError::Date => "appointment_date_invalid",
            AppointmentError::Time => "appointment_time_invalid",
            AppointmentError::Field => "appointment_field_invalid",
            AppointmentError::Timezone => "appointment_timezone_invalid",
        })?;
    let statistics = ceh_store::statistics::dashboard_file(
        &root.join("data/profile-statistics.sqlite3"),
        profile_id,
        at,
    )
    .map_err(|_| "statistics_unavailable")?
    .map(|dashboard| dashboard.all)
    .unwrap_or_else(|| json!({"legacy_unknown_fields":["checks","offices_seen","unique_offices","server_errors","rate_limited","blocked","booking_screens","flow_passes","actions_performed","pages_processed","active_seconds","started_at","first_available_seconds"]}));
    ceh_services::exports::AchievementStore::new(&root.join("data/achievements.json"))
        .record(profile_id, &statistics, &appointment, at)
        .map_err(|_| "achievements_unavailable")
}

#[derive(Serialize)]
pub struct AppointmentContext {
    pub records: Vec<Value>,
    pub offices: Vec<String>,
    pub selected_office: String,
    pub today: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivateField {
    Office,
    Address,
    Confirmation,
}
fn profile(
    root: &std::path::Path,
    id: &str,
) -> Result<ceh_core::profiles::ProfileDraft, &'static str> {
    let profiles =
        ceh_store::profiles::read_profiles(&root.join("tasks.yml"), crate::catalog(), "ru")
            .map_err(|_| "profiles_unavailable")?;
    profiles
        .profiles
        .into_iter()
        .find(|p| p.validation.normalized.id == id)
        .map(|p| p.validation.normalized)
        .ok_or("profile_not_found")
}
fn records(root: &std::path::Path, id: &str) -> Result<Vec<Value>, &'static str> {
    let path = root.join("data/achievements.json");
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Ok(m) if m.is_file() => (),
        _ => return Err("achievements_unavailable"),
    }
    let rows = ceh_services::exports::AchievementStore::new(&path)
        .list(id)
        .map_err(|_| "achievements_unavailable")?;
    rows.into_iter().map(|r|{
  if !r["id"].as_str().is_some_and(|s|!s.is_empty()&&s.len()<=128)||!r["recorded_at"].as_str().is_some_and(|s|chrono::DateTime::parse_from_rfc3339(s).is_ok())||!r["statistics"].is_object()||!r["appointment"].is_object(){return Err("achievements_unavailable");}
  let mut value=json!({"id":r["id"],"recorded_at":r["recorded_at"],"statistics":r["statistics"],"appointment":ceh_services::exports::appointment_data(&r["appointment"])});
  if let Some(quality)=r.get("statistics_provenance"){value["statistics_provenance"]=quality.clone();}
  Ok(value)
 }).collect()
}
/// Read only existing local data; a missing store is empty, never an implicit migration.
pub fn context_at(
    root: &std::path::Path,
    id: &str,
    at: chrono::DateTime<chrono::Utc>,
) -> Result<AppointmentContext, &'static str> {
    let profile = profile(root, id)?;
    let zone = profile
        .timezone
        .parse::<chrono_tz::Tz>()
        .map_err(|_| "appointment_timezone_invalid")?;
    let (labels, last) = ceh_store::statistics::appointment_offices_file(
        &root.join("data/profile-statistics.sqlite3"),
        id,
        at,
    )
    .map_err(|_| "statistics_unavailable")?
    .unwrap_or_default();
    let prefix = format!("{}: ", profile.province);
    let strip = |label: &str| {
        label
            .strip_prefix(&prefix)
            .map(str::to_owned)
            .or_else(|| (!label.contains(": ")).then(|| label.to_owned()))
    };
    let offices = labels.iter().filter_map(|label| strip(label)).collect();
    let selected_office = strip(&last).unwrap_or_default();
    Ok(AppointmentContext {
        records: records(root, id)?,
        offices,
        selected_office,
        today: at.with_timezone(&zone).format("%Y-%m-%d").to_string(),
    })
}
pub fn update(
    root: &std::path::Path,
    id: &str,
    achievement_id: &str,
    draft: &AppointmentDraft,
) -> Result<Value, &'static str> {
    let p = profile(root, id)?;
    let appointment = draft.validated(&p.timezone).map_err(validation_code)?;
    // Resolve the record inside the selected profile, never accept client-supplied statistics.
    if !records(root, id)?.iter().any(|r| r["id"] == achievement_id) {
        return Err("achievement_not_found");
    }
    ceh_services::exports::AchievementStore::new(&root.join("data/achievements.json"))
        .update_appointment(id, achievement_id, &appointment)
        .map_err(|_| "achievements_unavailable")
}
fn validation_code(error: AppointmentError) -> &'static str {
    match error {
        AppointmentError::Date => "appointment_date_invalid",
        AppointmentError::Time => "appointment_time_invalid",
        AppointmentError::Field => "appointment_field_invalid",
        AppointmentError::Timezone => "appointment_timezone_invalid",
    }
}
/// Return only the user-selected local field, not the full achievement/private payload.
pub fn private_field(
    root: &std::path::Path,
    id: &str,
    achievement_id: &str,
    field: PrivateField,
) -> Result<String, &'static str> {
    profile(root, id)?;
    let row = records(root, id)?
        .into_iter()
        .find(|r| r["id"] == achievement_id)
        .ok_or("achievement_not_found")?;
    let key = match field {
        PrivateField::Office => "office",
        PrivateField::Address => "address",
        PrivateField::Confirmation => "confirmation",
    };
    Ok(row["appointment"][key].as_str().unwrap_or("").to_owned())
}

/// Clipboard changes occur only through an explicit selected-field command.
#[cfg(feature = "shell")]
pub fn copy_field(
    root: &std::path::Path,
    id: &str,
    achievement_id: &str,
    field: PrivateField,
) -> Result<(), &'static str> {
    let text = private_field(root, id, achievement_id, field)?;
    crate::notification_actions::clipboard(&text).map_err(|_| "clipboard_unavailable")
}

pub(crate) fn achievement(
    root: &std::path::Path,
    id: &str,
    key: &str,
) -> Result<Value, &'static str> {
    profile(root, id)?;
    records(root, id)?
        .into_iter()
        .find(|r| r["id"] == key)
        .ok_or("achievement_not_found")
}
