//! Atomic, revision-checked profile edits. Existing unknown YAML values are retained.
use crate::{
    legacy_yaml::{LegacyKey, MAX_BYTES, RawValue},
    profiles::read_profiles,
};
use ceh_core::profiles::{ProfileCatalog, ProfileDraft, validate_desktop_profile};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::Write,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
#[derive(Debug, PartialEq, Eq)]
pub enum ProfileWriteError {
    Read,
    Conflict,
    Validation(Vec<String>),
    Unavailable,
    Limit,
}
#[derive(Serialize)]
pub struct ProfileSaved {
    pub id: String,
    pub revision: String,
}
fn map(value: &mut RawValue) -> Result<&mut BTreeMap<LegacyKey, Arc<RawValue>>, ProfileWriteError> {
    if matches!(value, RawValue::Null) {
        *value = RawValue::Mapping(BTreeMap::new());
    }
    if let RawValue::Mapping(map) = value {
        Ok(map)
    } else {
        Err(ProfileWriteError::Read)
    }
}
fn set(
    value: &mut RawValue,
    path: &[&str],
    replacement: RawValue,
) -> Result<(), ProfileWriteError> {
    let (first, rest) = path.split_first().ok_or(ProfileWriteError::Read)?;
    let map = map(value)?;
    let key = LegacyKey::String((*first).into());
    if rest.is_empty() {
        map.insert(key, Arc::new(replacement));
    } else {
        let value = map.entry(key).or_insert_with(|| Arc::new(RawValue::Null));
        set(Arc::make_mut(value), rest, replacement)?;
    }
    Ok(())
}
fn text(value: &mut RawValue, path: &[&str], replacement: &str) -> Result<(), ProfileWriteError> {
    set(value, path, RawValue::String(replacement.into()))
}
fn append(out: &mut String, text: &str) -> Result<(), ProfileWriteError> {
    if out.len().saturating_add(text.len()) > MAX_BYTES {
        return Err(ProfileWriteError::Limit);
    }
    out.push_str(text);
    Ok(())
}
fn quoted(out: &mut String, value: &str) -> Result<(), ProfileWriteError> {
    append(
        out,
        &serde_json::to_string(value).map_err(|_| ProfileWriteError::Read)?,
    )
}
fn emit(value: &RawValue, out: &mut String, depth: usize) -> Result<(), ProfileWriteError> {
    if depth > 64 {
        return Err(ProfileWriteError::Limit);
    }
    match value {
        RawValue::Null => append(out, "null")?,
        RawValue::Bool(v) => append(out, if *v { "true" } else { "false" })?,
        RawValue::Integer(v) => append(out, v)?,
        RawValue::String(v) => quoted(out, v)?,
        RawValue::Timestamp(v) => {
            append(out, "!!timestamp ")?;
            quoted(out, v)?;
        }
        RawValue::Float(v) => {
            append(out, "!!float ")?;
            quoted(
                out,
                &if v.is_nan() {
                    ".nan".into()
                } else if *v == f64::INFINITY {
                    ".inf".into()
                } else if *v == f64::NEG_INFINITY {
                    "-.inf".into()
                } else {
                    format!("{v:?}")
                },
            )?;
        }
        RawValue::Sequence(values) => {
            append(out, "[")?;
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    append(out, ", ")?;
                }
                emit(value, out, depth + 1)?;
            }
            append(out, "]")?;
        }
        RawValue::Mapping(values) => {
            append(out, "{")?;
            for (index, (key, value)) in values.iter().enumerate() {
                if index > 0 {
                    append(out, ", ")?;
                }
                match key {
                    LegacyKey::String(v) => quoted(out, v)?,
                    LegacyKey::Null => append(out, "null")?,
                    LegacyKey::Timestamp(v) => {
                        append(out, "!!timestamp ")?;
                        quoted(out, v)?;
                    }
                    LegacyKey::Number(v) => {
                        let value = match v.as_str() {
                            "inf" => ".inf",
                            "-inf" => "-.inf",
                            "nan" => ".nan",
                            _ => v,
                        };
                        if value.chars().any(|c| matches!(c, '.' | 'e' | 'E')) {
                            append(out, "!!float ")?;
                            quoted(out, value)?;
                        } else {
                            append(out, value)?;
                        }
                    }
                }
                append(out, ": ")?;
                emit(value, out, depth + 1)?;
            }
            append(out, "}")?;
        }
        RawValue::MergeKey => return Err(ProfileWriteError::Read),
    }
    Ok(())
}
pub fn save_profile(
    path: &Path,
    editing_id: Option<&str>,
    draft: ProfileDraft,
    revision: &str,
    catalog: &ProfileCatalog,
) -> Result<ProfileSaved, ProfileWriteError> {
    save_profile_with_timings(path, editing_id, draft, revision, catalog, None)
}
/// `timings` is a complete editor snapshot; omitted snapshots preserve the old YAML.
pub fn save_profile_with_timings(
    path: &Path,
    editing_id: Option<&str>,
    draft: ProfileDraft,
    revision: &str,
    catalog: &ProfileCatalog,
    timings: Option<&serde_json::Value>,
) -> Result<ProfileSaved, ProfileWriteError> {
    save_profile_changes(
        path,
        editing_id,
        draft,
        revision,
        catalog,
        ProfileEdits {
            timings,
            notifications: None,
        },
    )
}
#[derive(Default)]
pub struct ProfileEdits<'a> {
    pub timings: Option<&'a serde_json::Value>,
    pub notifications: Option<&'a crate::notification_metadata::NotificationMetadata>,
}
pub fn save_profile_changes(
    path: &Path,
    editing_id: Option<&str>,
    mut draft: ProfileDraft,
    revision: &str,
    catalog: &ProfileCatalog,
    edits: ProfileEdits<'_>,
) -> Result<ProfileSaved, ProfileWriteError> {
    if let Some(metadata) = edits.notifications {
        metadata.validate()?;
    }
    let timings = edits.timings;
    let timings = timings
        .map(|raw| {
            ceh_core::timings::parse_timings(raw)
                .map_err(|_| ProfileWriteError::Validation(vec!["Invalid profile timings".into()]))
        })
        .transpose()?;
    let lock = lock_profile(path)?;
    let bundle = read_profiles(path, catalog, "en").map_err(|_| ProfileWriteError::Read)?;
    if bundle.revision() != revision {
        return Err(ProfileWriteError::Conflict);
    }
    let mut root = bundle.raw().root().clone();
    let tasks = map(&mut root)?
        .entry(LegacyKey::String("tasks".into()))
        .or_insert_with(|| Arc::new(RawValue::Sequence(vec![])));
    let RawValue::Sequence(tasks) = Arc::make_mut(tasks) else {
        return Err(ProfileWriteError::Read);
    };
    let index = if let Some(id) = editing_id {
        Some(
            tasks
                .iter()
                .position(|task| {
                    task.get("id").and_then(|v| v.scalar_text()).as_deref() == Some(id)
                })
                .ok_or(ProfileWriteError::Conflict)?,
        )
    } else {
        None
    };
    let id = editing_id.map(str::to_owned).unwrap_or_else(|| {
        format!(
            "person_{}",
            &uuid::Uuid::new_v4().simple().to_string()[..12]
        )
    });
    draft.id = id.clone();
    let report = validate_desktop_profile(draft, catalog);
    if !report.errors.is_empty() {
        return Err(ProfileWriteError::Validation(report.errors));
    }
    let draft = report.normalized;
    let mut task = index
        .map(|index| tasks[index].as_ref().clone())
        .unwrap_or(RawValue::Mapping(BTreeMap::new()));
    for (path, value) in [
        (&["id"][..], id.as_str()),
        (&["label"], draft.label.as_str()),
        (&["request", "province"], draft.province.as_str()),
        (&["request", "browser"], draft.browser.as_str()),
        (&["request", "office"], draft.office.as_str()),
        (&["request", "access_mode"], draft.access_mode.as_str()),
        (&["request", "clave_method"], draft.clave_method.as_str()),
        (&["request", "no_slots_mode"], draft.no_slots_mode.as_str()),
        (
            &["request", "applicant_no_slots_mode"],
            draft.applicant_no_slots_mode.as_str(),
        ),
        (
            &["applicant", "document_type"],
            draft.document_type.as_str(),
        ),
        (
            &["applicant", "document_number"],
            draft.document_number.as_str(),
        ),
        (&["applicant", "full_name"], draft.full_name.as_str()),
        (&["applicant", "nationality"], draft.nationality.as_str()),
        (&["applicant", "fill_mode"], draft.fill_mode.as_str()),
        (&["applicant", "contact", "phone"], draft.phone.as_str()),
        (&["applicant", "contact", "email"], draft.email.as_str()),
        (&["schedule", "timezone"], draft.timezone.as_str()),
    ] {
        text(&mut task, path, value)?;
    }
    text(
        &mut task,
        &["request", "procedure"],
        if draft.procedure == "regreso" {
            "AUTORIZACIÓN DE REGRESO"
        } else {
            "POLICÍA-TOMA DE HUELLAS (EXPEDICIÓN DE TARJETA) INICIAL, RENOVACIÓN, DUPLICADO Y LEY 14/2013"
        },
    )?;
    set(&mut task, &["enabled"], RawValue::Bool(draft.enabled))?;
    set(
        &mut task,
        &["request", "subprocedure"],
        draft
            .subprocedure
            .map(RawValue::String)
            .unwrap_or(RawValue::Null),
    )?;
    if let Some(timings) = timings {
        let values = serde_json::to_value(timings).map_err(|_| ProfileWriteError::Read)?;
        for (key, value) in values.as_object().ok_or(ProfileWriteError::Read)? {
            let raw = if let Some(items) = value.as_array() {
                RawValue::Sequence(
                    items
                        .iter()
                        .map(|v| Arc::new(RawValue::Float(v.as_f64().expect("validated number"))))
                        .collect(),
                )
            } else {
                RawValue::Float(value.as_f64().expect("validated number"))
            };
            set(&mut task, &["timings", key], raw)?;
        }
    }
    if let Some(metadata) = edits.notifications {
        let value = serde_json::to_value(metadata).map_err(|_| ProfileWriteError::Read)?;
        for name in ["telegram", "webhook"] {
            for (key, value) in value[name].as_object().ok_or(ProfileWriteError::Read)? {
                let replacement = if name == "telegram" && key == "recipients" {
                    merge_recipients(
                        task.get("notifications")
                            .and_then(|v| v.get("telegram"))
                            .and_then(|v| v.get("recipients")),
                        value,
                    )?
                } else {
                    metadata_value(value)?
                };
                set(&mut task, &["notifications", name, key], replacement)?;
            }
        }
    }
    if let Some(index) = index {
        tasks[index] = Arc::new(task);
    } else {
        tasks.push(Arc::new(task));
    }
    let new_revision = commit_root(path, &root, catalog)?;
    drop(lock);
    Ok(ProfileSaved {
        id,
        revision: new_revision,
    })
}

