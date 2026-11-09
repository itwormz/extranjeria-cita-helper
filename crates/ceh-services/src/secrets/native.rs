//! Explicit native stores. Construction does not initialize or read a credential store.
use super::{Backend, VaultError};
use keyring_core::{CredentialStore, Entry};
use std::sync::{Arc, Mutex, OnceLock};
use zeroize::Zeroize;
#[derive(Default)]
pub struct NativeBackend {
    store: OnceLock<Arc<CredentialStore>>,
    serial: Mutex<()>,
}
fn mapped(error: keyring_core::Error) -> VaultError {
    match error {
        keyring_core::Error::NoEntry => VaultError::Missing,
        keyring_core::Error::NoStorageAccess(_) => VaultError::Locked,
        keyring_core::Error::BadEncoding(mut data)
        | keyring_core::Error::BadDataFormat(mut data, _) => {
            data.zeroize();
            VaultError::Invalid
        }
        _ => VaultError::Unavailable,
    }
}
impl NativeBackend {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn initialized(&self) -> bool {
        self.store.get().is_some()
    }
    fn store(&self) -> Result<&Arc<CredentialStore>, VaultError> {
        initialize(&self.store, || {
            #[cfg(target_os = "macos")]
            let store = apple_native_keyring_store::keychain::Store::new().map_err(mapped)?;
            #[cfg(windows)]
            let store = windows_native_keyring_store::Store::new().map_err(mapped)?;
            #[cfg(target_os = "linux")]
            let store = zbus_secret_service_keyring_store::Store::new().map_err(mapped)?;
            #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
            return Err(VaultError::Unavailable);
            #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
            Ok(store as Arc<CredentialStore>)
        })
    }
    fn entry(&self, service: &str, key: &str) -> Result<Entry, VaultError> {
        #[cfg(windows)]
        {
            let target = format!("{key}@{service}");
            self.store()?
                .build(
                    service,
                    key,
                    Some(&std::collections::HashMap::from([(
                        "target",
                        target.as_str(),
                    )])),
                )
                .map_err(mapped)
        }
        #[cfg(not(windows))]
        {
            self.store()?.build(service, key, None).map_err(mapped)
        }
    }
    #[cfg(windows)]
    fn legacy_service_entry(&self, service: &str, key: &str) -> Result<Option<Entry>, VaultError> {
        let entry = self
            .store()?
            .build(
                service,
                key,
                Some(&std::collections::HashMap::from([("target", service)])),
            )
            .map_err(mapped)?;
        match entry.get_attributes() {
            Ok(attributes) if attributes.get("username").is_some_and(|user| user == key) => {
                Ok(Some(entry))
            }
            Ok(_) | Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(error) => Err(mapped(error)),
        }
    }
}
impl Backend for NativeBackend {
    fn get(&self, service: &str, key: &str) -> Result<Option<String>, VaultError> {
        let _serial = self.serial.lock().map_err(|_| VaultError::Unavailable)?;
        #[cfg(windows)]
        if let Some(entry) = self.legacy_service_entry(service, key)? {
            return entry.get_password().map(Some).map_err(mapped);
        }
        match self.entry(service, key)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(error) => Err(mapped(error)),
        }
    }
    fn set(&self, service: &str, key: &str, value: &str) -> Result<(), VaultError> {
        let _serial = self.serial.lock().map_err(|_| VaultError::Unavailable)?;
        self.entry(service, key)?
            .set_password(value)
            .map_err(mapped)
    }
    fn delete(&self, service: &str, key: &str) -> Result<(), VaultError> {
        let _serial = self.serial.lock().map_err(|_| VaultError::Unavailable)?;
        #[cfg(windows)]
        if let Some(entry) = self.legacy_service_entry(service, key)? {
            entry.delete_credential().map_err(mapped)?;
        }
        match self.entry(service, key)?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(error) => Err(mapped(error)),
        }
    }
}

// Callers serialize store operations; errors are never cached across explicit retries.
fn initialize<T: ?Sized>(
    cell: &OnceLock<Arc<T>>,
    create: impl FnOnce() -> Result<Arc<T>, VaultError>,
) -> Result<&Arc<T>, VaultError> {
    if let Some(store) = cell.get() {
        return Ok(store);
    }
    let store = create()?;
    let _ = cell.set(store);
    cell.get().ok_or(VaultError::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_failed_initialization_can_retry_after_unlock() {
        let cell = OnceLock::new();
        assert_eq!(
            initialize(&cell, || Err(VaultError::Locked)).err(),
            Some(VaultError::Locked)
        );
        assert!(initialize(&cell, || Ok(Arc::new(()))).is_ok());
    }
}
