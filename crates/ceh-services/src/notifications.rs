//! Notification data and transport. Never drives or inspects the browser.
use regex::Regex;
use reqwest::{
    Url,
    header::{HeaderName, HeaderValue},
};
use serde_json::{Value, json};
use std::{
    io::Read,
    sync::LazyLock,
    time::{Duration, Instant, SystemTime},
};
use zeroize::Zeroize;
const EVENTS: &[&str] = &["auth", "office", "booking", "error"];
const VARIABLES: &[&str] = &[
    "message",
    "event",
    "profile",
    "province",
    "timestamp",
    "event_id",
];
const RESPONSE_LIMIT: u64 = 256 * 1024;
static VARIABLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{\s*(\w+)\s*\}\}").unwrap());
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationError {
    InvalidConfig,
    InvalidResponse,
    Stale,
    Network,
    TooLarge,
    Http(u16),
    ExistingWebhook,
    SecretStorage,
    Expired,
}
impl std::fmt::Display for NotificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "notification: {self:?}")
    }
}
impl std::error::Error for NotificationError {}
fn yes(v: &Value, key: &str) -> bool {
    v[key].as_bool().unwrap_or(false)
}
pub fn defaults() -> Value {
    json!({"telegram":{"enabled":false,"events":EVENTS,"recipients":[],"secret_ref":""},"webhook":{"enabled":false,"events":EVENTS,"secret_ref":""}})
}
pub fn settings(raw: &Value) -> Value {
    let mut value = defaults();
    for channel in ["telegram", "webhook"] {
        if let Some(source) = raw[channel].as_object() {
            value[channel]
                .as_object_mut()
                .unwrap()
                .extend(source.clone());
        }
    }
    value
}
pub fn state_event(state: &Value) -> Option<&'static str> {
    if yes(state, "failed") {
        return Some("error");
    }
    if yes(state, "finished")
        || state["mode"] == "finished"
        || yes(state, "connection_lost")
        || yes(state, "paused")
    {
        return None;
    }
    if yes(state, "booking_handoff") {
        return Some("booking");
    }
    if state["mode"] != "manual" {
        return None;
    }
    match state["screen"].as_str() {
        Some("Выбор офиса") => Some("office"),
        Some("Выбор ситы" | "Капча") => Some("booking"),
        Some("Авторизация сертификатом" | "Cl@ve Móvil" | "Cl@ve Permanente" | "Авторизация") => {
            Some("auth")
        }
        _ => None,
    }
}
#[derive(Default)]
pub struct EventRouter {
    last: Option<(Value, &'static str)>,
}
impl EventRouter {
    pub fn observe(&mut self, state: &Value) -> Option<&'static str> {
        if !yes(state, "failed") && (yes(state, "connection_lost") || yes(state, "paused")) {
            return None;
        }
        let event = state_event(state);
        let next = event.map(|kind| (state["run_id"].clone(), kind));
        let changed = next != self.last;
        self.last = next;
        if changed { event } else { None }
    }
}
#[derive(Clone)]
pub struct Cancellation(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl Cancellation {
    fn new() -> Self {
        Self(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
            true,
        )))
    }
    pub fn current(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
    fn cancel(&self) {
        self.0.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}
pub struct DeliveryIntent {
    channel: &'static str,
    config: Value,
    event: Value,
    message: String,
    cancellation: Cancellation,
}
impl DeliveryIntent {
    pub fn channel(&self) -> &'static str {
        self.channel
    }
    pub fn config(&self) -> &Value {
        &self.config
    }
    pub fn event(&self) -> &Value {
        &self.event
    }
    pub fn message(&self) -> &str {
        &self.message
    }
    pub fn cancellation(&self) -> Cancellation {
        self.cancellation.clone()
    }
}
pub struct NotificationPlanner {
    router: EventRouter,
    telegram_router: EventRouter,
    profile: Option<String>,
    kind: Option<&'static str>,
    cancellation: Cancellation,
    closed: bool,
}
impl Default for NotificationPlanner {
    fn default() -> Self {
        Self {
            router: EventRouter::default(),
            telegram_router: EventRouter::default(),
            profile: None,
            kind: None,
            cancellation: Cancellation::new(),
            closed: false,
        }
    }
}
impl NotificationPlanner {
    fn invalidate(&mut self) {
        self.cancellation.cancel();
        self.cancellation = Cancellation::new();
    }
    pub fn reset(&mut self) {
        self.invalidate();
        self.router = EventRouter::default();
        self.telegram_router = EventRouter::default();
        self.kind = None;
    }
    pub fn stop(&mut self) {
        self.reset();
        self.closed = true;
        self.cancellation.cancel();
    }
    pub fn observe(
        &mut self,
        profile: &str,
        label: &str,
        province: &str,
        state: &Value,
        language: &str,
        raw_config: &Value,
    ) -> Result<Vec<DeliveryIntent>, NotificationError> {
        if self.closed || profile.is_empty() {
            return Ok(Vec::new());
        }
        if self.profile.as_deref() != Some(profile) {
            self.reset();
            self.profile = Some(profile.into());
        }
        let event = self.router.observe(state);
        let kind = state_event(state);
        if kind != self.kind {
            self.invalidate();
            self.kind = kind;
        }
        let telegram_event = if kind == Some("booking") && yes(state, "booking_details_pending") {
            None
        } else {
            self.telegram_router.observe(state)
        };
        let Some(kind) = event.or(telegram_event) else {
            return Ok(Vec::new());
        };
        let payload = make_event(kind, label, province, language)?;
        let config = settings(raw_config);
        let mut result = Vec::new();
        for (channel, event) in [("telegram", telegram_event), ("webhook", event)] {
            let Some(event) = event else {
                continue;
            };
            let config = &config[channel];
            if !yes(config, "enabled")
                || !config["events"]
                    .as_array()
                    .is_some_and(|events| events.contains(&json!(event)))
            {
                continue;
            }
            let message = if channel == "telegram" {
                telegram_message(&payload, language, &state["booking_details"])
            } else {
                text(&payload, "message", "").into()
            };
            result.push(DeliveryIntent {
                channel,
                config: config.clone(),
                event: payload.clone(),
                message,
                cancellation: self.cancellation.clone(),
            });
        }
        Ok(result)
    }
}
impl Drop for NotificationPlanner {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}
pub struct NotificationRequest {
    url: String,
    method: &'static str,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
}
impl NotificationRequest {
    pub fn url(&self) -> &str {
        &self.url
    }
    pub fn body(&self) -> Option<&[u8]> {
        self.body.as_deref()
    }
    pub fn method(&self) -> &str {
        self.method
    }
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }
}
impl Drop for NotificationRequest {
    fn drop(&mut self) {
        self.url.zeroize();
        for (_, value) in &mut self.headers {
            value.zeroize();
        }
        if let Some(body) = &mut self.body {
            body.zeroize();
        }
    }
}
impl std::fmt::Debug for NotificationRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotificationRequest([REDACTED])")
    }
}
fn text<'a>(v: &'a Value, key: &str, default: &'a str) -> &'a str {
    v[key].as_str().unwrap_or(default)
}
pub fn expand(value: &str, event: &Value) -> Result<String, NotificationError> {
    let mut invalid = false;
    let result = VARIABLE
        .replace_all(value, |capture: &regex::Captures<'_>| {
            let key = &capture[1];
            if !VARIABLES.contains(&key) || event.get(key).is_none() {
                invalid = true;
                return String::new();
            }
            match &event[key] {
                Value::String(value) => value.clone(),
                value => value.to_string(),
            }
        })
        .into_owned();
    if invalid {
        Err(NotificationError::InvalidConfig)
    } else {
        Ok(result)
    }
}
fn walk(value: &mut Value, event: &Value) -> Result<(), NotificationError> {
    match value {
        Value::String(value) => *value = expand(value, event)?,
        Value::Array(values) => {
            for value in values {
                walk(value, event)?;
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                walk(value, event)?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub fn webhook_request(
    config: &Value,
    event: &Value,
) -> Result<NotificationRequest, NotificationError> {
    let raw = config["url"]
        .as_str()
        .ok_or(NotificationError::InvalidConfig)?;
    if raw.chars().any(char::is_whitespace) {
        return Err(NotificationError::InvalidConfig);
    }
    let mut url = Url::parse(raw).map_err(|_| NotificationError::InvalidConfig)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(NotificationError::InvalidConfig);
    }
    let method = match text(config, "method", "POST") {
        "POST" => "POST",
        "GET" => "GET",
        _ => return Err(NotificationError::InvalidConfig),
    };
    let mut headers = Vec::new();
    let mut names = std::collections::BTreeSet::new();
    for row in config["headers"].as_array().into_iter().flatten() {
        let name = text(row, "name", "");
        let value = text(row, "value", "");
        let lower = name.to_ascii_lowercase();
        if HeaderName::from_bytes(name.as_bytes()).is_err()
            || ["host", "content-length", "transfer-encoding", "connection"]
                .contains(&lower.as_str())
            || !names.insert(lower)
            || value
                .chars()
                .any(|c| ['\r', '\n', '\0'].contains(&c) || u32::from(c) > 255)
        {
            return Err(NotificationError::InvalidConfig);
        }
        headers.push((name.to_owned(), value.to_owned()));
    }
    let body_mode = text(config, "body_mode", "standard");
    if !["standard", "custom"].contains(&body_mode) {
        return Err(NotificationError::InvalidConfig);
    }
    if let Some(query) = config["query"].as_array() {
        for row in query {
            let name = text(row, "name", "");
            if name.trim().is_empty() {
                return Err(NotificationError::InvalidConfig);
            }
            let value = expand(text(row, "value", ""), event)?;
            url.query_pairs_mut().append_pair(name, &value);
        }
    }
    let body = if method == "POST" {
        let value = if body_mode == "custom" {
            let mut value = serde_json::from_str(text(config, "body", ""))
                .map_err(|_| NotificationError::InvalidConfig)?;
            walk(&mut value, event)?;
            value
        } else {
            event.clone()
        };
        if !headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        {
            headers.push(("Content-Type".into(), "application/json".into()));
        }
        Some(serde_json::to_vec(&value).map_err(|_| NotificationError::InvalidConfig)?)
    } else {
        None
    };
    Ok(NotificationRequest {
        url: url.to_string(),
        method,
        headers,
        body,
    })
}
pub fn recipient_from_chat(chat: &Value) -> Option<Value> {
    let id = match &chat["id"] {
        Value::String(id) => id.clone(),
        Value::Number(id) => id.to_string(),
        _ => return None,
    };
    let label = chat["title"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            let words = [chat["first_name"].as_str(), chat["last_name"].as_str()]
                .into_iter()
                .flatten()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            if words.is_empty() { id.clone() } else { words }
        });
    Some(
        json!({"id":id,"label":label,"type":text(chat,"type","private"),"username":text(chat,"username","")}),
    )
}
pub struct Pairing {
    username: String,
    code: String,
    created: u64,
    offset: Option<i64>,
}
impl Pairing {
    pub fn new(username: &str, now: u64) -> Result<Self, NotificationError> {
        Self::with_code(
            username,
            &format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            ),
            now,
        )
    }
    pub fn with_code(username: &str, code: &str, now: u64) -> Result<Self, NotificationError> {
        if username.len() < 5
            || !username
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_')
            || code.is_empty()
            || code.len() > 64
            || !code
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
        {
            return Err(NotificationError::InvalidConfig);
        }
        Ok(Self {
            username: username.into(),
            code: code.into(),
            created: now,
            offset: None,
        })
    }
    pub fn link(&self, group: bool) -> String {
        format!(
            "https://t.me/{}?{}={}",
            self.username,
            if group { "startgroup" } else { "start" },
            self.code
        )
    }
    pub fn offset(&self) -> Option<i64> {
        self.offset
    }
    pub fn expired(&self, now: u64) -> bool {
        now.saturating_sub(self.created) > 120_000
    }
    pub fn match_updates(&mut self, updates: &Value, group: bool, now: u64) -> Option<Value> {
        if self.expired(now) {
            return None;
        }
        for update in updates.as_array()? {
            let next = update["update_id"].as_i64().unwrap_or(0).checked_add(1)?;
            self.offset = Some(self.offset.unwrap_or(0).max(next));
            let message = &update["message"];
            let message_text = text(message, "text", "");
            let chat = &message["chat"];
            let kind = text(chat, "type", "");
            if [
                format!("/start {}", self.code),
                format!("/start@{} {}", self.username, self.code),
            ]
            .contains(&message_text.to_owned())
                && if group {
                    ["group", "supergroup"].contains(&kind)
                } else {
                    kind == "private"
                }
            {
                return recipient_from_chat(chat);
            }
        }
        None
    }
}

static TRANSLATIONS: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../app/desktop/assets/translations.json"
    ))
    .expect("shipped translation catalog")
});
fn tr(text: &str, language: &str) -> String {
    TRANSLATIONS[language][text]
        .as_str()
        .unwrap_or(text)
        .to_owned()
}
pub fn make_event(
    kind: &str,
    profile: &str,
    province: &str,
    language: &str,
) -> Result<Value, NotificationError> {
    let now: chrono::DateTime<chrono::Utc> = SystemTime::now().into();
    make_event_with_metadata(
        kind,
        profile,
        province,
        language,
        &uuid::Uuid::new_v4().to_string(),
        &now.to_rfc3339(),
    )
}
pub fn make_event_with_metadata(
    kind: &str,
    profile: &str,
    province: &str,
    language: &str,
    event_id: &str,
    timestamp: &str,
) -> Result<Value, NotificationError> {
    let (title, action) = match kind {
        "auth" => (
            "Проверить авторизацию в Camoufox",
            "Проверить браузер и завершить вход при необходимости.",
        ),
        "office" => ("Требуется выбрать офис", "Выбрать офис в Camoufox."),
        "booking" => (
            "Открыт выбор ситы",
            "Выбрать дату и время, заполнить CAPTCHA и подтвердить запись в Camoufox.",
        ),
        "error" => (
            "Поиск завершился с ошибкой",
            "Открыть приложение и проверить диагностику.",
        ),
        "test" => ("Тестовое уведомление", "Подключение уведомлений проверено."),
        _ => return Err(NotificationError::InvalidConfig),
    };
    Ok(
        json!({"schema_version":1,"event_id":event_id,"event":kind,"timestamp":timestamp,"profile":profile,"province":province,"action_required":kind!="test","message":format!("{}\n\n{}: {profile}\n{}: {province}\n\n{}\n\nCita Extranjeria Helper",tr(title,language),tr("Профиль",language),tr("Провинция",language),tr(action,language))}),
    )
}
pub fn telegram_message(event: &Value, language: &str, details: &Value) -> String {
    if event["event"] != "booking" || !details.is_object() {
        return text(event, "message", "").into();
    }
    let mut rows = Vec::new();
    if let Some(office) =
        ceh_core::booking::normalize_booking_details(&json!({"office":details["office"]})).office
    {
        rows.push(format!("📍 {office}"));
    }
    for offer in details["offers"].as_array().into_iter().flatten().take(8) {
        let normalized = ceh_core::booking::normalize_booking_details(&json!({"offers":[offer]}));
        if let Some(offer) = normalized.offers.first()
            && let Ok(day) = chrono::NaiveDate::parse_from_str(&offer.date, "%Y-%m-%d")
        {
            rows.push(format!("• {} — {}", day.format("%d.%m.%Y"), offer.time));
        }
    }
    if rows.is_empty() {
        return text(event, "message", "").into();
    }
    format!(
        "🗓 {}\n\n{}: {}\n{}: {}\n\n{}\n\n{}\n\nCita Extranjeria Helper",
        tr("Открыт выбор ситы", language),
        tr("Профиль", language),
        text(event, "profile", ""),
        tr("Провинция", language),
        text(event, "province", ""),
        rows.join("\n"),
        tr(
            "Выбрать дату и время, заполнить CAPTCHA и подтвердить запись в Camoufox.",
            language
        )
    )
}
pub struct Telegram {
    token: zeroize::Zeroizing<String>,
    api_base: String,
}
impl Telegram {
    pub fn new(token: &str) -> Result<Self, NotificationError> {
        static TOKEN: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^[0-9]{5,}:[A-Za-z0-9_-]+$").unwrap());
        if !TOKEN.is_match(token) {
            return Err(NotificationError::InvalidConfig);
        }
        Ok(Self {
            token: zeroize::Zeroizing::new(token.into()),
            api_base: "https://api.telegram.org".into(),
        })
    }
    fn request(&self, method: &str, data: Value) -> Result<NotificationRequest, NotificationError> {
        if ![
            "getMe",
            "getWebhookInfo",
            "getUpdates",
            "getChat",
            "sendMessage",
        ]
        .contains(&method)
        {
            return Err(NotificationError::InvalidConfig);
        }
        Ok(NotificationRequest {
            url: format!("{}/bot{}/{method}", self.api_base, *self.token),
            method: "POST",
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: Some(serde_json::to_vec(&data).map_err(|_| NotificationError::InvalidConfig)?),
        })
    }
    fn call(
        &self,
        delivery: &HttpDelivery,
        method: &str,
        data: Value,
        current: impl Fn() -> bool,
    ) -> Result<Value, NotificationError> {
        let receipt = delivery.send(&self.request(method, data)?, 1, current)?;
        let response: Value = serde_json::from_slice(receipt.body())
            .map_err(|_| NotificationError::InvalidResponse)?;
        if response["ok"] != true {
            return Err(NotificationError::InvalidResponse);
        }
        response
            .get("result")
            .cloned()
            .ok_or(NotificationError::InvalidResponse)
    }
    pub fn check(&self, delivery: &HttpDelivery) -> Result<Value, NotificationError> {
        let response = self.call(delivery, "getMe", json!({}), || true)?;
        static USER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_]{5,}$").unwrap());
        if !USER.is_match(text(&response, "username", "")) {
            return Err(NotificationError::InvalidResponse);
        }
        Ok(response)
    }
    pub fn begin_pairing(
        &self,
        delivery: &HttpDelivery,
        now: u64,
    ) -> Result<Pairing, NotificationError> {
        let info = self.call(delivery, "getWebhookInfo", json!({}), || true)?;
        if !text(&info, "url", "").is_empty() {
            return Err(NotificationError::ExistingWebhook);
        }
        Pairing::new(text(&self.check(delivery)?, "username", ""), now)
    }
    pub fn poll(
        &self,
        delivery: &HttpDelivery,
        pair: &mut Pairing,
        group: bool,
        now: impl Fn() -> u64,
    ) -> Result<Option<Value>, NotificationError> {
        if pair.expired(now()) {
            return Err(NotificationError::Expired);
        }
        let mut data = json!({"timeout":0,"limit":100});
        if let Some(offset) = pair.offset() {
            data["offset"] = json!(offset);
        }
        let updates = self.call(delivery, "getUpdates", data, || true)?;
        Ok(pair.match_updates(&updates, group, now()))
    }
    pub fn chat(&self, delivery: &HttpDelivery, chat_id: &str) -> Result<Value, NotificationError> {
        static ID: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^(?:-?[0-9]+|@[A-Za-z0-9_]{5,})$").unwrap());
        if !ID.is_match(chat_id) {
            return Err(NotificationError::InvalidConfig);
        }
        recipient_from_chat(&self.call(delivery, "getChat", json!({"chat_id":chat_id}), || true)?)
            .ok_or(NotificationError::InvalidResponse)
    }
    pub fn send(
        &self,
        delivery: &HttpDelivery,
        chat_id: &str,
        message: &str,
        current: impl Fn() -> bool,
    ) -> Result<u128, NotificationError> {
        let request = self.request(
            "sendMessage",
            json!({"chat_id":chat_id,"text":message,"link_preview_options":{"is_disabled":true}}),
        )?;
        let receipt = delivery.send(&request, 3, current)?;
        let response: Value = serde_json::from_slice(receipt.body())
            .map_err(|_| NotificationError::InvalidResponse)?;
        if response["ok"] != true {
            return Err(NotificationError::InvalidResponse);
        }
        Ok(receipt.elapsed_ms)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryOutcome {
    TelegramAccepted,
    Partial,
    Failed,
    Webhook { status: u16, elapsed_ms: u128 },
}
/// Call from a notification worker, never from the flow/browser dispatch thread.
pub fn deliver_intent<B: crate::secrets::Backend>(
    intent: &DeliveryIntent,
    vault: &crate::secrets::Vault<B>,
    http: &HttpDelivery,
) -> Result<DeliveryOutcome, NotificationError> {
    if !intent.cancellation.current() {
        return Err(NotificationError::Stale);
    }
    let secret = vault
        .get(text(&intent.config, "secret_ref", ""))
        .map_err(|_| NotificationError::SecretStorage)?;
    if !intent.cancellation.current() {
        return Err(NotificationError::Stale);
    }
    if intent.channel == "telegram" {
        let client = Telegram::new(text(secret.value(), "token", ""))?;
        let mut accepted = 0;
        let mut failures = 0;
        for recipient in intent.config["recipients"].as_array().into_iter().flatten() {
            if !intent.cancellation.current() {
                return Err(NotificationError::Stale);
            }
            let id = text(recipient, "id", "");
            if client
                .send(http, id, &intent.message, || intent.cancellation.current())
                .is_ok()
            {
                accepted += 1;
            } else {
                failures += 1;
            }
        }
        if !intent.cancellation.current() {
            return Err(NotificationError::Stale);
        }
        Ok(if failures == 0 {
            DeliveryOutcome::TelegramAccepted
        } else if accepted > 0 {
            DeliveryOutcome::Partial
        } else {
            DeliveryOutcome::Failed
        })
    } else {
        let request = webhook_request(secret.value(), &intent.event)?;
        let response = http.send(&request, 3, || intent.cancellation.current())?;
        Ok(DeliveryOutcome::Webhook {
            status: response.status,
            elapsed_ms: response.elapsed_ms,
        })
    }
}

pub struct DeliveryReceipt {
    pub status: u16,
    body: Vec<u8>,
    pub elapsed_ms: u128,
}
impl DeliveryReceipt {
    pub fn body(&self) -> &[u8] {
        &self.body
    }
}
impl Drop for DeliveryReceipt {
    fn drop(&mut self) {
        self.body.zeroize();
    }
}
pub struct HttpDelivery {
    client: reqwest::blocking::Client,
}
pub fn retry_seconds(value: Option<&str>, attempt: u32, now: SystemTime) -> f64 {
    let value = value
        .and_then(|value| {
            value.parse::<f64>().ok().or_else(|| {
                httpdate::parse_http_date(value)
                    .ok()
                    .map(|date| match date.duration_since(now) {
                        Ok(duration) => duration.as_secs_f64(),
                        Err(error) => -error.duration().as_secs_f64(),
                    })
            })
        })
        .unwrap_or(2_f64.powi(attempt.min(30) as i32));
    if value.is_nan() { 1.0 } else { value.max(1.0) }
}
impl HttpDelivery {
    pub fn new() -> Result<Self, NotificationError> {
        Self::configured(true)
    }
    fn configured(https_only: bool) -> Result<Self, NotificationError> {
        let client = reqwest::blocking::Client::builder()
            .https_only(https_only)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| NotificationError::Network)?;
        Ok(Self { client })
    }
    pub fn send(
        &self,
        request: &NotificationRequest,
        attempts: u32,
        current: impl Fn() -> bool,
    ) -> Result<DeliveryReceipt, NotificationError> {
        if !(1..=3).contains(&attempts) {
            return Err(NotificationError::InvalidConfig);
        }
        let started = Instant::now();
        for attempt in 0..attempts {
            if !current() {
                return Err(NotificationError::Stale);
            }
            let mut builder = self.client.request(
                if request.method == "GET" {
                    reqwest::Method::GET
                } else {
                    reqwest::Method::POST
                },
                &request.url,
            );
            for (name, value) in &request.headers {
                let mut header = HeaderValue::from_bytes(
                    &value
                        .chars()
                        .map(|c| {
                            u8::try_from(c as u32).map_err(|_| NotificationError::InvalidConfig)
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                )
                .map_err(|_| NotificationError::InvalidConfig)?;
                header.set_sensitive(true);
                builder = builder.header(name, header);
            }
            if let Some(body) = &request.body {
                builder = builder.body(body.clone());
            }
            let mut delay = 2_f64.powi(attempt as i32);
            match builder.send() {
                Ok(response) => {
                    let status = response.status().as_u16();
                    if (200..300).contains(&status) {
                        if response
                            .content_length()
                            .is_some_and(|size| size > RESPONSE_LIMIT)
                        {
                            return Err(NotificationError::TooLarge);
                        }
                        let mut body = Vec::new();
                        if response
                            .take(RESPONSE_LIMIT + 1)
                            .read_to_end(&mut body)
                            .is_err()
                        {
                            body.zeroize();
                            if attempt + 1 == attempts {
                                return Err(NotificationError::Network);
                            }
                        } else {
                            if body.len() as u64 > RESPONSE_LIMIT {
                                body.zeroize();
                                return Err(NotificationError::TooLarge);
                            }
                            if !current() {
                                body.zeroize();
                                return Err(NotificationError::Stale);
                            }
                            return Ok(DeliveryReceipt {
                                status,
                                body,
                                elapsed_ms: started.elapsed().as_millis(),
                            });
                        }
                    } else {
                        delay = retry_seconds(
                            response
                                .headers()
                                .get("retry-after")
                                .and_then(|v| v.to_str().ok()),
                            attempt,
                            SystemTime::now(),
                        );
                        if delay > 60.0
                            || !(status == 429 || status >= 500)
                            || attempt + 1 == attempts
                        {
                            return Err(NotificationError::Http(status));
                        }
                    }
                }
                Err(_) => {
                    if attempt + 1 == attempts {
                        return Err(NotificationError::Network);
                    }
                }
            }
            let end = Instant::now() + Duration::from_secs_f64(delay);
            while Instant::now() < end {
                if !current() {
                    return Err(NotificationError::Stale);
                }
                std::thread::sleep(
                    Duration::from_millis(200).min(end.saturating_duration_since(Instant::now())),
                );
            }
        }
        Err(NotificationError::Network)
    }
}

#[cfg(test)]
mod http_tests {
    use super::*;
    use std::{
        io::Write,
        net::TcpListener,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };
    fn server(response: Vec<u8>) -> (String, std::thread::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut bytes = vec![0; 8192];
            let n = socket.read(&mut bytes).unwrap();
            bytes.truncate(n);
            socket.write_all(&response).unwrap();
            bytes
        });
        (url, handle)
    }
    fn request(url: String) -> NotificationRequest {
        NotificationRequest {
            url,
            method: "POST",
            headers: vec![("X-Private".into(), "FICTITIOUS".into())],
            body: Some(b"{}".to_vec()),
        }
    }
    #[test]
    fn local_delivery_is_bounded_and_sends_headers() {
        let (url, server) =
            server(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_vec());
        let delivery = HttpDelivery::configured(false).unwrap();
        let receipt = delivery.send(&request(url), 1, || true).unwrap();
        assert_eq!(receipt.body(), b"{}");
        let received = String::from_utf8(server.join().unwrap()).unwrap();
        assert!(received.to_lowercase().contains("x-private: fictitious"));
    }
    #[test]
    fn redirect_is_returned_without_forwarding_credentials() {
        let (url,server)=server(b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec());
        let error = HttpDelivery::configured(false)
            .unwrap()
            .send(&request(url), 3, || true)
            .err();
        assert_eq!(error, Some(NotificationError::Http(302)));
        server.join().unwrap();
    }
    #[test]
    fn oversized_response_fails_and_stale_does_not_connect() {
        let (url, server) = server(
            b"HTTP/1.1 200 OK\r\nContent-Length: 262145\r\nConnection: close\r\n\r\n".to_vec(),
        );
        let delivery = HttpDelivery::configured(false).unwrap();
        assert_eq!(
            delivery.send(&request(url), 1, || true).err(),
            Some(NotificationError::TooLarge)
        );
        server.join().unwrap();
        assert_eq!(
            delivery
                .send(&request("http://127.0.0.1:1".into()), 3, || false)
                .err(),
            Some(NotificationError::Stale)
        );
    }
    #[test]
    fn existing_telegram_webhook_prevents_pairing_without_changes() {
        let body = br#"{"ok":true,"result":{"url":"https://existing.example.test"}}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            std::str::from_utf8(body).unwrap()
        )
        .into_bytes();
        let (url, server) = server(response);
        let mut telegram = Telegram::new("123456:FICTITIOUS").unwrap();
        telegram.api_base = url;
        assert_eq!(
            telegram
                .begin_pairing(&HttpDelivery::configured(false).unwrap(), 0)
                .err(),
            Some(NotificationError::ExistingWebhook)
        );
        let request = String::from_utf8(server.join().unwrap()).unwrap();
        assert!(request.contains("/getWebhookInfo"));
        assert!(!request.contains("deleteWebhook"));
    }
    #[test]
    fn retryable_http_response_retries_only_within_attempt_limit() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let mut paths = Vec::new();
            for status in [503, 200] {
                let (mut socket, _) = listener.accept().unwrap();
                let mut bytes = [0; 4096];
                let size = socket.read(&mut bytes).unwrap();
                paths.push(String::from_utf8_lossy(&bytes[..size]).to_string());
                write!(socket,"HTTP/1.1 {status} Status\r\nRetry-After: 0\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}").unwrap();
            }
            paths
        });
        let receipt = HttpDelivery::configured(false)
            .unwrap()
            .send(&request(url), 3, || true)
            .unwrap();
        assert_eq!(receipt.status, 200);
        assert_eq!(server.join().unwrap().len(), 2);
    }
    #[test]
    fn truncated_success_body_is_retried_as_network_failure() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let end = Instant::now() + Duration::from_secs(3);
            let mut count = 0;
            while count < 2 && Instant::now() < end {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        socket
                            .set_read_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        let mut data = [0; 4096];
                        let _ = socket.read(&mut data);
                        let response = if count == 0 {
                            "HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nx"
                        } else {
                            "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}"
                        };
                        socket.write_all(response.as_bytes()).unwrap();
                        count += 1;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10))
                    }
                    Err(error) => panic!("local listener: {error}"),
                }
            }
            count
        });
        let result = HttpDelivery::configured(false)
            .unwrap()
            .send(&request(url), 2, || true);
        let count = server.join().unwrap();
        assert!(result.is_ok());
        assert_eq!(count, 2);
    }
    #[test]
    fn cancellation_interrupts_retry_wait() {
        let current = Arc::new(AtomicBool::new(true));
        let (url,server)=server(b"HTTP/1.1 503 Unavailable\r\nRetry-After: 30\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec());
        let flag = current.clone();
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            flag.store(false, Ordering::SeqCst);
        });
        let started = Instant::now();
        assert_eq!(
            HttpDelivery::configured(false)
                .unwrap()
                .send(&request(url), 3, || current.load(Ordering::SeqCst))
                .err(),
            Some(NotificationError::Stale)
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        server.join().unwrap();
        stopper.join().unwrap();
    }
}
