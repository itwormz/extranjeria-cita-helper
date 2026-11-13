//! Python 0.3.0-compatible local budget files, keyed by a private HMAC.
use ceh_core::{appointment_budget::AppointmentBudget, model::ProfileId, profiles::ProfileDraft};
use hmac::{Hmac, KeyInit, Mac};
use sha2_budget::{Digest, Sha256};
use std::{fs::{self, File, OpenOptions}, io::{self, Write}, path::{Path, PathBuf}};

pub struct BudgetStore {
    path: PathBuf,
    _locks: Vec<File>,
    aliases: Vec<PathBuf>,
}

impl BudgetStore {
    pub fn open(data: &Path, profile: &ProfileDraft) -> io::Result<Self> {
        Self::open_for_document(data, profile, None)
    }

    pub fn open_for_document(data: &Path, profile: &ProfileDraft, copied: Option<(&str, &str)>) -> io::Result<Self> {
        let id = ProfileId::new(&profile.id).map_err(io::Error::other)?;
        let root = data.join("appointment-budget");
        fs::create_dir_all(&root)?;
        let key_lock = lock(&root.join(".key.lock"))?;
        let key_path = root.join("key");
        if fs::symlink_metadata(&key_path).is_ok_and(|m|!m.file_type().is_file()||m.len()!=32) {return Err(io::ErrorKind::InvalidData.into());}
        let secret = match fs::read(&key_path) {
            Ok(secret) => secret,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let mut secret = Vec::with_capacity(32);
                secret.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
                secret.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
                atomic_write(&key_path, &secret)?;
                secret
            }
            Err(error) => return Err(error),
        };
        drop(key_lock);
        if secret.len() != 32 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid budget key"));
        }
        let (kind, number) = copied.unwrap_or((&profile.document_type, &profile.document_number));
        let document: String = number.to_uppercase().split_whitespace().collect();
        let identity = if document.is_empty() || (profile.fill_mode == "copy" && copied.is_none()) {
            format!("profile:{}", id.as_str())
        } else {
            format!("{}:{document}", kind.to_uppercase())
        };
        let current = document_key(&secret, &identity)?;
        let mut keys = vec![current.clone()];
        let mut imported = fs::read_dir(&root)?.filter_map(Result::ok).filter_map(|entry| {
            let name=entry.file_name(); let name=name.to_str()?;
            is_import_key(name).then_some(entry.path())
        }).collect::<Vec<_>>();
        imported.sort();
        if imported.len()>32 { return Err(io::Error::new(io::ErrorKind::InvalidData,"too many imported budget keys")); }
        for path in imported {
            if !fs::symlink_metadata(&path)?.file_type().is_file() || fs::metadata(&path)?.len()!=32 { return Err(io::ErrorKind::InvalidData.into()); }
            let secret=fs::read(&path)?;
            let digest=Sha256::digest(&secret).iter().map(|b|format!("{b:02x}")).collect::<String>();
            if path.file_name().and_then(|n|n.to_str())!=Some(format!("import-key-{digest}.key").as_str()) {return Err(io::ErrorKind::InvalidData.into());}
            let key = document_key(&secret,&identity)?;
            if !keys.contains(&key) && root.join(format!("{key}.json")).is_file() { keys.push(key); }
        }
        // Always consolidate into the current identity. Keep imported originals as immutable
        // inputs, with their consumed digests recorded atomically in the canonical budget.
        let primary = current;
        let aliases=keys.iter().filter(|key|**key!=primary && root.join(format!("{key}.json")).is_file()).map(|key|root.join(format!("{key}.json"))).collect();
        // Also lock the current-salt identity even when an imported file is selected. Every
        // manual/copy profile for this document obtains exactly the same lock set.
        keys.sort(); keys.dedup();
        let locks=keys.iter().map(|key|lock(&data.join(format!(".budget-{key}.lock")))).collect::<io::Result<Vec<_>>>()?;
        let store = Self { path: root.join(format!("{primary}.json")), _locks:locks, aliases };
        consolidate_imports(&store.path,&store.aliases)?;
        if profile.fill_mode != "copy" || copied.is_some() { store.remember_profile(data,id.as_str())?; }
        Ok(store)
    }

    /// The budget a profile edits while no search runs: the last document's file for a copy profile that
    /// was bound once, otherwise the profile's own budget. The file is locked until the store is dropped.
    pub fn open_saved(data: &Path, profile: &ProfileDraft) -> io::Result<Self> {
        if profile.fill_mode == "copy" {
            let id = ProfileId::new(&profile.id).map_err(io::Error::other)?;
            if let Some(key) = Self::profile_key(&data.join("appointment-budget"), id.as_str())? {
                let root=data.join("appointment-budget");
                let mapping:serde_json::Value=serde_json::from_slice(&fs::read(root.join(format!("profile-{}.json",id.as_str())))?)?;
                let keys=mapping_keys(&mapping)?;
                let locks=keys.iter().map(|key|lock(&data.join(format!(".budget-{key}.lock")))).collect::<io::Result<Vec<_>>>()?;
                let store=Self { path:root.join(format!("{key}.json")),_locks:locks,aliases:keys.into_iter().filter(|k|k!=&key).map(|k|root.join(format!("{k}.json"))).collect() };
                consolidate_imports(&store.path,&store.aliases)?;
                return Ok(store);
            }
        }
        Self::open(data, profile)
    }

    fn profile_key(root: &Path, id: &str) -> io::Result<Option<String>> {
        let mapping: serde_json::Value = match fs::read(root.join(format!("profile-{id}.json"))) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        mapping["key"]
            .as_str()
            .filter(|key| key.len() == 64 && key.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)))
            .map(|key| Some(key.to_owned()))
            .ok_or_else(|| io::ErrorKind::InvalidData.into())
    }

    pub fn exists(&self) -> bool {
        self.path.is_file()
    }

    /// Bind the profile to this budget file (what a finished search leaves behind for copy profiles).
    pub fn remember_profile(&self, data: &Path, id: &str) -> io::Result<()> {
        let id = ProfileId::new(id).map_err(io::Error::other)?;
        let key = self.path.file_stem().and_then(|stem| stem.to_str()).ok_or(io::ErrorKind::InvalidData)?;
        let aliases=self.aliases.iter().filter_map(|p|p.file_stem()?.to_str()).collect::<Vec<_>>();
        atomic_write(&data.join("appointment-budget").join(format!("profile-{}.json", id.as_str())), &serde_json::to_vec(&serde_json::json!({"key":key,"aliases":aliases}))?)
    }

    pub fn load_profile(data: &Path, id: &str) -> io::Result<Option<AppointmentBudget>> {
        let id = ProfileId::new(id).map_err(io::Error::other)?;
        let root = data.join("appointment-budget");
        let map_path = root.join(format!("profile-{}.json", id.as_str()));
        let mapping: serde_json::Value = match fs::read(map_path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let keys=mapping_keys(&mapping)?;
        let key=mapping["key"].as_str().ok_or(io::ErrorKind::InvalidData)?;
        let path=root.join(format!("{key}.json"));
        if !path.is_file() {return Ok(None);}
        let aliases=keys.into_iter().filter(|k|k!=key).map(|k|root.join(format!("{k}.json"))).collect::<Vec<_>>();
        load_merged(&path,&aliases).map(Some)
    }

    /// Refresh one importer-owned snapshot without replacing local choices or replaying the
    /// same incoming observations. The receipt and budget become durable in one replacement.
    pub(crate) fn merge_imported_snapshot(source:&Path,target:&Path,data:&Path,key:&str,backup:&Path)->io::Result<bool> {
        if !valid_key(key) {return Err(io::ErrorKind::InvalidInput.into());}
        let _lock=lock(&data.join(format!(".budget-{key}.lock")))?;
        let (value,changed)=merged_document(target,&[source.to_owned()])?;
        if changed {fs::copy(target,backup)?;write_budget(target,&value)?;}
        Ok(changed)
    }

    pub fn load(&self) -> io::Result<AppointmentBudget> { read_budget(&self.path) }

    pub fn save(&self, budget: &AppointmentBudget) -> io::Result<()> {
        let mut value=serde_json::to_value(budget)?;
        let consumed=consumed_imports(&read_document(&self.path)?)?;
        if !consumed.is_empty() {value["_imported_budgets"]=serde_json::Value::Object(consumed);}
        write_budget(&self.path,&value)
    }
}

