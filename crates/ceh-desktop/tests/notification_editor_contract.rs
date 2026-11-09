use ceh_desktop::notification_editor::{NotificationDraft, editor, save};
use ceh_services::secrets::{Backend, Vault, VaultError};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    path::Path,
};
#[derive(Default)]
struct Memory {
    values: RefCell<BTreeMap<String, String>>,
    calls: Cell<usize>,
    writes: Cell<usize>,
    fail_at: Cell<Option<usize>>,
    locked: Cell<bool>,
    mutate_after_write: RefCell<Option<std::path::PathBuf>>,
}
impl Backend for Memory {
    fn get(&self, _: &str, key: &str) -> Result<Option<String>, VaultError> {
        self.calls.set(self.calls.get() + 1);
        if self.locked.get() {
            return Err(VaultError::Locked);
        }
        Ok(self.values.borrow().get(key).cloned())
    }
    fn set(&self, _: &str, key: &str, value: &str) -> Result<(), VaultError> {
        self.calls.set(self.calls.get() + 1);
        let count = self.writes.get() + 1;
        self.writes.set(count);
        if self.fail_at.get() == Some(count) {
            return Err(VaultError::Unavailable);
        }
        self.values.borrow_mut().insert(key.into(), value.into());
        if let Some(path) = self.mutate_after_write.borrow_mut().take() {
            std::fs::write(path, "tasks: []\n").unwrap();
        }
        Ok(())
    }
    fn delete(&self, _: &str, key: &str) -> Result<(), VaultError> {
        self.calls.set(self.calls.get() + 1);
        self.values.borrow_mut().remove(key);
        Ok(())
    }
}
fn profile(root: &Path) -> (ceh_core::profiles::ProfileDraft, String) {
    let editor = ceh_desktop::profile_editor(root, None).unwrap();
    let mut draft = editor.draft;
    draft.label = "Fixture".into();
    draft.province = "Barcelona".into();
    draft.fill_mode = "copy".into();
    draft.nationality = "ALBANIA".into();
    draft.phone = "600000000".into();
    draft.email = "fixture@example.test".into();
    (draft, editor.revision)
}
fn enabled() -> NotificationDraft {
    serde_json::from_value(json!({"telegram":{"enabled":true,"events":["booking"],"recipients":[{"id":"123456","label":"Fixture","type":"private","username":""}],"token":"123456:FAKE_TOKEN"},"webhook":{"enabled":true,"events":["error"],"config":{"url":"https://example.test/secret-endpoint","method":"POST","headers":[{"name":"Authorization","value":"FIXTURE_HEADER_SECRET","secret":true}],"query":[],"body_mode":"standard","body":""}}})).unwrap()
}
#[test]
fn plain_editor_and_plain_save_never_touch_vault() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::new(Memory::default());
    let empty = editor(root.path(), None, &profile(root.path()).1, &vault).unwrap();
    assert!(!empty.draft.telegram.enabled);
    assert_eq!(vault.backend().calls.get(), 0);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    let (draft, revision) = profile(root.path());
    save(root.path(), None, draft, &revision, None, None, &vault).unwrap();
    assert_eq!(vault.backend().calls.get(), 0);
}
#[test]
fn save_roundtrip_keeps_secrets_out_of_yaml_and_unchanged_edit_reuses_refs() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::new(Memory::default());
    let (draft, revision) = profile(root.path());
    let created = save(
        root.path(),
        None,
        draft,
        &revision,
        None,
        Some(&enabled()),
        &vault,
    )
    .unwrap();
    let text = std::fs::read_to_string(root.path().join("tasks.yml")).unwrap();
    for secret in ["FAKE_TOKEN", "secret-endpoint", "FIXTURE_HEADER_SECRET"] {
        assert!(!text.contains(secret));
    }
    let opened = editor(root.path(), Some(&created.id), &created.revision, &vault).unwrap();
    assert_eq!(opened.draft.telegram.token, "123456:FAKE_TOKEN");
    assert_eq!(
        opened.draft.webhook.config.headers[0].value,
        "FIXTURE_HEADER_SECRET"
    );
    let count = vault.backend().writes.get();
    let basic = ceh_desktop::profile_editor(root.path(), Some(&created.id)).unwrap();
    save(
        root.path(),
        Some(&created.id),
        basic.draft,
        &basic.revision,
        None,
        Some(&opened.draft),
        &vault,
    )
    .unwrap();
    assert_eq!(vault.backend().writes.get(), count);
}
#[test]
fn invalid_or_stale_draft_does_not_write_vault() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::new(Memory::default());
    let (mut draft, revision) = profile(root.path());
    draft.phone = "invalid".into();
    assert!(
        save(
            root.path(),
            None,
            draft,
            &revision,
            None,
            Some(&enabled()),
            &vault
        )
        .is_err()
    );
    assert_eq!(vault.backend().calls.get(), 0);
    let (draft, _) = profile(root.path());
    assert!(
        save(
            root.path(),
            None,
            draft,
            "wrong",
            None,
            Some(&enabled()),
            &vault
        )
        .is_err()
    );
    assert_eq!(vault.backend().calls.get(), 0);
    let (mut settings, (draft, revision)) = (enabled(), profile(root.path()));
    settings.telegram.events = vec!["unknown".into()];
    assert!(
        save(
            root.path(),
            None,
            draft,
            &revision,
            None,
            Some(&settings),
            &vault
        )
        .is_err()
    );
    assert_eq!(vault.backend().calls.get(), 0);
}
#[test]
fn second_vault_write_failure_rolls_back_new_refs_without_creating_profile() {
    let root = tempfile::tempdir().unwrap();
    let backend = Memory::default();
    backend.fail_at.set(Some(3));
    let vault = Vault::new(backend);
    let (draft, revision) = profile(root.path());
    assert!(
        save(
            root.path(),
            None,
            draft,
            &revision,
            None,
            Some(&enabled()),
            &vault
        )
        .is_err()
    );
    assert_eq!(vault.backend().writes.get(), 3);
    assert!(vault.backend().values.borrow().is_empty());
    assert!(!root.path().join("tasks.yml").exists());
}
#[test]
fn yaml_conflict_after_secret_prepare_rolls_back_new_refs() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::new(Memory::default());
    *vault.backend().mutate_after_write.borrow_mut() = Some(root.path().join("tasks.yml"));
    let (draft, revision) = profile(root.path());
    assert!(
        save(
            root.path(),
            None,
            draft,
            &revision,
            None,
            Some(&enabled()),
            &vault
        )
        .is_err()
    );
    assert!(vault.backend().values.borrow().is_empty());
    assert_eq!(
        std::fs::read_to_string(root.path().join("tasks.yml")).unwrap(),
        "tasks: []\n"
    );
}
#[test]
fn locked_vault_does_not_clear_saved_credentials() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::new(Memory::default());
    let (draft, revision) = profile(root.path());
    let created = save(
        root.path(),
        None,
        draft,
        &revision,
        None,
        Some(&enabled()),
        &vault,
    )
    .unwrap();
    let before = std::fs::read(root.path().join("tasks.yml")).unwrap();
    let entries = vault.backend().values.borrow().clone();
    vault.backend().locked.set(true);
    // The editor still opens (Python load_errors): the unreadable channels are marked, never silently emptied.
    let opened = editor(root.path(), Some(&created.id), &created.revision, &vault).unwrap();
    assert_eq!(opened.unreadable, ["telegram", "webhook"]);
    assert!(opened.draft.telegram.token.is_empty() && opened.draft.telegram.enabled, "metadata is shown, the secret is not");
    let basic = ceh_desktop::profile_editor(root.path(), Some(&created.id)).unwrap();
    // Enabling an unreadable channel without replacement credentials must fail (Python 0.3.3);
    // supplying credentials, or disabling, is allowed and covered by the tests below.
    let without_credentials: NotificationDraft = serde_json::from_value(json!({
        "telegram":{"enabled":true,"events":["booking"],"recipients":[{"id":"123456","label":"Fixture","type":"private","username":""}]},
        "webhook":{"enabled":true,"events":["error"]}
    }))
    .unwrap();
    assert!(
        save(
            root.path(),
            Some(&created.id),
            basic.draft,
            &basic.revision,
            None,
            Some(&without_credentials),
            &vault
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read(root.path().join("tasks.yml")).unwrap(),
        before
    );
    assert_eq!(*vault.backend().values.borrow(), entries);
}
#[test]
fn notification_draft_cannot_supply_vault_reference() {
    let mut value = serde_json::to_value(enabled()).unwrap();
    value["telegram"]["secret_ref"] = json!("notifications-other-profile");
    assert!(serde_json::from_value::<NotificationDraft>(value).is_err());
}

