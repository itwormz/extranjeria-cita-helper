//! Bounded YAML syntax with the legacy PyYAML scalar schema. No source snippets in errors.
use chrono::{NaiveDate, NaiveTime};
use num_bigint::BigInt;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    str::Chars,
    sync::Arc,
    sync::LazyLock,
};
use yaml_rust2::{
    parser::{Event, Parser, Tag},
    scanner::TScalarStyle,
};
pub const MAX_BYTES: usize = 2 * 1024 * 1024;
const MAX_EVENTS: usize = 100_000;
const MAX_DEPTH: usize = 64;
const MAX_SCALAR: usize = 64 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    NotFound,
    Io,
    Syntax,
    UnsupportedTag,
    ResourceLimit,
    InvalidScalar,
}
impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "legacy profile read failed: {self:?}")
    }
}
impl std::error::Error for ReadError {}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum LegacyKey {
    String(String),
    Number(String),
    Null,
    Timestamp(String),
}
impl fmt::Debug for LegacyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LegacyKey([REDACTED])")
    }
}
#[derive(Clone)]
pub enum RawValue {
    Null,
    Bool(bool),
    Integer(String),
    Float(f64),
    String(String),
    Timestamp(String),
    Sequence(Vec<Arc<RawValue>>),
    Mapping(BTreeMap<LegacyKey, Arc<RawValue>>),
    MergeKey,
}
impl fmt::Debug for RawValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RawValue([REDACTED])")
    }
}
fn float_text(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value == f64::INFINITY {
        return "inf".into();
    }
    if value == f64::NEG_INFINITY {
        return "-inf".into();
    }
    let text = format!("{value:?}");
    if let Some((mantissa, exponent)) = text.split_once('e') {
        let exponent: i32 = exponent.parse().expect("numeric exponent");
        format!(
            "{mantissa}e{}{:02}",
            if exponent < 0 { "-" } else { "+" },
            exponent.unsigned_abs()
        )
    } else {
        text
    }
}
impl RawValue {
    pub fn get(&self, key: &str) -> Option<&RawValue> {
        match self {
            Self::Mapping(map) => map.get(&LegacyKey::String(key.into())).map(Arc::as_ref),
            _ => None,
        }
    }
    pub fn sequence(&self) -> Option<&[Arc<RawValue>]> {
        if let Self::Sequence(values) = self {
            Some(values)
        } else {
            None
        }
    }
    pub fn is_mapping(&self) -> bool {
        matches!(self, Self::Mapping(_))
    }
    pub fn truthy(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(v) => *v,
            Self::Integer(v) => v != "0",
            Self::Float(v) => *v != 0.0,
            Self::String(v) | Self::Timestamp(v) => !v.is_empty(),
            Self::Sequence(v) => !v.is_empty(),
            Self::Mapping(v) => !v.is_empty(),
            Self::MergeKey => true,
        }
    }
    pub fn scalar_text(&self) -> Option<String> {
        match self {
            Self::Null => Some("None".into()),
            Self::Bool(v) => Some(if *v { "True" } else { "False" }.into()),
            Self::Integer(v) | Self::String(v) | Self::Timestamp(v) => Some(v.clone()),
            Self::Float(v) => Some(float_text(*v)),
            Self::MergeKey => Some("<<".into()),
            _ => None,
        }
    }
}
pub struct LegacyDocument {
    root: Arc<RawValue>,
    revision: String,
}
impl fmt::Debug for LegacyDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LegacyDocument([REDACTED])")
    }
}
impl LegacyDocument {
    pub fn root(&self) -> &RawValue {
        &self.root
    }
    pub fn revision(&self) -> &str {
        &self.revision
    }
}
static INTEGER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[+-]?(?:0b[01_]+|0[0-7_]+|0|[1-9][0-9_]*|0x[0-9a-fA-F_]+|[1-9][0-9_]*(?::[0-5]?[0-9])+)$").expect("integer schema")
});
static FLOAT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:[-+]?[0-9][0-9_]*\.[0-9_]*(?:[eE][-+][0-9]+)?|\.[0-9][0-9_]*(?:[eE][-+][0-9]+)?|[-+]?[0-9][0-9_]*(?::[0-5]?[0-9])+\.[0-9_]*|[-+]?\.(?:inf|Inf|INF)|\.(?:nan|NaN|NAN))$").expect("float schema")
});
static DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}$").expect("date schema"));
static TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([0-9]{4})-([0-9]{1,2})-([0-9]{1,2})(?:[Tt]|[ \t]+)([0-9]{1,2}):([0-9]{2}):([0-9]{2})(?:\.([0-9]*))?(?:[ \t]*(Z|[-+][0-9]{1,2}(?::[0-9]{2})?))?$").expect("timestamp schema")
});
fn integer(text: &str) -> Result<String, ReadError> {
    let clean = text.replace('_', "");
    if clean.len() > 4300 {
        return Err(ReadError::ResourceLimit);
    }
    let negative = clean.starts_with('-');
    let body = clean.trim_start_matches(['-', '+']);
    let mut value = if body.contains(':') {
        let mut value = BigInt::from(0);
        for part in body.split(':') {
            value = value * 60
                + BigInt::parse_bytes(part.as_bytes(), 10).ok_or(ReadError::InvalidScalar)?;
        }
        value
    } else {
        let (digits, base) = if let Some(v) = body.strip_prefix("0b") {
            (v, 2)
        } else if let Some(v) = body.strip_prefix("0x") {
            (v, 16)
        } else if let Some(v) = body.strip_prefix("0o") {
            (v, 8)
        } else if body.starts_with('0') && body.len() > 1 {
            (&body[1..], 8)
        } else {
            (body, 10)
        };
        BigInt::parse_bytes(digits.as_bytes(), base).ok_or(ReadError::InvalidScalar)?
    };
    if negative {
        value = -value;
    }
    let result = value.to_string();
    if result.len() > 4300 {
        return Err(ReadError::ResourceLimit);
    }
    Ok(result)
}
fn float(text: &str) -> Result<f64, ReadError> {
    let clean = text.replace('_', "").to_ascii_lowercase();
    match clean.as_str() {
        ".nan" => return Ok(f64::NAN),
        ".inf" | "+.inf" => return Ok(f64::INFINITY),
        "-.inf" => return Ok(f64::NEG_INFINITY),
        _ => {}
    }
    let negative = clean.starts_with('-');
    let body = clean.trim_start_matches(['-', '+']);
    if body.contains(':') {
        let mut value = 0.0;
        for part in body.split(':') {
            value = value * 60.0 + part.parse::<f64>().map_err(|_| ReadError::InvalidScalar)?;
        }
        Ok(if negative { -value } else { value })
    } else {
        clean.parse().map_err(|_| ReadError::InvalidScalar)
    }
}
fn timestamp(text: &str) -> Result<String, ReadError> {
    if DATE.is_match(text) {
        if text.starts_with("0000") {
            return Err(ReadError::InvalidScalar);
        }
        NaiveDate::parse_from_str(text, "%Y-%m-%d").map_err(|_| ReadError::InvalidScalar)?;
        return Ok(text.into());
    }
    let c = TIMESTAMP.captures(text).ok_or(ReadError::InvalidScalar)?;
    let n = |index: usize| {
        c[index]
            .parse::<u32>()
            .map_err(|_| ReadError::InvalidScalar)
    };
    let (year, month, day, hour, minute, second) = (n(1)?, n(2)?, n(3)?, n(4)?, n(5)?, n(6)?);
    if year == 0 {
        return Err(ReadError::InvalidScalar);
    }
    NaiveDate::from_ymd_opt(year as i32, month, day).ok_or(ReadError::InvalidScalar)?;
    NaiveTime::from_hms_opt(hour, minute, second).ok_or(ReadError::InvalidScalar)?;
    let mut result = format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}");
    if let Some(frac) = c.get(7) {
        let mut micro = frac.as_str().chars().take(6).collect::<String>();
        while micro.len() < 6 {
            micro.push('0');
        }
        if micro != "000000" {
            result.push('.');
            result.push_str(&micro);
        }
    }
    if let Some(zone) = c.get(8) {
        let zone = zone.as_str();
        let minutes = if zone == "Z" {
            0i32
        } else {
            let body = &zone[1..];
            let mut parts = body.split(':');
            let hour: i32 = parts
                .next()
                .ok_or(ReadError::InvalidScalar)?
                .parse()
                .map_err(|_| ReadError::InvalidScalar)?;
            let minute: i32 = parts
                .next()
                .unwrap_or("0")
                .parse()
                .map_err(|_| ReadError::InvalidScalar)?;
            let total = hour * 60 + minute;
            if zone.starts_with('-') { -total } else { total }
        };
        if minutes.abs() >= 24 * 60 {
            return Err(ReadError::InvalidScalar);
        }
        result.push_str(&format!(
            "{}{:02}:{:02}",
            if minutes < 0 { "-" } else { "+" },
            minutes.abs() / 60,
            minutes.abs() % 60
        ));
    }
    Ok(result)
}
fn tag_name(tag: &Option<Tag>) -> Result<Option<&str>, ReadError> {
    match tag {
        None => Ok(None),
        Some(tag) if tag.handle == "tag:yaml.org,2002:" => Ok(Some(tag.suffix.as_str())),
        _ => Err(ReadError::UnsupportedTag),
    }
}
fn scalar(text: String, style: TScalarStyle, tag: Option<Tag>) -> Result<RawValue, ReadError> {
    if text.len() > MAX_SCALAR {
        return Err(ReadError::ResourceLimit);
    }
    let kind = tag_name(&tag)?;
    let plain = style == TScalarStyle::Plain;
    if kind == Some("str") || kind.is_none() && !plain {
        return Ok(RawValue::String(text));
    }
    if kind == Some("null")
        || kind.is_none() && matches!(text.as_str(), "" | "~" | "null" | "Null" | "NULL")
    {
        return Ok(RawValue::Null);
    }
    let boolean = match text.as_str() {
        "yes" | "Yes" | "YES" | "true" | "True" | "TRUE" | "on" | "On" | "ON" => Some(true),
        "no" | "No" | "NO" | "false" | "False" | "FALSE" | "off" | "Off" | "OFF" => Some(false),
        _ => None,
    };
    if kind == Some("bool") || kind.is_none() && boolean.is_some() {
        return boolean.map(RawValue::Bool).ok_or(ReadError::InvalidScalar);
    }
    if kind == Some("int") || kind.is_none() && INTEGER.is_match(&text) {
        return integer(&text).map(RawValue::Integer);
    }
    if kind == Some("float") || kind.is_none() && FLOAT.is_match(&text) {
        return float(&text).map(RawValue::Float);
    }
    if kind == Some("timestamp")
        || kind.is_none() && (DATE.is_match(&text) || TIMESTAMP.is_match(&text))
    {
        return timestamp(&text).map(RawValue::Timestamp);
    }
    if kind == Some("merge") || kind.is_none() && text == "<<" {
        return Ok(RawValue::MergeKey);
    }
    if kind.is_some() {
        return Err(ReadError::UnsupportedTag);
    }
    Ok(RawValue::String(text))
}
fn key(value: &RawValue) -> Result<LegacyKey, ReadError> {
    match value {
        RawValue::String(v) => Ok(LegacyKey::String(v.clone())),
        RawValue::Null => Ok(LegacyKey::Null),
        RawValue::Bool(v) => Ok(LegacyKey::Number(if *v { "1" } else { "0" }.into())),
        RawValue::Integer(v) => Ok(LegacyKey::Number(v.clone())),
        RawValue::Float(v) => Ok(LegacyKey::Number(if v.is_finite() && v.fract() == 0.0 {
            format!("{v:.0}")
        } else {
            float_text(*v)
        })),
        RawValue::Timestamp(v) => Ok(LegacyKey::Timestamp(v.clone())),
        _ => Err(ReadError::InvalidScalar),
    }
}
struct Decoder<'a> {
    parser: Parser<Chars<'a>>,
    anchors: HashMap<usize, Arc<RawValue>>,
    events: usize,
    merge_entries: usize,
    merge_key_bytes: usize,
}
impl<'a> Decoder<'a> {
    fn next(&mut self) -> Result<Event, ReadError> {
        self.events += 1;
        if self.events > MAX_EVENTS {
            return Err(ReadError::ResourceLimit);
        }
        self.parser
            .next_token()
            .map(|(event, _)| event)
            .map_err(|_| ReadError::Syntax)
    }
    fn merge(
        &mut self,
        into: &mut BTreeMap<LegacyKey, Arc<RawValue>>,
        value: &RawValue,
    ) -> Result<(), ReadError> {
        match value {
            RawValue::Mapping(map) => {
                self.merge_entries = self
                    .merge_entries
                    .checked_add(map.len())
                    .ok_or(ReadError::ResourceLimit)?;
                if self.merge_entries > MAX_EVENTS {
                    return Err(ReadError::ResourceLimit);
                }
                for key in map.keys() {
                    let bytes = match key {
                        LegacyKey::String(s) | LegacyKey::Number(s) | LegacyKey::Timestamp(s) => {
                            s.len()
                        }
                        LegacyKey::Null => 0,
                    };
                    self.merge_key_bytes = self
                        .merge_key_bytes
                        .checked_add(bytes)
                        .ok_or(ReadError::ResourceLimit)?;
                    if self.merge_key_bytes > MAX_BYTES * 2 {
                        return Err(ReadError::ResourceLimit);
                    }
                }
                into.extend(map.iter().map(|(k, v)| (k.clone(), v.clone())));
            }
            RawValue::Sequence(values) => {
                for value in values.iter().rev() {
                    if !value.is_mapping() {
                        return Err(ReadError::InvalidScalar);
                    }
                    self.merge(into, value)?;
                }
            }
            _ => return Err(ReadError::InvalidScalar),
        }
        Ok(())
    }
    fn node(&mut self, event: Event, depth: usize) -> Result<Arc<RawValue>, ReadError> {
        if depth > MAX_DEPTH {
            return Err(ReadError::ResourceLimit);
        }
        let (anchor, value) = match event {
            Event::Alias(id) => {
                return self
                    .anchors
                    .get(&id)
                    .cloned()
                    .ok_or(ReadError::InvalidScalar);
            }
            Event::Scalar(text, style, anchor, tag) => (anchor, scalar(text, style, tag)?),
            Event::SequenceStart(anchor, tag) => {
                if !matches!(tag_name(&tag)?, None | Some("seq")) {
                    return Err(ReadError::UnsupportedTag);
                }
                let mut values = vec![];
                loop {
                    let next = self.next()?;
                    if next == Event::SequenceEnd {
                        break;
                    }
                    values.push(self.node(next, depth + 1)?);
                }
                (anchor, RawValue::Sequence(values))
            }
            Event::MappingStart(anchor, tag) => {
                if !matches!(tag_name(&tag)?, None | Some("map")) {
                    return Err(ReadError::UnsupportedTag);
                }
                let mut explicit = BTreeMap::new();
                let mut merged = BTreeMap::new();
                loop {
                    let next = self.next()?;
                    if next == Event::MappingEnd {
                        break;
                    }
                    let key_node = self.node(next, depth + 1)?;
                    let next = self.next()?;
                    let value = self.node(next, depth + 1)?;
                    if matches!(key_node.as_ref(), RawValue::MergeKey) {
                        self.merge(&mut merged, &value)?;
                    } else {
                        explicit.insert(key(&key_node)?, value);
                    }
                }
                merged.extend(explicit);
                (anchor, RawValue::Mapping(merged))
            }
            _ => return Err(ReadError::Syntax),
        };
        let value = Arc::new(value);
        if anchor != 0 {
            self.anchors.insert(anchor, value.clone());
        }
        Ok(value)
    }
}
pub fn parse_document(bytes: &[u8]) -> Result<LegacyDocument, ReadError> {
    if bytes.len() > MAX_BYTES {
        return Err(ReadError::ResourceLimit);
    }
    let source = std::str::from_utf8(bytes).map_err(|_| ReadError::Syntax)?;
    let mut decoder = Decoder {
        parser: Parser::new_from_str(source),
        anchors: HashMap::new(),
        events: 0,
        merge_entries: 0,
        merge_key_bytes: 0,
    };
    if decoder.next()? != Event::StreamStart {
        return Err(ReadError::Syntax);
    }
    let event = decoder.next()?;
    let root = if event == Event::StreamEnd {
        Arc::new(RawValue::Null)
    } else {
        if event != Event::DocumentStart {
            return Err(ReadError::Syntax);
        }
        let next = decoder.next()?;
        let root = decoder.node(next, 0)?;
        if decoder.next()? != Event::DocumentEnd || decoder.next()? != Event::StreamEnd {
            return Err(ReadError::Syntax);
        }
        root
    };
    Ok(LegacyDocument {
        root,
        revision: format!("{:x}", Sha256::digest(bytes)),
    })
}
pub fn read_document(path: &std::path::Path) -> Result<LegacyDocument, ReadError> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ReadError::NotFound
        } else {
            ReadError::Io
        }
    })?;
    let mut bytes = vec![];
    file.take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| ReadError::Io)?;
    parse_document(&bytes)
}
