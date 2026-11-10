//! Offline public achievement models and private calendar exports.
use chrono::{
    DateTime, Datelike, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, Offset, SecondsFormat,
    TimeZone, Timelike, Utc,
};
use chrono_tz::Tz;
use serde_json::{Value, json};
mod card;
pub use card::card_layout;
const COUNTS: &[&str] = &[
    "checks",
    "offices_seen",
    "unique_offices",
    "server_errors",
    "rate_limited",
    "blocked",
    "booking_screens",
    "flow_passes",
    "actions_performed",
    "pages_processed",
];
const TITLE: &str = "Extranjería — Cita";
const EN: [&str; 14] = [
    "Appointment booked",
    "days of searching",
    "Checks",
    "Active search",
    "Office appearances",
    "Unique offices",
    "Booking screens",
    "First availability",
    "Server errors",
    "HTTP 429 responses",
    "Blocked responses",
    "Passes from the start",
    "Actions performed",
    "Pages processed",
];
const ES: [&str; 14] = [
    "Cita conseguida",
    "días de búsqueda",
    "Comprobaciones",
    "Búsqueda activa",
    "Apariciones de oficinas",
    "Oficinas distintas",
    "Pantallas de cita",
    "Primera disponibilidad",
    "Errores del servidor",
    "Respuestas HTTP 429",
    "Bloqueos",
    "Recorridos desde el inicio",
    "Acciones realizadas",
    "Páginas procesadas",
];
const RU: [&str; 14] = [
    "Сита получена",
    "дней поиска",
    "Проверки",
    "Активный поиск",
    "Появления офисов",
    "Разные офисы",
    "Экраны записи",
    "Первая доступность",
    "Ошибки сервера",
    "Ответы HTTP 429",
    "Блокировки",
    "Проходы с начала",
    "Действий выполнено",
    "Страниц обработано",
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportError {
    InvalidAppointment,
    Unavailable,
    InvalidStore,
    Missing,
    Busy,
}
fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
fn normalized_time(value: &Value) -> Option<String> {
    let raw = value.as_str()?;
    if let Ok(value) = DateTime::parse_from_rfc3339(raw)
        && value.year() > 0
        && value.year() <= 9999
    {
        return Some(value.to_rfc3339_opts(
            if value.timestamp_subsec_micros() == 0 {
                SecondsFormat::Secs
            } else {
                SecondsFormat::Micros
            },
            false,
        ));
    }
    let parsed = [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M",
    ]
    .into_iter()
    .find_map(|pattern| NaiveDateTime::parse_from_str(raw, pattern).ok())
    .or_else(|| {
        NaiveDate::parse_from_str(raw, "%Y-%m-%d")
            .ok()
            .map(|date| date.and_time(NaiveTime::MIN))
    })?;
    if parsed.year() < 1 || parsed.year() > 9999 {
        return None;
    }
    Some(if parsed.nanosecond() == 0 {
        parsed.format("%Y-%m-%dT%H:%M:%S").to_string()
    } else {
        format!(
            "{}.{:06}",
            parsed.format("%Y-%m-%dT%H:%M:%S"),
            parsed.nanosecond() / 1000
        )
    })
}
fn instant(value: &Value) -> Option<DateTime<Utc>> {
    let normalized = normalized_time(value)?;
    DateTime::parse_from_rfc3339(&normalized)
        .map(|v| v.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(&normalized, "%Y-%m-%dT%H:%M:%S%.f")
                .ok()
                .map(|v| v.and_utc())
        })
}
pub fn snapshot(statistics: &Value) -> Value {
    let mut result = serde_json::Map::new();
    for key in COUNTS
        .iter()
        .copied()
        .chain(["active_seconds", "first_available_seconds"])
    {
        let original = &statistics[key];
        let effort = ["actions_performed", "pages_processed"].contains(&key);
        let absent = original.is_null() && (effort || key == "first_available_seconds");
        let inactive = effort
            && statistics
                .get("effort_started_at")
                .is_some_and(|v| v.is_null() || v == "" || v == false);
        let value = if absent || inactive {
            Value::Null
        } else if original.as_f64().is_some_and(|v| v.is_finite() && v >= 0.0) {
            original.clone()
        } else if effort {
            Value::Null
        } else {
            json!(0)
        };
        result.insert(key.into(), value);
    }
    for key in ["started_at", "first_available_at"] {
        result.insert(key.into(), json!(normalized_time(&statistics[key])));
    }
    // Public counters (Python 0.3.x): unknown legacy values stay null instead of becoming 0.
    let unknown_fields = statistics["legacy_unknown_fields"].as_array();
    let any_activity = ["checks", "active_seconds", "offices_seen", "booking_screens"]
        .iter()
        .any(|key| truthy(&statistics[*key]));
    for key in ["days", "banners", "no_slots", "cod_oper_count"] {
        let value = &statistics[key];
        let unknown = unknown_fields.is_some_and(|fields| fields.iter().any(|f| f == key))
            || (key == "days" && !truthy(value) && any_activity);
        result.insert(key.into(), if !unknown && non_negative_number(value) { value.clone() } else { Value::Null });
    }
    // First office / first booking: seconds count only with an `exact` or `minimum` quality.
    for prefix in ["first_office", "first_booking"] {
        let quality = match statistics[format!("{prefix}_active_quality")].as_str() {
            Some(quality @ ("exact" | "minimum")) => quality,
            _ => "unknown",
        };
        let seconds = &statistics[format!("{prefix}_seconds")];
        result.insert(format!("{prefix}_active_quality"), json!(quality));
        result.insert(
            format!("{prefix}_seconds"),
            if quality != "unknown" && non_negative_number(seconds) { seconds.clone() } else { Value::Null },
        );
    }
    Value::Object(result)
}
/// Python truthiness of a JSON value.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|v| v != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}
/// A finite number >= 0 that is not a boolean (Python `type(value) in (int, float)`).
fn non_negative_number(value: &Value) -> bool {
    value.as_f64().is_some_and(|v| v.is_finite() && v >= 0.0)
}
pub fn appointment_data(value: &Value) -> Value {
    let mut result = serde_json::Map::new();
    for key in [
        "date",
        "time",
        "office",
        "address",
        "confirmation",
        "timezone",
    ] {
        let raw = &value[key];
        let text = match raw {
            Value::String(text) => text.clone(),
            Value::Null => String::new(),
            Value::Bool(false) => String::new(),
            Value::Number(number) if number.as_f64() == Some(0.0) => String::new(),
            other => other.to_string(),
        };
        result.insert(key.into(), json!(text));
    }
    Value::Object(result)
}
fn duration(value: &Value, language: &str) -> String {
    let Some(seconds) = value.as_f64() else {
        return "—".into();
    };
    let minutes = (seconds as u64) / 60;
    let (hour, minute) = if language == "ru" {
        ("ч", "мин")
    } else {
        ("h", "min")
    };
    if minutes >= 60 && minutes.is_multiple_of(60) {
        format!("{} {hour}", minutes / 60)
    } else if minutes >= 60 {
        format!("{} {hour} {} {minute}", minutes / 60, minutes % 60)
    } else {
        format!("{minutes} {minute}")
    }
}
fn days(achievement: &Value) -> Option<i64> {
    let stats = snapshot(&achievement["statistics"]);
    let start = instant(&stats["started_at"])?;
    let end = instant(&achievement["recorded_at"])?;
    Some((end - start).num_days().max(0))
}
pub fn public_duration(achievement: &Value, language: &str) -> String {
    let Some(days) = days(achievement) else {
        return "—".into();
    };
    if days == 0 {
        return match language {
            "ru" => "<1 дня поиска",
            "es" => "<1 día de búsqueda",
            _ => "<1 day of searching",
        }
        .into();
    }
    match language {
        "ru" => {
            let form = if days % 10 == 1 && days % 100 != 11 {
                "день"
            } else if [2, 3, 4].contains(&(days % 10)) && ![12, 13, 14].contains(&(days % 100)) {
                "дня"
            } else {
                "дней"
            };
            format!("{days} {form} поиска")
        }
        "es" => format!(
            "{days} {} de búsqueda",
            if days == 1 { "día" } else { "días" }
        ),
        _ => format!(
            "{days} {} of searching",
            if days == 1 { "day" } else { "days" }
        ),
    }
}
fn labels(language: &str) -> &'static [&'static str; 14] {
    match language {
        "ru" => &RU,
        "es" => &ES,
        _ => &EN,
    }
}
fn credit(language: &str) -> &'static str {
    match language {
        "ru" => "Получено с помощью Cita Extranjeria Helper",
        "es" => "Conseguido con Cita Extranjeria Helper",
        _ => "Obtained with Cita Extranjeria Helper",
    }
}
/// The public statistics of an achievement; fields the legacy import could not know stay unknown.
fn public_stats(achievement: &Value) -> Value {
    let mut stats = snapshot(&achievement["statistics"]);
    if let Some(fields) = achievement["statistics_provenance"]["unknown_fields"].as_array() {
        for field in fields.iter().filter_map(Value::as_str) {
            if stats
                .get(field)
                .is_some_and(|value| value.as_f64() == Some(0.0))
            {
                stats[field] = Value::Null;
            }
        }
    }
    stats
}
pub fn public_card(achievement: &Value, language: &str) -> Value {
    let stats = public_stats(achievement);
    let names = labels(language);
    let values = [
        stats["checks"].clone(),
        json!(duration(&stats["active_seconds"], language)),
        stats["offices_seen"].clone(),
        stats["unique_offices"].clone(),
        stats["booking_screens"].clone(),
        json!(duration(&stats["first_available_seconds"], language)),
        stats["server_errors"].clone(),
        stats["rate_limited"].clone(),
        stats["blocked"].clone(),
        stats["flow_passes"].clone(),
        stats["actions_performed"].clone(),
        stats["pages_processed"].clone(),
    ];
    let rows = names[2..]
        .iter()
        .zip(values)
        .map(|(label, value)| {
            json!([
                label,
                match value {
                    Value::Null => "—".into(),
                    Value::String(value) => value,
                    value => value.to_string(),
                }
            ])
        })
        .collect::<Vec<_>>();
    json!({"title":names[0],"duration":public_duration(achievement,language),"rows":rows,"credit":credit(language),"width":1200,"height":900,"top_strip_height":7})
}
/// (statistics key, Russian source label, icon) for the detailed post, in Python 0.3.6 order.
const POST_METRICS: [(&str, &str, &str); 17] = [
    ("checks", "Проверок сит", "🔎"),
    ("active_seconds", "Активный поиск", "⏱"),
    ("days", "Дни с поиском", "🗓"),
    ("offices_seen", "Появлений офисов", "🏢"),
    ("unique_offices", "Разных офисов", "📍"),
    ("booking_screens", "Появления выбора даты", "📅"),
    ("banners", "«Сит нет» до запроса", "📋"),
    ("no_slots", "Ответы «Сит нет» на запрос записи", "💬"),
    ("server_errors", "Ошибок сервера", "🔧"),
    ("rate_limited", "Ограничений HTTP 429", "🚦"),
    ("blocked", "Ограничения доступа", "🛡"),
    ("flow_passes", "Повторные проходы", "🔄"),
    ("cod_oper_count", "Ответы «Сит нет» с cod. oper", "⚠"),
    ("actions_performed", "Действий выполнено", "🖱"),
    ("pages_processed", "Страниц обработано", "📄"),
    ("first_office_seconds", "Первый офис", "🕒"),
    ("first_booking_seconds", "Первый выбор даты", "📆"),
];
fn post_value(stats: &Value, key: &str, language: &str) -> String {
    let value = &stats[key];
    if value.is_null() {
        return "—".into();
    }
    if !key.ends_with("_seconds") {
        return value.to_string();
    }
    let seconds = value.as_f64().unwrap_or(0.0);
    let text = if seconds < 60.0 {
        format!("{} {}", seconds as i64, crate::notifications::tr("с", language))
    } else {
        duration(value, language)
    };
    let quality = format!("{}_active_quality", key.trim_end_matches("_seconds"));
    if stats[quality.as_str()] == "minimum" {
        format!("≥ {text}")
    } else {
        text
    }
}
pub fn public_post(achievement: &Value, language: &str, detailed: bool) -> String {
    let card = public_card(achievement, language);
    let mut lines = vec![format!("🎉 {}!", text(&card, "title"))];
    let duration = text(&card, "duration");
    if duration != "—" {
        lines.push(format!("{duration}."));
    }
    if detailed {
        let stats = public_stats(achievement);
        for (key, label, icon) in POST_METRICS {
            lines.push(format!(
                "{icon} {}: {}",
                crate::notifications::tr(label, language),
                post_value(&stats, key, language)
            ));
        }
    }
    lines.push(credit(language).into());
    lines.join("\n")
}
pub fn share_url(network: &str) -> Option<&'static str> {
    match network {
        "telegram" => Some("https://web.telegram.org/"),
        "x" => Some("https://x.com/"),
        "facebook" => Some("https://www.facebook.com/"),
        "instagram" => Some("https://www.instagram.com/"),
        "reddit" => Some("https://www.reddit.com/submit"),
        _ => None,
    }
}
fn appointment(achievement: &Value) -> Result<(NaiveDateTime, Tz, String), ExportError> {
    let value = &achievement["appointment"];
    let date = text(value, "date");
    let time = text(value, "time");
    let zone = match text(value, "timezone") {
        "" | "Europe/Madrid" => chrono_tz::Europe::Madrid,
        "Atlantic/Canary" => chrono_tz::Atlantic::Canary,
        _ => return Err(ExportError::InvalidAppointment),
    };
    if date.len() != 10
        || time.len() != 5
        || !date.bytes().enumerate().all(|(i, c)| {
            if [4, 7].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
        || !time.bytes().enumerate().all(|(i, c)| {
            if i == 2 {
                c == b':'
            } else {
                c.is_ascii_digit()
            }
        })
    {
        return Err(ExportError::InvalidAppointment);
    }
    let date =
        NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| ExportError::InvalidAppointment)?;
    if date.year() < 1 {
        return Err(ExportError::InvalidAppointment);
    }
    let time =
        NaiveTime::parse_from_str(time, "%H:%M").map_err(|_| ExportError::InvalidAppointment)?;
    let location = [text(value, "office"), text(value, "address")]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    Ok((date.and_time(time), zone, location))
}
fn as_utc(local: NaiveDateTime, zone: Tz) -> Result<DateTime<Utc>, ExportError> {
    match zone.from_local_datetime(&local) {
        LocalResult::Single(value) => Ok(value.with_timezone(&Utc)),
        LocalResult::Ambiguous(first, second) => Ok(first.min(second).with_timezone(&Utc)),
        LocalResult::None => {
            for minute in 1..=1440 {
                let Some(before) = local.checked_sub_signed(chrono::Duration::minutes(minute))
                else {
                    break;
                };
                if let Some(before) = zone.from_local_datetime(&before).earliest() {
                    let offset = before.offset().fix();
                    return offset
                        .from_local_datetime(&local)
                        .single()
                        .map(|v| v.with_timezone(&Utc))
                        .ok_or(ExportError::InvalidAppointment);
                }
            }
            Err(ExportError::InvalidAppointment)
        }
    }
}
pub fn google_calendar_url(achievement: &Value) -> Result<String, ExportError> {
    let (local, zone, location) = appointment(achievement)?;
    let instant = as_utc(local, zone)?.format("%Y%m%dT%H%M%SZ").to_string();
    let mut url = reqwest::Url::parse("https://calendar.google.com/calendar/render")
        .expect("fixed calendar URL");
    url.query_pairs_mut().extend_pairs([
        ("action", "TEMPLATE"),
        ("text", TITLE),
        ("dates", &format!("{instant}/{instant}")),
        ("ctz", zone.name()),
        ("location", &location),
    ]);
    Ok(url.into())
}
fn ics_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\\n")
        .replace(';', "\\;")
        .replace(',', "\\,")
}
fn fold(value: &str) -> String {
    let mut result = String::new();
    let mut bytes = 0;
    for c in value.chars() {
        if bytes + c.len_utf8() > 75 {
            result.push_str("\r\n ");
            bytes = 1;
        }
        result.push(c);
        bytes += c.len_utf8();
    }
    result
}
pub fn calendar(achievement: &Value, at: DateTime<Utc>) -> Result<String, ExportError> {
    let (local, zone, location) = appointment(achievement)?;
    let identifier = uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_URL,
        format!("cita-achievement:{}", text(achievement, "id")).as_bytes(),
    );
    let (summer, winter, standard, daylight) = if zone == chrono_tz::Europe::Madrid {
        ("020000", "030000", "+0100", "+0200")
    } else {
        ("010000", "020000", "+0000", "+0100")
    };
    let lines = vec![
        "BEGIN:VCALENDAR".into(),
        "VERSION:2.0".into(),
        "PRODID:-//Cita Extranjeria Helper//Appointments//EN".into(),
        "CALSCALE:GREGORIAN".into(),
        "BEGIN:VTIMEZONE".into(),
        format!("TZID:{}", zone.name()),
        "BEGIN:DAYLIGHT".into(),
        format!("DTSTART:19700329T{summer}"),
        format!("TZOFFSETFROM:{standard}"),
        format!("TZOFFSETTO:{daylight}"),
        "RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU".into(),
        "END:DAYLIGHT".into(),
        "BEGIN:STANDARD".into(),
        format!("DTSTART:19701025T{winter}"),
        format!("TZOFFSETFROM:{daylight}"),
        format!("TZOFFSETTO:{standard}"),
        "RRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU".into(),
        "END:STANDARD".into(),
        "END:VTIMEZONE".into(),
        "BEGIN:VEVENT".into(),
        format!("UID:{identifier}@cita-helper.local"),
        format!("DTSTAMP:{}", at.format("%Y%m%dT%H%M%SZ")),
        format!(
            "DTSTART;TZID={}:{}",
            zone.name(),
            local.format("%Y%m%dT%H%M%S")
        ),
        format!("SUMMARY:{TITLE}"),
        format!("LOCATION:{}", ics_escape(&location)),
        "END:VEVENT".into(),
        "END:VCALENDAR".into(),
    ];
    Ok(lines
        .into_iter()
        .map(|line| fold(&line))
        .collect::<Vec<_>>()
        .join("\r\n")
        + "\r\n")
}