#[test]
fn existing_locked_credentials_are_untouched_by_plain_edit() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::new(Memory::default());
    let (draft, revision) = profile(root.path());
    let created = save(
        root.path(),
        None,
        draft,
        &revision,
        None,
        Some(&enabled()),
        &vault,
    )
    .unwrap();
    let before = vault.backend().values.borrow().clone();
    vault.backend().locked.set(true);
    let calls = vault.backend().calls.get();
    let mut basic = ceh_desktop::profile_editor(root.path(), Some(&created.id)).unwrap();
    basic.draft.label = "Renamed".into();
    save(
        root.path(),
        Some(&created.id),
        basic.draft,
        &basic.revision,
        None,
        None,
        &vault,
    )
    .unwrap();
    assert_eq!(vault.backend().calls.get(), calls);
    assert_eq!(*vault.backend().values.borrow(), before);
    let raw = ceh_store::legacy_yaml::read_document(&root.path().join("tasks.yml")).unwrap();
    let metadata = ceh_store::notification_metadata::NotificationMetadata::from_task(Some(
        &raw.root().get("tasks").unwrap().sequence().unwrap()[0],
    ))
    .unwrap();
    assert!(!metadata.telegram.channel.secret_ref.is_empty());
}
#[test]
fn missing_secret_reference_fails_without_replacing_yaml() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::new(Memory::default());
    let (draft, revision) = profile(root.path());
    let created = save(
        root.path(),
        None,
        draft,
        &revision,
        None,
        Some(&enabled()),
        &vault,
    )
    .unwrap();
    let before = std::fs::read(root.path().join("tasks.yml")).unwrap();
    vault.backend().values.borrow_mut().clear();
    let opened = editor(root.path(), Some(&created.id), &created.revision, &vault).unwrap();
    assert_eq!(opened.unreadable, ["telegram", "webhook"], "a deleted secret is reported, not fatal");
    assert_eq!(
        std::fs::read(root.path().join("tasks.yml")).unwrap(),
        before
    );
}
#[test]
fn unknown_metadata_and_unknown_secret_values_survive_and_old_refs_remain() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::new(Memory::default());
    let (draft, revision) = profile(root.path());
    let created = save(
        root.path(),
        None,
        draft,
        &revision,
        None,
        Some(&enabled()),
        &vault,
    )
    .unwrap();
    let path = root.path().join("tasks.yml");
    let raw = ceh_store::legacy_yaml::read_document(&path).unwrap();
    let old = ceh_store::notification_metadata::NotificationMetadata::from_task(Some(
        &raw.root().get("tasks").unwrap().sequence().unwrap()[0],
    ))
    .unwrap();
    let replacement = vault
        .put(
            &ceh_services::secrets::SecretData::new(
                json!({"token":"123456:FAKE_TOKEN","future_secret":"FIXTURE_FUTURE"}),
            )
            .unwrap(),
        )
        .unwrap();
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace(&old.telegram.channel.secret_ref, &replacement)
        .replace(
            "\"telegram\": {",
            "\"telegram\": {\"future_setting\": true, ",
        );
    std::fs::write(&path, text).unwrap();
    let basic = ceh_desktop::profile_editor(root.path(), Some(&created.id)).unwrap();
    let mut opened = editor(root.path(), Some(&created.id), &basic.revision, &vault).unwrap();
    opened.draft.telegram.token = "123456:CHANGED_TOKEN".into();
    save(
        root.path(),
        Some(&created.id),
        basic.draft,
        &basic.revision,
        None,
        Some(&opened.draft),
        &vault,
    )
    .unwrap();
    let raw = ceh_store::legacy_yaml::read_document(&path).unwrap();
    let task = &raw.root().get("tasks").unwrap().sequence().unwrap()[0];
    assert!(
        task.get("notifications")
            .unwrap()
            .get("telegram")
            .unwrap()
            .get("future_setting")
            .unwrap()
            .truthy()
    );
    let now =
        ceh_store::notification_metadata::NotificationMetadata::from_task(Some(task)).unwrap();
    let new = vault.get(&now.telegram.channel.secret_ref).unwrap();
    assert_eq!(new.value()["future_secret"], "FIXTURE_FUTURE");
    assert_eq!(new.value()["token"], "123456:CHANGED_TOKEN");
    assert_eq!(
        vault.get(&replacement).unwrap().value()["token"],
        "123456:FAKE_TOKEN"
    );
}

