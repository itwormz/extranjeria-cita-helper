#![cfg(all(feature = "native-vault", target_os = "macos"))]
use ceh_services::secrets::{Backend, SecretData, Vault, VaultError, native::NativeBackend};
struct Isolated {
    backend: NativeBackend,
    service: String,
}
impl Backend for Isolated {
    fn get(&self, _: &str, key: &str) -> Result<Option<String>, VaultError> {
        self.backend.get(&self.service, key)
    }
    fn set(&self, _: &str, key: &str, value: &str) -> Result<(), VaultError> {
        self.backend.set(&self.service, key, value)
    }
    fn delete(&self, _: &str, key: &str) -> Result<(), VaultError> {
        self.backend.delete(&self.service, key)
    }
}
#[test]
#[ignore = "explicit opt-in; creates only fictitious entries under a unique test service"]
fn native_mac_roundtrip_in_isolated_service() {
    assert_eq!(std::env::var("CEH_NATIVE_VAULT_TEST").as_deref(), Ok("1"));
    let service = format!("CEH Rust migration TEST {}", uuid::Uuid::new_v4());
    let backend = Isolated {
        backend: NativeBackend::new(),
        service,
    };
    // Exact pre-chunk Python format must remain readable.
    backend
        .set(
            "",
            "notifications-legacy-fixture",
            r#"{"token":"FICTITIOUS"}"#,
        )
        .unwrap();
    let vault = Vault::new(backend);
    let legacy = vault.get("notifications-legacy-fixture");
    vault.delete("notifications-legacy-fixture");
    assert_eq!(legacy.unwrap().value()["token"], "FICTITIOUS");
    let data = SecretData::new(serde_json::json!({"body":"fictitious".repeat(500)})).unwrap();
    let reference = vault.put(&data).unwrap();
    let restored = vault.get(&reference);
    vault.delete(&reference);
    assert_eq!(restored.unwrap().value(), data.value());
    assert_eq!(vault.get(&reference).err(), Some(VaultError::Missing));
}
