//! Python 0.3.0-compatible local budget files, keyed by a private HMAC.
use ceh_core::{appointment_budget::AppointmentBudget, model::ProfileId, profiles::ProfileDraft};
use hmac::{Hmac, KeyInit, Mac};
use sha2_budget::Sha256;
use std::{fs::{self, File, OpenOptions}, io::{self, Write}, path::{Path, PathBuf}};

pub struct BudgetStore {
    path: PathBuf,
    _lock: File,
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
        let mut mac = Hmac::<Sha256>::new_from_slice(&secret).map_err(io::Error::other)?;
        mac.update(identity.as_bytes());
        let key = mac.finalize().into_bytes().iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        let store = Self {
            path: root.join(format!("{key}.json")),
            _lock: lock(&data.join(format!(".budget-{key}.lock")))?,
        };
        if profile.fill_mode != "copy" || copied.is_some() {
            atomic_write(&root.join(format!("profile-{}.json", id.as_str())), &serde_json::to_vec(&serde_json::json!({"key":key}))?)?;
        }
        Ok(store)
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
        let key = mapping["key"].as_str()
            .filter(|key| key.len() == 64 && key.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)))
            .ok_or(io::ErrorKind::InvalidData)?;
        match fs::read(root.join(format!("{key}.json"))) {
            Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(Into::into),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn load(&self) -> io::Result<AppointmentBudget> {
        match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(Into::into),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(AppointmentBudget::default()),
            Err(error) => Err(error),
        }
    }

    pub fn save(&self, budget: &AppointmentBudget) -> io::Result<()> {
        atomic_write(&self.path, &serde_json::to_vec(budget)?)
    }
}

fn lock(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new().create(true).truncate(false).read(true).write(true).open(path)?;
    fs2::FileExt::try_lock_exclusive(&file)?;
    Ok(file)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(path.parent().ok_or(io::ErrorKind::InvalidInput)?)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}