fn lock_profile(path: &Path) -> Result<std::fs::File, ProfileWriteError> {
    if std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(ProfileWriteError::Unavailable);
    }
    let parent = path.parent().ok_or(ProfileWriteError::Unavailable)?;
    std::fs::create_dir_all(parent).map_err(|_| ProfileWriteError::Unavailable)?;
    let lock_path = parent.join(format!(
        ".{}.lock",
        path.file_name()
            .and_then(|s| s.to_str())
            .ok_or(ProfileWriteError::Unavailable)?
    ));
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let lock = options
        .open(lock_path)
        .map_err(|_| ProfileWriteError::Unavailable)?;
    let deadline = Instant::now() + Duration::from_millis(100);
    loop {
        match fs2::FileExt::try_lock_exclusive(&lock) {
            Ok(()) => break,
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(_) => return Err(ProfileWriteError::Unavailable),
        }
    }
    Ok(lock)
}

fn commit_root(
    path: &Path,
    root: &RawValue,
    catalog: &ProfileCatalog,
) -> Result<String, ProfileWriteError> {
    let mut output = String::new();
    emit(root, &mut output, 0)?;
    append(&mut output, "\n")?;
    let verified = crate::profiles::decode_profiles(output.as_bytes(), catalog, "en")
        .map_err(|_| ProfileWriteError::Read)?;
    let new_revision = verified.revision().to_owned();
    let mut temporary =
        tempfile::NamedTempFile::new_in(path.parent().ok_or(ProfileWriteError::Unavailable)?)
            .map_err(|_| ProfileWriteError::Unavailable)?;
    temporary
        .write_all(output.as_bytes())
        .map_err(|_| ProfileWriteError::Unavailable)?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| ProfileWriteError::Unavailable)?;
    temporary
        .persist(path)
        .map_err(|_| ProfileWriteError::Unavailable)?;
    Ok(new_revision)
}

