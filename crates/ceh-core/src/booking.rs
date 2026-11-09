//! Bounded public offer parsing. No form values, filters, CAPTCHA or tokens.
use crate::python_unicode::{decimal, is_space};
use chrono::{Datelike, NaiveDate, Weekday};
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use std::sync::LazyLock;

const MAX_OFFERS: usize = 8;
static LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)D[ií]a\s*:\s*(\d{2})/(\d{2})/(\d{4})\s+Hora\s*:\s*(\d{2}):(\d{2})")
        .expect("fixed offer pattern")
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Offer {
    pub date: String,
    pub time: String,
}
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct PublicBookingDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub office: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub offers: Vec<Offer>,
}
fn normalized_space(value: &str) -> String {
    value
        .split(is_space)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
fn digits(value: &str) -> Option<u32> {
    value.chars().try_fold(0u32, |number, c| {
        decimal(c).map(|digit| number * 10 + digit)
    })
}
fn date_string(date: NaiveDate) -> String {
    format!("{:04}-{:02}-{:02}", date.year(), date.month(), date.day())
}

pub fn parse_offers(raw: &Value) -> Vec<Offer> {
    let mut offers = Vec::new();
    let Some(labels) = raw.as_array() else {
        return offers;
    };
    for raw_label in labels.iter().take(32) {
        let Some(label) = raw_label.as_str() else {
            continue;
        };
        let bounded = label.chars().take(240).collect::<String>();
        let label = normalized_space(&bounded);
        // Equivalent to Python's negative lookahead, which Rust regex does not support.
        let candidate = LABEL.captures_iter(&label).find(|captures| {
            let end = captures.get(0).expect("matched label").end();
            !label[end..]
                .chars()
                .next()
                .is_some_and(|c| decimal(c).is_some())
        });
        let Some(captures) = candidate else {
            continue;
        };
        // Python strptime accepts Unicode only in specific digit positions.
        let day_chars: Vec<_> = captures[1].chars().collect();
        let day_alphabet = matches!(day_chars[0], '1' | '2') && decimal(day_chars[1]).is_some()
            || day_chars[0] == '3' && matches!(day_chars[1], '0' | '1')
            || day_chars[0] == '0' && matches!(day_chars[1], '1'..='9');
        if !day_alphabet
            || !captures[2].bytes().all(|c| c.is_ascii_digit())
            || !valid_hour(&format!("{}:{}", &captures[4], &captures[5]))
        {
            continue;
        }
        let parts: Option<Vec<u32>> = (1..=5).map(|i| digits(&captures[i])).collect();
        let Some(parts) = parts else {
            continue;
        };
        let (day, month, year, hour, minute) = (parts[0], parts[1], parts[2], parts[3], parts[4]);
        if year == 0 || hour > 23 || minute > 59 {
            continue;
        }
        let Some(date) = NaiveDate::from_ymd_opt(year as i32, month, day) else {
            continue;
        };
        let offer = Offer {
            date: date_string(date),
            time: format!("{hour:02}:{minute:02}"),
        };
        if !offers.contains(&offer) {
            offers.push(offer);
        }
        if offers.len() >= MAX_OFFERS {
            break;
        }
    }
    offers
}

fn iso_date(value: &str) -> Option<NaiveDate> {
    if !value.is_ascii() {
        return None;
    }
    let b = value.as_bytes();
    if b.len() == 10 && b[4] == b'-' && b[7] == b'-' {
        if !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
        {
            return None;
        }
        let year = value[..4].parse().ok()?;
        if year == 0 {
            return None;
        }
        return NaiveDate::from_ymd_opt(
            year,
            value[5..7].parse().ok()?,
            value[8..10].parse().ok()?,
        );
    }
    if b.len() == 8 && b.iter().all(u8::is_ascii_digit) {
        let year = value[..4].parse().ok()?;
        if year == 0 {
            return None;
        }
        return NaiveDate::from_ymd_opt(year, value[4..6].parse().ok()?, value[6..8].parse().ok()?);
    }
    let (week, weekday) = match b.len() {
        7 if b[4] == b'W' => (&value[5..7], 1),
        8 if b[4] == b'W' => (&value[5..7], u32::from(b[7].checked_sub(b'0')?)),
        8 if b[4] == b'-' && b[5] == b'W' => (&value[6..8], 1),
        10 if b[4] == b'-' && b[5] == b'W' && b[8] == b'-' => {
            (&value[6..8], u32::from(b[9].checked_sub(b'0')?))
        }
        _ => return None,
    };
    if !b[..4].iter().all(u8::is_ascii_digit) || !week.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let year = value[..4].parse().ok()?;
    if year == 0 {
        return None;
    }
    let day = match weekday {
        1 => Weekday::Mon,
        2 => Weekday::Tue,
        3 => Weekday::Wed,
        4 => Weekday::Thu,
        5 => Weekday::Fri,
        6 => Weekday::Sat,
        7 => Weekday::Sun,
        _ => return None,
    };
    NaiveDate::from_isoywd_opt(year, week.parse().ok()?, day)
}
fn valid_hour(value: &str) -> bool {
    let c: Vec<_> = value.chars().collect();
    if c.len() != 5 || c[2] != ':' {
        return false;
    }
    let hour = matches!(c[0], '0' | '1') && decimal(c[1]).is_some()
        || c[0] == '2' && matches!(c[1], '0'..='3');
    hour && matches!(c[3], '0'..='5') && decimal(c[4]).is_some()
}

pub fn normalize_booking_details(raw: &Value) -> PublicBookingDetails {
    let mut result = PublicBookingDetails::default();
    let Some(data) = raw.as_object() else {
        return result;
    };
    if let Some(office) = data.get("office").and_then(Value::as_str) {
        let office = normalized_space(office);
        if !office.is_empty() {
            result.office = Some(office.chars().take(320).collect());
        }
    }
    if let Some(rows) = data.get("offers").and_then(Value::as_array) {
        for row in rows.iter().take(MAX_OFFERS) {
            let Some(date) = row.get("date").and_then(Value::as_str).and_then(iso_date) else {
                continue;
            };
            let Some(hour) = row
                .get("time")
                .and_then(Value::as_str)
                .filter(|s| valid_hour(s))
            else {
                continue;
            };
            let offer = Offer {
                date: date_string(date),
                time: hour.to_owned(),
            };
            if !result.offers.contains(&offer) {
                result.offers.push(offer);
            }
        }
    }
    result
}
pub fn booking_lines(raw: &Value) -> Vec<String> {
    let details = normalize_booking_details(raw);
    let mut lines: Vec<_> = details.office.into_iter().collect();
    for offer in details.offers {
        if let Some(date) = iso_date(&offer.date) {
            lines.push(format!(
                "{:02}.{:02}.{:04} — {}",
                date.day(),
                date.month(),
                date.year(),
                offer.time
            ));
        }
    }
    lines
}
