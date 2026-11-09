//! Existing Python vault format, with explicit lazy native access and no file fallback.
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use zeroize::{Zeroize, Zeroizing};
pub const SERVICE: &str = "Cita Extranjeria Helper notifications";
const MAX_CHUNKS: usize = 256;
const CHUNK: usize = 1000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VaultError {
    Unavailable,
    Locked,
    Missing,
    Invalid,
    TooLarge,
}
impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "notification secrets: {self:?}")
    }
}
impl std::error::Error for VaultError {}
pub trait Backend {
    fn get(&self, service: &str, key: &str) -> Result<Option<String>, VaultError>;
    fn set(&self, service: &str, key: &str, value: &str) -> Result<(), VaultError>;
    fn delete(&self, service: &str, key: &str) -> Result<(), VaultError>;
}
pub struct SecretData(Value);
impl SecretData {
    pub fn new(mut value: Value) -> Result<Self, VaultError> {
        if value.is_object() {
            Ok(Self(value))
        } else {
            wipe(&mut value);
            Err(VaultError::Invalid)
        }
    }
    pub fn value(&self) -> &Value {
        &self.0
    }
}
impl std::fmt::Debug for SecretData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretData([REDACTED])")
    }
}
fn wipe(value: &mut Value) {
    match value {
        Value::String(text) => text.zeroize(),
        Value::Array(values) => values.iter_mut().for_each(wipe),
        Value::Object(values) => {
            for (mut key, mut value) in std::mem::take(values) {
                key.zeroize();
                wipe(&mut value);
            }
        }
        _ => {}
    }
}
impl Drop for SecretData {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}
pub struct Vault<B> {
    backend: B,
}
impl<B: Backend> Vault<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }
    pub fn backend(&self) -> &B {
        &self.backend
    }
    pub fn put(&self, data: &SecretData) -> Result<String, VaultError> {
        let bytes =
            Zeroizing::new(serde_json::to_vec(data.value()).map_err(|_| VaultError::Invalid)?);
        let encoded = Zeroizing::new(STANDARD.encode(&*bytes));
        if encoded.len() > MAX_CHUNKS * CHUNK {
            return Err(VaultError::TooLarge);
        }
        let reference = format!("notifications-{}", uuid::Uuid::new_v4().simple());
        let mut written: Vec<String> = Vec::new();
        for (index, chunk) in encoded.as_bytes().chunks(CHUNK).enumerate() {
            let key = format!("{reference}-{index}");
            if let Err(error) = self.backend.set(
                SERVICE,
                &key,
                std::str::from_utf8(chunk).map_err(|_| VaultError::Invalid)?,
            ) {
                for key in written {
                    let _ = self.backend.delete(SERVICE, &key);
                }
                return Err(error);
            }
            written.push(key);
        }
        let manifest = json!({"_ceh_chunks":written.len()}).to_string();
        if let Err(error) = self.backend.set(SERVICE, &reference, &manifest) {
            for key in written {
                let _ = self.backend.delete(SERVICE, &key);
            }
            return Err(error);
        }
        Ok(reference)
    }
    pub fn get(&self, reference: &str) -> Result<SecretData, VaultError> {
        if reference.is_empty() {
            return SecretData::new(json!({}));
        }
        if reference.len() > 256 || !reference.starts_with("notifications-") {
            return Err(VaultError::Invalid);
        }
        let raw = Zeroizing::new(
            self.backend
                .get(SERVICE, reference)?
                .ok_or(VaultError::Missing)?,
        );
        if raw.len() > MAX_CHUNKS * CHUNK {
            return Err(VaultError::TooLarge);
        }
        let result: Value = serde_json::from_str(&raw).map_err(|_| VaultError::Invalid)?;
        let result = SecretData::new(result)?;
        let Some(count) = result.value().get("_ceh_chunks") else {
            return Ok(result);
        };
        // Python considers bool an int, but an application-written manifest always stores an integer.
        let count = count
            .as_u64()
            .filter(|n| (1..=MAX_CHUNKS as u64).contains(n))
            .ok_or(VaultError::Invalid)?;
        let mut encoded = Zeroizing::new(String::new());
        for index in 0..count {
            let chunk = Zeroizing::new(
                self.backend
                    .get(SERVICE, &format!("{reference}-{index}"))?
                    .ok_or(VaultError::Missing)?,
            );
            if chunk.len() > CHUNK {
                return Err(VaultError::TooLarge);
            }
            encoded.push_str(&chunk);
        }
        let bytes = Zeroizing::new(
            STANDARD
                .decode(encoded.as_bytes())
                .map_err(|_| VaultError::Invalid)?,
        );
        SecretData::new(serde_json::from_slice(&bytes).map_err(|_| VaultError::Invalid)?)
    }
    /// Best-effort cleanup. Failure retains the saved reference; never claims deletion succeeded.
    pub fn delete(&self, reference: &str) {
        if reference.is_empty() || reference.len() > 256 || !reference.starts_with("notifications-")
        {
            return;
        }
        let Ok(Some(raw)) = self.backend.get(SERVICE, reference) else {
            return;
        };
        let raw = Zeroizing::new(raw);
        let Ok(manifest) = serde_json::from_str::<Value>(&raw)
            .map_err(|_| VaultError::Invalid)
            .and_then(SecretData::new)
        else {
            return;
        };
        let count = manifest
            .value()
            .get("_ceh_chunks")
            .map_or(Some(0), Value::as_u64);
        let Some(count) = count.filter(|n| *n <= MAX_CHUNKS as u64) else {
            return;
        };
        for index in 0..count {
            if self
                .backend
                .delete(SERVICE, &format!("{reference}-{index}"))
                .is_err()
            {
                return;
            }
        }
        let _ = self.backend.delete(SERVICE, reference);
    }
}
#[cfg(feature = "native-vault")]
pub mod native;