#[test]
fn unrelated_notification_edit_preserves_unknown_recipient_fields() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::new(Memory::default());
    let (draft, revision) = profile(root.path());
    let created = save(
        root.path(),
        None,
        draft,
        &revision,
        None,
        Some(&enabled()),
        &vault,
    )
    .unwrap();
    let path = root.path().join("tasks.yml");
    let text = std::fs::read_to_string(&path).unwrap().replace(
        "\"recipients\": [{",
        "\"recipients\": [{\"future_chat_flag\": true, ",
    );
    std::fs::write(&path, text).unwrap();
    let basic = ceh_desktop::profile_editor(root.path(), Some(&created.id)).unwrap();
    let mut opened = editor(root.path(), Some(&created.id), &basic.revision, &vault).unwrap();
    opened.draft.webhook.enabled = false;
    save(
        root.path(),
        Some(&created.id),
        basic.draft,
        &basic.revision,
        None,
        Some(&opened.draft),
        &vault,
    )
    .unwrap();
    let raw = ceh_store::legacy_yaml::read_document(&path).unwrap();
    let chat = &raw.root().get("tasks").unwrap().sequence().unwrap()[0]
        .get("notifications")
        .unwrap()
        .get("telegram")
        .unwrap()
        .get("recipients")
        .unwrap()
        .sequence()
        .unwrap()[0];
    assert!(chat.get("future_chat_flag").is_some_and(|v| v.truthy()));
}

