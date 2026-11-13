//! Validated profile-owned delays; randomness and sleeping stay outside core.
use crate::python_unicode::{decimal, is_space};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Serialize, Deserialize)]
struct Values {
    action_delay_seconds: [f64; 2],
    bootstrap_seconds: [f64; 2],
    typing_ms: [f64; 2],
    contact_typing_ms: [f64; 2],
    reload_seconds: [f64; 2],
    navigation_seconds: f64,
    clave_error_seconds: f64,
    rate_limit_seconds: f64,
    no_slots_restart_seconds: f64,
    alert_seconds: f64,
    recovery_jitter_seconds: [f64; 2],
    expired_seconds: Vec<f64>,
    server_button_seconds: Vec<f64>,
    server_retry_seconds: Vec<f64>,
    blocked_seconds: Vec<f64>,
    timeout_retry_seconds: Vec<f64>,
    post_timeout_seconds: f64,
    restart_seconds: f64,
    #[serde(default = "default_true")]
    adaptive_reload_on_429: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Serialize)]
#[serde(transparent)]
pub struct AssistTimings(Values);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimingError {
    pub field: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryKind {
    Expired,
    ServerButton,
    ServerRetry,
    Blocked,
    UnsentClick,
    RateLimit,
    ClaveError,
    NoSlotsRestart,
}

struct Spec {
    key: &'static str,
    lower: f64,
    upper: f64,
    sequence: bool,
}
const FIELDS: &[Spec] = &[
    Spec {
        key: "action_delay_seconds",
        lower: 0.0,
        upper: 10.0,
        sequence: false,
    },
    Spec {
        key: "bootstrap_seconds",
        lower: 0.0,
        upper: 120.0,
        sequence: false,
    },
    Spec {
        key: "typing_ms",
        lower: 0.0,
        upper: 1000.0,
        sequence: false,
    },
    Spec {
        key: "contact_typing_ms",
        lower: 0.0,
        upper: 1000.0,
        sequence: false,
    },
    Spec {
        key: "reload_seconds",
        lower: 5.0,
        upper: 3600.0,
        sequence: false,
    },
    Spec {
        key: "navigation_seconds",
        lower: 5.0,
        upper: 300.0,
        sequence: false,
    },
    Spec {
        key: "no_slots_restart_seconds",
        lower: 30.0,
        upper: 3600.0,
        sequence: false,
    },
    Spec {
        key: "clave_error_seconds",
        lower: 30.0,
        upper: 3600.0,
        sequence: false,
    },
    Spec {
        key: "rate_limit_seconds",
        lower: 180.0,
        upper: 3600.0,
        sequence: false,
    },
    Spec {
        key: "alert_seconds",
        lower: 1.0,
        upper: 60.0,
        sequence: false,
    },
    Spec {
        key: "recovery_jitter_seconds",
        lower: 0.0,
        upper: 300.0,
        sequence: false,
    },
    Spec {
        key: "expired_seconds",
        lower: 0.0,
        upper: 3600.0,
        sequence: true,
    },
    Spec {
        key: "server_button_seconds",
        lower: 0.0,
        upper: 3600.0,
        sequence: true,
    },
    Spec {
        key: "server_retry_seconds",
        lower: 1.0,
        upper: 3600.0,
        sequence: true,
    },
    Spec {
        key: "blocked_seconds",
        lower: 30.0,
        upper: 7200.0,
        sequence: true,
    },
    Spec {
        key: "timeout_retry_seconds",
        lower: 0.1,
        upper: 60.0,
        sequence: true,
    },
    Spec {
        key: "post_timeout_seconds",
        lower: 1.0,
        upper: 60.0,
        sequence: false,
    },
    Spec {
        key: "restart_seconds",
        lower: 1.0,
        upper: 60.0,
        sequence: false,
    },
];
fn defaults_json() -> Value {
    serde_json::json!({"action_delay_seconds": [0.3, 0.8], "bootstrap_seconds": [5, 8], "typing_ms": [70, 140], "contact_typing_ms": [35, 70], "reload_seconds": [30, 60], "navigation_seconds": 45, "clave_error_seconds": 180, "rate_limit_seconds": 180, "no_slots_restart_seconds": 180, "alert_seconds": 3, "recovery_jitter_seconds": [0, 0], "expired_seconds": [0, 30, 60, 120, 300], "server_button_seconds": [0, 30, 60, 120, 300], "server_retry_seconds": [30, 60, 120, 300], "blocked_seconds": [30, 60, 120, 300, 600], "timeout_retry_seconds": [2, 5], "post_timeout_seconds": 5, "restart_seconds": 5})
}

fn number(value: &Value) -> Option<f64> {
    if let Some(value) = value.as_f64() {
        return Some(value);
    }
    let raw = value
        .as_str()?
        .trim_matches(|c| is_space(c) && !matches!(c, '\u{1c}'..='\u{1f}'));
    let chars: Vec<_> = raw.chars().collect();
    let mut normalized = String::new();
    for (index, &c) in chars.iter().enumerate() {
        if c == '_' {
            if index == 0
                || index + 1 == chars.len()
                || decimal(chars[index - 1]).is_none()
                || decimal(chars[index + 1]).is_none()
            {
                return None;
            }
        } else if let Some(digit) = decimal(c) {
            normalized.push(char::from(b'0' + digit as u8));
        } else {
            normalized.push(c);
        }
    }
    normalized.parse().ok()
}

pub fn parse_timings(raw: &Value) -> Result<AssistTimings, TimingError> {
    let input = if raw.is_null() {
        None
    } else {
        Some(raw.as_object().ok_or(TimingError { field: None })?)
    };
    let mut values = defaults_json()
        .as_object()
        .expect("fixed timing defaults")
        .clone();
    for spec in FIELDS {
        let error = TimingError {
            field: Some(spec.key),
        };
        let default = values.get(spec.key).expect("fixed timing field").clone();
        let supplied = input
            .and_then(|map| map.get(spec.key))
            .filter(|value| !value.is_null() && value.as_str() != Some(""));
        let mut value = supplied.unwrap_or(&default).clone();
        if spec.key == "blocked_seconds"
            && value.as_array().is_some_and(|values| {
                let old = [300., 360., 420., 540., 600.];
                values.len() == old.len()
                    && values.iter().zip(old).all(|(a, b)| a.as_f64() == Some(b))
            })
        {
            value = default.clone();
        }
        let multiple = default.is_array();
        let numbers: Vec<f64> = if multiple {
            let list = value.as_array().ok_or(error)?;
            if list.is_empty() || list.len() > 10 {
                return Err(error);
            }
            list.iter()
                .map(|v| number(v).ok_or(error))
                .collect::<Result<_, _>>()?
        } else {
            vec![number(&value).ok_or(error)?]
        };
        if numbers
            .iter()
            .any(|v| !v.is_finite() || *v < spec.lower || *v > spec.upper)
        {
            return Err(error);
        }
        if multiple && !spec.sequence && (numbers.len() != 2 || numbers[0] > numbers[1]) {
            return Err(error);
        }
        let result = if multiple {
            serde_json::json!(numbers)
        } else {
            serde_json::json!(numbers[0])
        };
        values.insert(spec.key.to_owned(), result);
    }
    let adaptive = match input.and_then(|map| map.get("adaptive_reload_on_429")) {
        None => true,
        Some(Value::Bool(flag)) => *flag,
        Some(_) => {
            return Err(TimingError {
                field: Some("adaptive_reload_on_429"),
            });
        }
    };
    values.insert("adaptive_reload_on_429".to_owned(), Value::Bool(adaptive));
    let values =
        serde_json::from_value(Value::Object(values)).map_err(|_| TimingError { field: None })?;
    Ok(AssistTimings(values))
}

impl Default for AssistTimings {
    fn default() -> Self {
        parse_timings(&Value::Null).expect("fixed valid defaults")
    }
}
/// Common policy units: typing values are milliseconds, other values seconds.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum TimingKind{Bootstrap,Action,Typing,ContactTyping,Reload}
impl AssistTimings {
    pub fn sample(&self,kind:TimingKind,unit:f64)->Result<f64,TimingError>{
        if !unit.is_finite()||!(0.0..=1.0).contains(&unit){return Err(TimingError{field:None});}
        let [low,high]=match kind{TimingKind::Bootstrap=>self.0.bootstrap_seconds,TimingKind::Action=>self.0.action_delay_seconds,TimingKind::Typing=>self.0.typing_ms,TimingKind::ContactTyping=>self.0.contact_typing_ms,TimingKind::Reload=>self.0.reload_seconds};
        Ok(low+(high-low)*unit)
    }