/// Only salt archives created by the importer; never follow arbitrary names or links.
pub(crate) fn is_import_key(name:&str)->bool {
    name.strip_prefix("import-key-").and_then(|v|v.strip_suffix(".key")).is_some_and(valid_key)
}
fn valid_key(key:&str)->bool {key.len()==64&&key.bytes().all(|c|c.is_ascii_digit()||(b'a'..=b'f').contains(&c))}
fn document_key(secret:&[u8],identity:&str)->io::Result<String> {
    if secret.len()!=32 {return Err(io::ErrorKind::InvalidData.into());}
    let mut mac=Hmac::<Sha256>::new_from_slice(secret).map_err(io::Error::other)?;mac.update(identity.as_bytes());
    Ok(mac.finalize().into_bytes().iter().map(|b|format!("{b:02x}")).collect())
}
fn mapping_keys(mapping:&serde_json::Value)->io::Result<Vec<String>> {
    let key=mapping["key"].as_str().filter(|v|valid_key(v)).ok_or(io::ErrorKind::InvalidData)?;
    let mut keys=vec![key.to_owned()];
    if let Some(aliases)=mapping.get("aliases") {
        let aliases=aliases.as_array().filter(|a|a.len()<=32).ok_or(io::ErrorKind::InvalidData)?;
        for value in aliases {let key=value.as_str().filter(|v|valid_key(v)).ok_or(io::ErrorKind::InvalidData)?;if !keys.iter().any(|v|v==key){keys.push(key.to_owned());}}
    }
    keys.sort();Ok(keys)
}
fn read_document(path:&Path)->io::Result<serde_json::Value> {
    if fs::symlink_metadata(path).is_ok_and(|m|!m.file_type().is_file()||m.len()>1024*1024) {return Err(io::ErrorKind::InvalidData.into());}
    match fs::read(path) {
        Ok(bytes)=>serde_json::from_slice(&bytes).map_err(Into::into),
        Err(e) if e.kind()==io::ErrorKind::NotFound=>serde_json::to_value(AppointmentBudget::default()).map_err(Into::into),
        Err(e)=>Err(e)
    }
}
fn read_budget(path:&Path)->io::Result<AppointmentBudget> {
    serde_json::from_value::<AppointmentBudget>(read_document(path)?).map(AppointmentBudget::normalized).map_err(Into::into)
}
fn consumed_imports(value:&serde_json::Value)->io::Result<serde_json::Map<String,serde_json::Value>> {
    match value.get("_imported_budgets") {
        None=>Ok(serde_json::Map::new()),
        Some(value)=>value.as_object().filter(|map|map.len()<=32&&map.iter().all(|(key,digest)|valid_key(key)&&digest.as_str().is_some_and(valid_key))).cloned().ok_or_else(||io::ErrorKind::InvalidData.into())
    }
}
fn merged_document(path:&Path,aliases:&[PathBuf])->io::Result<(serde_json::Value,bool)> {
    let original=read_document(path)?;
    let mut current=serde_json::to_value(serde_json::from_value::<AppointmentBudget>(original.clone())?.normalized())?;
    let mut consumed=consumed_imports(&original)?;
    let mut changed=false;
    for alias in aliases {
        if !alias.try_exists()? {continue;}
        let incoming=read_document(alias)?;
        let incoming=serde_json::to_value(serde_json::from_value::<AppointmentBudget>(incoming)?.normalized())?;
        // The semantic, canonical representation avoids whitespace changes replaying a restriction.
        let bytes=serde_json::to_vec(&incoming)?;
        let digest=Sha256::digest(bytes).iter().map(|b|format!("{b:02x}")).collect::<String>();
        let key=alias.file_stem().and_then(|v|v.to_str()).filter(|v|valid_key(v)).ok_or(io::ErrorKind::InvalidData)?;
        if consumed.get(key).and_then(|v|v.as_str())==Some(&digest) {continue;}
        merge_observations(&mut current,&incoming);
        consumed.insert(key.into(),digest.into());changed=true;
    }
    if consumed.len()>32 {return Err(io::ErrorKind::InvalidData.into());}
    if !consumed.is_empty() {current["_imported_budgets"]=serde_json::Value::Object(consumed);}
    Ok((current,changed))
}
fn write_budget(path:&Path,value:&serde_json::Value)->io::Result<()> {
    let bytes=serde_json::to_vec(value)?;
    if bytes.len()>1024*1024 {return Err(io::ErrorKind::InvalidData.into());}
    atomic_write(path,&bytes)
}
fn consolidate_imports(path:&Path,aliases:&[PathBuf])->io::Result<()> {
    if aliases.is_empty() {return Ok(());}
    let (value,changed)=merged_document(path,aliases)?;
    if changed {write_budget(path,&value)?;}
    Ok(())
}
fn load_merged(path:&Path,aliases:&[PathBuf])->io::Result<AppointmentBudget> {
    let (value,_)=merged_document(path,aliases)?;
    serde_json::from_value::<AppointmentBudget>(value).map(AppointmentBudget::normalized).map_err(Into::into)
}
// Imported states may share old history with the current app. Maximum observations preserve the
// known lower bound without double-counting every repeated import; manual choices stay local.
fn merge_observations(current:&mut serde_json::Value,incoming:&serde_json::Value) {
    // A saved day scopes counters and manual choices, not a restriction episode. Compute
    // lasting facts before selecting either daily snapshot, so neither direction loses them.
    let restriction = merged_restriction(current, incoming);
    let last_submission = if budget_time(&incoming["last_submission"]) > budget_time(&current["last_submission"]) {
        incoming["last_submission"].clone()
    } else { current["last_submission"].clone() };
    let day=current["day"].as_str().unwrap_or("");let other_day=incoming["day"].as_str().unwrap_or("");
    if other_day>day {
        *current=incoming.clone();
    } else if day==other_day {
        let prior_sent=current["sent"].clone();
        for key in ["sent","banners","no_slots","cod_oper_count"] {
            let max=current[key].as_u64().unwrap_or(0).max(incoming[key].as_u64().unwrap_or(0));current[key]=max.into();
        }
        for key in ["target_override","next_target"] {if current[key].is_null(){current[key]=incoming[key].clone();}}
        if current["sent"]!=prior_sent {current["windows"]=serde_json::json!([]);}
    }
    if current["last_submission"]!=last_submission {current["windows"]=serde_json::json!([]);}
    current["last_submission"]=last_submission;
    for key in ["episode_history","first_cod_oper","last_cod_oper","restriction_until","episode_offices","offices_during_restriction"] {
        current[key]=restriction[key].clone();
    }
}
fn budget_time(value:&serde_json::Value)->Option<chrono::DateTime<chrono::Utc>> {
    value.as_str().and_then(|text|chrono::DateTime::parse_from_rfc3339(text).ok()).map(|at|at.to_utc())
}
fn merged_restriction(current:&serde_json::Value,incoming:&serde_json::Value)->serde_json::Value {
    let mut merged=current.clone();
    union_array(&mut merged,incoming,"episode_history");
    let recovered=merged["episode_history"].as_array().into_iter().flatten().filter_map(|episode|budget_time(&episode["recovered"])).max();
    for key in ["first_cod_oper","last_cod_oper","restriction_until"] {merged[key]=serde_json::Value::Null;}
    merged["episode_offices"]=serde_json::json!([]);
    let mut active_offices:Option<u64>=None;
    for source in [current,incoming] {
        let last=budget_time(&source["last_cod_oper"]).or_else(||budget_time(&source["first_cod_oper"]))
            .or_else(||budget_time(&source["restriction_until"]));
        let Some(last)=last else {continue};
        if recovered.is_some_and(|end|end>=last) {continue;}
        let mut facts=source.clone();
        if recovered.is_some_and(|end|budget_time(&source["first_cod_oper"]).is_some_and(|first|first<=end)) {
            // This source did not observe the local recovery. Its genuinely later response
            // starts a new episode; pre-recovery offices must stay in the closed history.
            facts["first_cod_oper"]=source["last_cod_oper"].clone();
            let offices=source["episode_offices"].as_array().into_iter().flatten()
                .filter(|office|budget_time(&office["at"]).is_some_and(|at|recovered.is_none_or(|end|at>end)))
                .cloned().collect::<Vec<_>>();
            facts["offices_during_restriction"]=(offices.len() as u64).into();
            facts["episode_offices"]=offices.into();
        }
        for key in ["first_cod_oper","last_cod_oper","restriction_until"] {
            if let Some(at)=budget_time(&facts[key]) {
                let previous=budget_time(&merged[key]);
                if previous.is_none_or(|old|if key=="first_cod_oper" {at<old} else {at>old}) {merged[key]=facts[key].clone();}
            }
        }
        union_array(&mut merged,&facts,"episode_offices");
        active_offices=Some(active_offices.unwrap_or(0).max(facts["offices_during_restriction"].as_u64().unwrap_or(0)));
    }
    // Outside an active episode the core retains the last known office count as well.
    merged["offices_during_restriction"]=active_offices.unwrap_or_else(||current["offices_during_restriction"].as_u64().unwrap_or(0)
        .max(incoming["offices_during_restriction"].as_u64().unwrap_or(0))).into();
    merged
}
fn union_array(current:&mut serde_json::Value,incoming:&serde_json::Value,key:&str) {
    let Some(other)=incoming[key].as_array() else{return};let Some(values)=current[key].as_array_mut()else{return};
    for value in other {if !values.contains(value){values.push(value.clone());}}
}

fn lock(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new().create(true).truncate(false).read(true).write(true).open(path)?;
    crate::file_lock::try_exclusive(&file)?;
    Ok(file)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(path.parent().ok_or(io::ErrorKind::InvalidInput)?)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    ceh_core::file_replace::replace(file, |file| file.persist(path).map(|_| ()).map_err(|e| (e.file, e.error)))
}