#[test]
fn disabling_channels_with_a_locked_vault_keeps_their_secret_references() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::new(Memory::default());
    let (draft, revision) = profile(root.path());
    let created = save(root.path(), None, draft, &revision, None, Some(&enabled()), &vault).unwrap();
    let before = std::fs::read_to_string(root.path().join("tasks.yml")).unwrap();
    let references: Vec<String> = before
        .split("notifications-")
        .skip(1)
        .map(|rest| format!("notifications-{}", rest.chars().take_while(char::is_ascii_hexdigit).collect::<String>()))
        .collect();
    assert_eq!(references.len(), 2, "telegram and webhook secrets are referenced: {before}");
    let entries = vault.backend().values.borrow().clone();
    vault.backend().locked.set(true);
    let basic = ceh_desktop::profile_editor(root.path(), Some(&created.id)).unwrap();
    let disabled: NotificationDraft = serde_json::from_value(json!({
        "telegram":{"enabled":false,"events":["booking"],"recipients":[{"id":"123456","label":"Fixture","type":"private","username":""}]},
        "webhook":{"enabled":false,"events":["error"]}
    }))
    .unwrap();
    save(root.path(), Some(&created.id), basic.draft, &basic.revision, None, Some(&disabled), &vault)
        .expect("disabling must not need the unreadable secrets");
    let after = std::fs::read_to_string(root.path().join("tasks.yml")).unwrap();
    for reference in &references {
        assert!(after.contains(reference.as_str()), "reference {reference} survives: {after}");
    }
    assert_eq!(*vault.backend().values.borrow(), entries, "stored secrets are untouched");
}