    pub fn action_delay_seconds(&self) -> [f64; 2] {
        self.0.action_delay_seconds
    }
    pub fn bootstrap_seconds(&self) -> [f64; 2] {
        self.0.bootstrap_seconds
    }
    pub fn contact_typing_ms(&self) -> [f64; 2] {
        self.0.contact_typing_ms
    }
    pub fn recovery_jitter_seconds(&self) -> [f64; 2] {
        self.0.recovery_jitter_seconds
    }
    pub fn reload_seconds(&self) -> [f64; 2] {
        self.0.reload_seconds
    }
    /// Whether each HTTP 429 on the applicant form lengthens its refresh interval.
    pub fn adaptive_reload_on_429(&self) -> bool {
        self.0.adaptive_reload_on_429
    }
    pub fn no_slots_restart_seconds(&self) -> f64 {
        self.0.no_slots_restart_seconds
    }
    pub fn alert_seconds(&self) -> f64 { self.0.alert_seconds }
    pub fn post_timeout_seconds(&self) -> f64 { self.0.post_timeout_seconds }
    pub fn restart_seconds(&self) -> f64 { self.0.restart_seconds }
    pub fn unsent_retry_count(&self) -> usize { self.0.timeout_retry_seconds.len() }
    pub fn navigation_seconds(&self) -> f64 {
        self.0.navigation_seconds
    }
    pub fn recovery_delay(
        &self,
        kind: RetryKind,
        attempt: usize,
        jitter: f64,
    ) -> Result<f64, TimingError> {
        let range = self.0.recovery_jitter_seconds;
        if !jitter.is_finite() || jitter < range[0] || jitter > range[1] {
            return Err(TimingError {
                field: Some("recovery_jitter_seconds"),
            });
        }
        let schedule = match kind {
            RetryKind::Expired => Some(&self.0.expired_seconds),
            RetryKind::ServerButton => Some(&self.0.server_button_seconds),
            RetryKind::ServerRetry => Some(&self.0.server_retry_seconds),
            RetryKind::Blocked => Some(&self.0.blocked_seconds),
            RetryKind::UnsentClick => Some(&self.0.timeout_retry_seconds),
            _ => None,
        };
        let base = if let Some(schedule) = schedule {
            schedule[attempt.min(schedule.len() - 1)]
        } else {
            match kind {
                RetryKind::RateLimit => self.0.rate_limit_seconds,
                RetryKind::ClaveError => self.0.clave_error_seconds,
                RetryKind::NoSlotsRestart => self.0.no_slots_restart_seconds,
                _ => unreachable!("schedule selected above"),
            }
        };
        Ok(base + jitter)
    }
}

/// Round upwards: sub-millisecond precision must not shorten a configured pause.
pub fn deadline_millis(now: u64, seconds: f64) -> Result<u64, TimingError> {
    let error = TimingError { field: None };
    if !seconds.is_finite() || seconds < 0. {
        return Err(error);
    }
    let millis = (seconds * 1000.).ceil();
    if !millis.is_finite() || millis >= u64::MAX as f64 {
        return Err(error);
    }
    now.checked_add(millis as u64).ok_or(error)
}
