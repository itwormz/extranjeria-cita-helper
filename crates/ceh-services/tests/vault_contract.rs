use ceh_services::secrets::{Backend, SERVICE, SecretData, Vault, VaultError};
use std::{cell::RefCell, collections::BTreeMap};
#[derive(Default)]
struct Memory {
    values: RefCell<BTreeMap<(String, String), String>>,
    calls: RefCell<usize>,
}
impl Backend for Memory {
    fn get(&self, service: &str, key: &str) -> Result<Option<String>, VaultError> {
        *self.calls.borrow_mut() += 1;
        Ok(self
            .values
            .borrow()
            .get(&(service.into(), key.into()))
            .cloned())
    }
    fn set(&self, service: &str, key: &str, value: &str) -> Result<(), VaultError> {
        *self.calls.borrow_mut() += 1;
        self.values
            .borrow_mut()
            .insert((service.into(), key.into()), value.into());
        Ok(())
    }
    fn delete(&self, service: &str, key: &str) -> Result<(), VaultError> {
        *self.calls.borrow_mut() += 1;
        self.values
            .borrow_mut()
            .remove(&(service.into(), key.into()));
        Ok(())
    }
}
#[test]
fn lazy_and_chunk_roundtrip() {
    let backend = Memory::default();
    let vault = Vault::new(backend);
    assert_eq!(*vault.backend().calls.borrow(), 0);
    let data =
        SecretData::new(serde_json::json!({"token":"FICTITIOUS","body":"x".repeat(6000)})).unwrap();
    let reference = vault.put(&data).unwrap();
    assert!(reference.starts_with("notifications-"));
    assert_eq!(vault.get(&reference).unwrap().value(), data.value());
    assert!(
        vault
            .backend()
            .values
            .borrow()
            .values()
            .all(|value| value.len() <= 1000)
    );
    vault.delete(&reference);
    assert!(vault.backend().values.borrow().is_empty());
}
#[test]
fn legacy_direct_json_is_accepted_and_missing_rejected() {
    let backend = Memory::default();
    backend
        .set(
            SERVICE,
            "notifications-fixture",
            r#"{"token":"FICTITIOUS"}"#,
        )
        .unwrap();
    let vault = Vault::new(backend);
    assert_eq!(
        vault.get("notifications-fixture").unwrap().value()["token"],
        "FICTITIOUS"
    );
    assert_eq!(
        vault.get("notifications-missing").err(),
        Some(VaultError::Missing)
    );
    assert_eq!(vault.get("").unwrap().value(), &serde_json::json!({}));
}
#[test]
fn malformed_manifest_and_missing_chunk_fail_closed() {
    for raw in [
        r#"{"_ceh_chunks":0}"#,
        r#"{"_ceh_chunks":257}"#,
        r#"{"_ceh_chunks":"2"}"#,
        r#"{"_ceh_chunks":2}"#,
        "[]",
    ] {
        let backend = Memory::default();
        backend.set(SERVICE, "notifications-fixture", raw).unwrap();
        let vault = Vault::new(backend);
        assert!(vault.get("notifications-fixture").is_err());
    }
}

#[test]
fn failed_chunk_or_manifest_write_cleans_only_new_entries() {
    struct Failing {
        memory: Memory,
        fail_at: usize,
    }
    impl Backend for Failing {
        fn get(&self, s: &str, k: &str) -> Result<Option<String>, VaultError> {
            self.memory.get(s, k)
        }
        fn set(&self, s: &str, k: &str, v: &str) -> Result<(), VaultError> {
            if self.memory.values.borrow().len() == self.fail_at {
                Err(VaultError::Locked)
            } else {
                self.memory.set(s, k, v)
            }
        }
        fn delete(&self, s: &str, k: &str) -> Result<(), VaultError> {
            self.memory.delete(s, k)
        }
    }
    for fail_at in [0, 1, 2, 3] {
        let backend = Failing {
            memory: Memory::default(),
            fail_at,
        };
        let vault = Vault::new(backend);
        let data = SecretData::new(serde_json::json!({"body":"x".repeat(2000)})).unwrap();
        assert_eq!(vault.put(&data).err(), Some(VaultError::Locked));
        assert!(vault.backend().memory.values.borrow().is_empty());
    }
}
#[test]
fn secret_debug_and_errors_are_redacted() {
    let data = SecretData::new(serde_json::json!({"token":"FICTITIOUS_PRIVATE"})).unwrap();
    assert!(!format!("{data:?}").contains("FICTITIOUS_PRIVATE"));
    for error in [
        VaultError::Locked,
        VaultError::Unavailable,
        VaultError::Invalid,
    ] {
        assert!(!error.to_string().contains("FICTITIOUS_PRIVATE"));
    }
}
#[cfg(feature = "native-vault")]
#[test]
fn constructing_native_backend_does_not_initialize_storage() {
    let backend = ceh_services::secrets::native::NativeBackend::new();
    assert!(!backend.initialized());
    let _vault = Vault::new(backend);
}

#[test]
fn saved_python_entries_decode_identically() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../../../contracts/fixtures/vault.json")).unwrap();
    for case in data["cases"].as_array().unwrap() {
        let backend = Memory::default();
        for (key, value) in case["input"]["entries"].as_object().unwrap() {
            backend.set(SERVICE, key, value.as_str().unwrap()).unwrap();
        }
        let vault = Vault::new(backend);
        let result = vault.get(case["input"]["reference"].as_str().unwrap());
        let actual = match result {
            Ok(data) => serde_json::json!({"data":data.value()}),
            Err(_) => serde_json::json!({"error":"unreadable"}),
        };
        assert_eq!(actual, case["expected"], "{}", case["id"]);
    }
}