#[test]
fn replacement_credentials_for_an_unreadable_channel_get_a_new_reference_and_keep_the_old_secret() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::new(Memory::default());
    let (draft, revision) = profile(root.path());
    let created = save(root.path(), None, draft, &revision, None, Some(&enabled()), &vault).unwrap();
    let before = std::fs::read_to_string(root.path().join("tasks.yml")).unwrap();
    let entries = vault.backend().values.borrow().clone();
    vault.backend().locked.set(true);
    let basic = ceh_desktop::profile_editor(root.path(), Some(&created.id)).unwrap();
    let mut replacement = enabled();
    replacement.telegram.token = "123456:REPLACEMENT_TOKEN".into();
    save(root.path(), Some(&created.id), basic.draft, &basic.revision, None, Some(&replacement), &vault)
        .expect("replacement credentials do not need the unreadable old secret");
    let after = std::fs::read_to_string(root.path().join("tasks.yml")).unwrap();
    assert_ne!(after, before, "a new reference is written");
    let kept = vault.backend().values.borrow();
    assert!(entries.iter().all(|(key, value)| kept.get(key) == Some(value)), "the old secret entries stay in the vault");
}

#[test]
fn readable_secrets_report_no_unreadable_channels() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::new(Memory::default());
    let (draft, revision) = profile(root.path());
    let created = save(root.path(), None, draft, &revision, None, Some(&enabled()), &vault).unwrap();
    let opened = editor(root.path(), Some(&created.id), &created.revision, &vault).unwrap();
    assert!(opened.unreadable.is_empty());
    assert_eq!(opened.draft.telegram.token, "123456:FAKE_TOKEN");
}

mod protection {
    use super::*;
    use ceh_core::appointment_budget::AppointmentPolicy;
    use ceh_desktop::notification_editor::save_full;

    #[test]
    fn editor_exposes_defaults_for_a_new_profile_and_the_saved_policy_for_an_existing_one() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(ceh_desktop::profile_editor(root.path(), None).unwrap().protection, AppointmentPolicy::default());
        let vault = Vault::new(Memory::default());
        let (draft, revision) = profile(root.path());
        let policy = AppointmentPolicy { distribute_attempts: true, daily_target: 11, ..Default::default() };
        let created = save_full(root.path(), None, draft, &revision, None, None, Some(&policy), &vault).unwrap();
        assert_eq!(ceh_desktop::profile_editor(root.path(), Some(&created.id)).unwrap().protection, policy);
    }

    #[test]
    fn saving_without_a_policy_keeps_the_saved_one_and_an_invalid_policy_changes_nothing() {
        let root = tempfile::tempdir().unwrap();
        let vault = Vault::new(Memory::default());
        let (draft, revision) = profile(root.path());
        let policy = AppointmentPolicy { minimum_pause_minutes: 20, ..Default::default() };
        let created = save_full(root.path(), None, draft, &revision, None, None, Some(&policy), &vault).unwrap();
        let basic = ceh_desktop::profile_editor(root.path(), Some(&created.id)).unwrap();
        let mut renamed = basic.draft.clone();
        renamed.label = "Renamed".into();
        let saved = save(root.path(), Some(&created.id), renamed, &basic.revision, None, None, &vault).unwrap();
        assert_eq!(ceh_desktop::profile_editor(root.path(), Some(&created.id)).unwrap().protection, policy);
        let before = std::fs::read(root.path().join("tasks.yml")).unwrap();
        let bad = AppointmentPolicy { daily_target: 0, ..Default::default() };
        let basic = ceh_desktop::profile_editor(root.path(), Some(&created.id)).unwrap();
        assert!(save_full(root.path(), Some(&created.id), basic.draft, &saved.revision, None, None, Some(&bad), &vault).is_err());
        assert_eq!(std::fs::read(root.path().join("tasks.yml")).unwrap(), before);
    }
}