/// Private JSON storage retains the existing format; public exports never serialize this object.
pub struct AchievementStore {
    path: std::path::PathBuf,
}
impl AchievementStore {
    pub fn new(path: &std::path::Path) -> Self {
        Self { path: path.into() }
    }
    fn lock(&self) -> Result<std::fs::File, ExportError> {
        let parent = self.path.parent().ok_or(ExportError::InvalidStore)?;
        if parent
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
            || self
                .path
                .symlink_metadata()
                .is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err(ExportError::InvalidStore);
        }
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(parent)
            .map_err(|_| ExportError::Unavailable)?;
        let mut options = std::fs::OpenOptions::new();
        options.create(true).truncate(false).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(self.path.with_extension("lock"))
            .map_err(|_| ExportError::Unavailable)?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            match fs2::FileExt::try_lock_exclusive(&file) {
                Ok(()) => return Ok(file),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() >= deadline {
                        return Err(ExportError::Busy);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(_) => return Err(ExportError::Unavailable),
            }
        }
    }
    fn read(&self) -> Result<Value, ExportError> {
        use std::io::Read;
        let file = match std::fs::File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(json!({})),
            Err(_) => return Err(ExportError::Unavailable),
        };
        let mut bytes = Vec::new();
        file.take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ExportError::Unavailable)?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(ExportError::InvalidStore);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| ExportError::InvalidStore)?;
        if !value
            .as_object()
            .is_some_and(|profiles| profiles.values().all(Value::is_array))
        {
            return Err(ExportError::InvalidStore);
        }
        Ok(value)
    }
    fn write(&self, value: &Value) -> Result<(), ExportError> {
        use std::io::Write;
        let bytes = serde_json::to_vec(value).map_err(|_| ExportError::InvalidStore)?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(ExportError::InvalidStore);
        }
        let mut file = tempfile::Builder::new()
            .prefix(".achievements-")
            .tempfile_in(self.path.parent().ok_or(ExportError::InvalidStore)?)
            .map_err(|_| ExportError::Unavailable)?;
        file.write_all(&bytes)
            .map_err(|_| ExportError::Unavailable)?;
        file.as_file()
            .sync_all()
            .map_err(|_| ExportError::Unavailable)?;
        ceh_core::file_replace::replace(file, |file| file.persist(&self.path).map(|_| ()).map_err(|e| (e.file, e.error)))
            .map_err(|_| ExportError::Unavailable)?;
        Ok(())
    }
    pub fn list(&self, profile: &str) -> Result<Vec<Value>, ExportError> {
        let _lock = self.lock()?;
        let value = self.read()?;
        Ok(value[profile].as_array().cloned().unwrap_or_default())
    }
    pub fn record(
        &self,
        profile: &str,
        statistics: &Value,
        appointment: &Value,
        at: DateTime<Utc>,
    ) -> Result<Value, ExportError> {
        if profile.is_empty() {
            return Err(ExportError::InvalidStore);
        }
        let _lock = self.lock()?;
        let mut raw = self.read()?;
        let mut item = json!({"id":uuid::Uuid::new_v4().to_string(),"recorded_at":at.to_rfc3339_opts(if at.timestamp_subsec_micros()==0{SecondsFormat::Secs}else{SecondsFormat::Micros},false),"statistics":snapshot(statistics),"appointment":appointment_data(appointment)});
        if let Some(fields) = statistics["legacy_unknown_fields"].as_array() {
            let fields: Vec<_> = fields
                .iter()
                .filter_map(Value::as_str)
                .filter(|key| {
                    COUNTS.contains(key)
                        || ["active_seconds", "first_available_seconds", "started_at"].contains(key)
                })
                .collect();
            item["statistics_provenance"] = json!({"unknown_fields":fields});
        }
        if raw.get(profile).is_none() {
            raw[profile] = json!([]);
        }
        raw[profile]
            .as_array_mut()
            .ok_or(ExportError::InvalidStore)?
            .push(item.clone());
        self.write(&raw)?;
        Ok(item)
    }
    pub fn update_appointment(
        &self,
        profile: &str,
        id: &str,
        appointment: &Value,
    ) -> Result<Value, ExportError> {
        let _lock = self.lock()?;
        let mut raw = self.read()?;
        let Some(items) = raw[profile].as_array_mut() else {
            return Err(ExportError::Missing);
        };
        let Some(item) = items.iter_mut().find(|item| item["id"] == id) else {
            return Err(ExportError::Missing);
        };
        if !item.is_object() {
            return Err(ExportError::InvalidStore);
        }
        item["appointment"] = appointment_data(appointment);
        let result = item.clone();
        self.write(&raw)?;
        Ok(result)
    }
}