pub fn save_selection(
    path: &Path,
    id: &str,
    catalog: &ProfileCatalog,
) -> Result<(), ProfileWriteError> {
    let _lock = lock_profile(path)?;
    let bundle = read_profiles(path, catalog, "en").map_err(|_| ProfileWriteError::Read)?;
    if !bundle
        .profiles
        .iter()
        .any(|p| p.validation.normalized.id == id)
    {
        return Err(ProfileWriteError::Conflict);
    }
    if bundle.last_profile_id.as_deref() == Some(id) {
        return Ok(());
    }
    let mut root = bundle.raw().root().clone();
    text(&mut root, &["desktop", "last_profile_id"], id)?;
    commit_root(path, &root, catalog)?;
    Ok(())
}
pub fn save_preferences(
    path: &Path,
    preferences: &crate::profiles::DesktopPreferences,
    catalog: &ProfileCatalog,
) -> Result<(), ProfileWriteError> {
    if !["ru", "en", "es"].contains(&preferences.language.as_str()) {
        return Err(ProfileWriteError::Validation(vec![
            "Unsupported interface language".into(),
        ]));
    }
    let _lock = lock_profile(path)?;
    let bundle = read_profiles(path, catalog, "en").map_err(|_| ProfileWriteError::Read)?;
    let mut root = bundle.raw().root().clone();
    for (key, value) in [
        ("dark", preferences.dark),
        ("diagnostics", preferences.diagnostics),
        ("sound", preferences.sound),
        ("automatic_updates", preferences.automatic_updates),
    ] {
        set(&mut root, &["desktop", key], RawValue::Bool(value))?;
    }
    text(&mut root, &["desktop", "language"], &preferences.language)?;
    commit_root(path, &root, catalog)?;
    Ok(())
}

fn metadata_value(value: &serde_json::Value) -> Result<RawValue, ProfileWriteError> {
    use serde_json::Value;
    Ok(match value {
        Value::Null => RawValue::Null,
        Value::Bool(b) => RawValue::Bool(*b),
        Value::String(s) => RawValue::String(s.clone()),
        Value::Array(items) => RawValue::Sequence(
            items
                .iter()
                .map(|v| metadata_value(v).map(Arc::new))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(items) => RawValue::Mapping(
            items
                .iter()
                .map(|(key, v)| Ok((LegacyKey::String(key.clone()), Arc::new(metadata_value(v)?))))
                .collect::<Result<_, ProfileWriteError>>()?,
        ),
        _ => return Err(ProfileWriteError::Read),
    })
}

fn merge_recipients(
    old: Option<&RawValue>,
    new: &serde_json::Value,
) -> Result<RawValue, ProfileWriteError> {
    let mut result = Vec::new();
    for value in new.as_array().ok_or(ProfileWriteError::Read)? {
        let id = value["id"].as_str().ok_or(ProfileWriteError::Read)?;
        let previous = old.and_then(RawValue::sequence).and_then(|items| {
            items
                .iter()
                .find(|r| r.get("id").and_then(RawValue::scalar_text).as_deref() == Some(id))
        });
        let mut recipient = previous
            .map(|v| v.as_ref().clone())
            .unwrap_or(RawValue::Mapping(BTreeMap::new()));
        for (key, value) in value.as_object().ok_or(ProfileWriteError::Read)? {
            set(&mut recipient, &[key], metadata_value(value)?)?;
        }
        result.push(Arc::new(recipient));
    }
    Ok(RawValue::Sequence(result))
}
