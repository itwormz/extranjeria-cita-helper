use ceh_core::profiles::{ProfileDraft, ValidationReport, validate_desktop_profile};
use ceh_desktop::{Bootstrap, bootstrap, trusted_origin};
use std::path::PathBuf;
use tauri::{Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
struct DesktopRuntime {
    search: ceh_desktop::search::SearchController,
    root: PathBuf,
    preview_source: &'static str,
    sound: std::sync::Arc<std::sync::Mutex<ceh_desktop::sound::SoundPlayback>>,
    exports: std::sync::Arc<ceh_desktop::achievement_exports::NativeExports>,
    notifications: std::sync::Arc<ceh_desktop::notification_actions::Actions>,
    update: ceh_desktop::update_job::UpdateJob,
}
fn authorize(window: &WebviewWindow) -> Result<(), &'static str> {
    let url = window.url().map_err(|_| "untrusted_window")?;
    if trusted_origin(window.label(), url.as_str()) {
        Ok(())
    } else {
        Err("untrusted_window")
    }
}
#[tauri::command]
fn desktop_bootstrap(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
) -> Result<Bootstrap, &'static str> {
    authorize(&window)?;
    let mut data = bootstrap(&runtime.root, "ru")?;
    data.preview_source = runtime.preview_source;
    // Native window appearance is cosmetic; it must not hide valid profiles on failure.
    let _ = ceh_desktop::window_theme::apply(&window, data.preferences.dark);
    Ok(data)
}
#[tauri::command]
fn validate_profile(
    window: WebviewWindow,
    draft: ProfileDraft,
) -> Result<ValidationReport, &'static str> {
    authorize(&window)?;
    Ok(validate_desktop_profile(draft, ceh_desktop::catalog()))
}
#[tauri::command]
fn profile_editor(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    id: Option<String>,
) -> Result<ceh_desktop::ProfileEditor, &'static str> {
    authorize(&window)?;
    ceh_desktop::profile_editor(&runtime.root, id.as_deref())
}
#[tauri::command]
async fn profile_statistics(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    id: String,
) -> Result<Option<ceh_store::statistics::Dashboard>, &'static str> {
    authorize(&window)?;
    let root = runtime.root.clone();
    tauri::async_runtime::spawn_blocking(move || ceh_desktop::profile_statistics(&root, &id))
        .await
        .map_err(|_| "statistics_unavailable")?
}
#[tauri::command]
async fn profile_history(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    id: String,
    selection: ceh_desktop::history::HistoryRange,
) -> Result<Option<ceh_store::statistics::HistorySnapshot>, &'static str> {
    authorize(&window)?;
    let root = runtime.root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        ceh_desktop::history::profile_history(&root, &id, &selection)
    })
    .await
    .map_err(|_| "statistics_unavailable")?
}
#[tauri::command]
async fn profile_selection(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    id: String,
    selection: ceh_desktop::history::HistoryRange,
    ids: Vec<String>,
) -> Result<Option<serde_json::Value>, &'static str> {
    authorize(&window)?;
    let root = runtime.root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        ceh_desktop::history::profile_selection(&root, &id, &selection, &ids)
    })
    .await
    .map_err(|_| "statistics_unavailable")?
}
#[tauri::command]
async fn appointment_context(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    id: String,
) -> Result<ceh_desktop::appointment::AppointmentContext, &'static str> {
    authorize(&window)?;
    let root = runtime.root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        ceh_desktop::appointment::context_at(&root, &id, ceh_desktop::current_time()?)
    })
    .await
    .map_err(|_| "achievements_unavailable")?
}
#[tauri::command]
async fn save_appointment(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    id: String,
    achievement_id: Option<String>,
    draft: ceh_desktop::appointment::AppointmentDraft,
) -> Result<serde_json::Value, &'static str> {
    authorize(&window)?;
    let root = runtime.root.clone();
    tauri::async_runtime::spawn_blocking(move || match achievement_id {
        Some(key) => ceh_desktop::appointment::update(&root, &id, &key, &draft),
        None => {
            ceh_desktop::appointment::record_at(&root, &id, &draft, ceh_desktop::current_time()?)
        }
    })
    .await
    .map_err(|_| "achievements_unavailable")?
}
#[tauri::command]
async fn copy_appointment_field(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    id: String,
    achievement_id: String,
    field: ceh_desktop::appointment::PrivateField,
) -> Result<(), &'static str> {
    authorize(&window)?;
    let root = runtime.root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        ceh_desktop::appointment::copy_field(&root, &id, &achievement_id, field)
    })
    .await
    .map_err(|_| "clipboard_unavailable")?
}
#[tauri::command]
async fn achievement_public(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    id: String,
    achievement_id: String,
    language: String,
) -> Result<serde_json::Value, &'static str> {
    authorize(&window)?;
    let root = runtime.root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        ceh_desktop::achievement_exports::public_model(&root, &id, &achievement_id, &language)
    })
    .await
    .map_err(|_| "export_failed")?
}
#[tauri::command]
async fn achievement_calendar(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    id: String,
    achievement_id: String,
) -> Result<serde_json::Value, &'static str> {
    authorize(&window)?;
    let root = runtime.root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        ceh_desktop::achievement_exports::calendar_model(&root, &id, &achievement_id)
    })
    .await
    .map_err(|_| "export_failed")?
}
#[tauri::command]
async fn achievement_export(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    id: String,
    achievement_id: String,
    language: String,
    request: ceh_desktop::achievement_exports::ExportAction,
) -> Result<serde_json::Value, &'static str> {
    authorize(&window)?;
    let root = runtime.root.clone();
    let actions = runtime.exports.clone();
    tauri::async_runtime::spawn_blocking(move || {
        actions.execute(&window, &root, &id, &achievement_id, &language, request)
    })
    .await
    .map_err(|_| "export_failed")?
}
#[derive(serde::Serialize)]
struct UpdateStatus {
    current: &'static str,
    latest: Option<String>,
    available: bool,
}
/// Check the selected update channel; no package is installed automatically.
#[tauri::command]
async fn check_updates(window: WebviewWindow,runtime:State<'_,DesktopRuntime>)->Result<UpdateStatus,&'static str>{
    authorize(&window)?;
    let root=runtime.root.clone();let current=env!("CARGO_PKG_VERSION");
    let latest=tauri::async_runtime::spawn_blocking(move||{
        let include_beta=ceh_desktop::bootstrap(&root,"ru")?.preferences.beta_updates;
        ceh_services::updates::fetch_latest_for(include_beta).map_err(|_|"updates_unavailable")
    }).await.map_err(|_|"updates_unavailable")??;
    Ok(UpdateStatus{current,available:latest.as_ref().is_some_and(|latest|ceh_services::updates::newer(&latest.version,current)),latest:latest.map(|latest|latest.version)})
}
/// Start downloading and verifying the latest release's package (progress through `update_status`).
#[tauri::command]
fn update_download(window: WebviewWindow, runtime: State<'_, DesktopRuntime>) -> Result<(), &'static str> {
    authorize(&window)?;
    let dir = runtime.root.join("data").join("updates");
    let include_beta=ceh_desktop::bootstrap(&runtime.root,"ru")?.preferences.beta_updates;
    if runtime.update.start_channel(env!("CARGO_PKG_VERSION"), dir,include_beta) { Ok(()) } else { Err("update_busy") }
}
#[tauri::command]
fn update_status(window: WebviewWindow, runtime: State<'_, DesktopRuntime>) -> Result<ceh_desktop::update_job::Status, &'static str> {
    authorize(&window)?;
    Ok(runtime.update.status())
}
#[tauri::command]
fn update_cancel(window: WebviewWindow, runtime: State<'_, DesktopRuntime>) -> Result<(), &'static str> {
    authorize(&window)?;
    runtime.update.cancel();
    Ok(())
}
/// Open the verified package (disk image, installer or AppImage); the person finishes the installation. A portable
/// install is updated in place instead: the program file is swapped and the program restarts.
#[tauri::command]
async fn update_install(window: WebviewWindow, runtime: State<'_, DesktopRuntime>) -> Result<(), &'static str> {
    authorize(&window)?;
    let path = runtime.update.install_path().ok_or("update_not_ready")?;
    let include_beta=ceh_desktop::bootstrap(&runtime.root,"ru")?.preferences.beta_updates;
    let to_verify=path.clone();
    tauri::async_runtime::spawn_blocking(move||ceh_services::updates::verify_download(&to_verify,include_beta))
        .await.map_err(|_|"update_install_failed")?.map_err(|_|"update_install_failed")?;
    if ceh_services::updates::is_portable_install() {
        if runtime.search.snapshot()?.active {
            return Err("search_already_running");
        }
        let exe = std::env::current_exe().map_err(|_| "update_install_failed")?;
        let target = exe.clone();
        tauri::async_runtime::spawn_blocking(move || ceh_desktop::apply_portable_update(&path, &target))
            .await
            .map_err(|_| "update_install_failed")??;
        // The new program waits a moment (CEH_AFTER_UPDATE) so this one is gone before it starts.
        std::process::Command::new(&exe)
            .env("CEH_AFTER_UPDATE", "1")
            .spawn()
            .map_err(|_| "update_install_failed")?;
        window.app_handle().exit(0);
        return Ok(());
    }
    tauri::async_runtime::spawn_blocking(move || ceh_desktop::notification_actions::open_installer(&path))
        .await
        .map_err(|_| "update_install_failed")?
        .map_err(|_| "update_install_failed")
}
#[tauri::command]
async fn open_public_link(
    window: WebviewWindow,
    target: ceh_desktop::achievement_exports::PublicLink,
    language: String,
) -> Result<(), &'static str> {
    authorize(&window)?;
    let url = ceh_desktop::achievement_exports::public_link(target, &language)?;
    tauri::async_runtime::spawn_blocking(move || ceh_desktop::notification_actions::open_url(&url))
        .await
        .map_err(|_| "open_failed")?
}
#[derive(serde::Serialize)]
struct SaveFailure {
    code: &'static str,
    errors: Vec<String>,
}
#[tauri::command]
async fn save_profile(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    id: Option<String>,
    draft: ProfileDraft,
    revision: String,
    timings: Option<serde_json::Value>,
    notifications: Option<ceh_desktop::notification_editor::NotificationDraft>,
    protection: Option<ceh_core::appointment_budget::AppointmentPolicy>,
) -> Result<ceh_store::profile_write::ProfileSaved, SaveFailure> {
    authorize(&window).map_err(|code| SaveFailure {
        code,
        errors: vec![],
    })?;
    let path = runtime.root.join("tasks.yml");
    let saved = tauri::async_runtime::spawn_blocking(move || {
        ceh_desktop::notification_editor::save_full(
            path.parent().expect("runtime root"),
            id.as_deref(),
            draft,
            &revision,
            timings.as_ref(),
            notifications.as_ref(),
            protection.as_ref(),
            &ceh_services::secrets::Vault::new(ceh_services::secrets::native::NativeBackend::new()),
        )
    })
    .await
    .map_err(|_| SaveFailure {
        code: "save_failed",
        errors: vec![],
    })?
    .map_err(edit_failure)?;
    // A running search applies saved timings and protection to itself; with no search this is a no-op.
    let _ = runtime.search.refresh_settings(&runtime.root);
    Ok(saved)
}
fn edit_failure(error: ceh_desktop::notification_editor::EditError) -> SaveFailure {
    use ceh_desktop::notification_editor::EditError;
    use ceh_services::secrets::VaultError;
    use ceh_store::profile_write::ProfileWriteError;
    match error {
        EditError::Profile(ProfileWriteError::Conflict) => SaveFailure {
            code: "conflict",
            errors: vec![],
        },
        EditError::Profile(ProfileWriteError::Validation(errors)) => SaveFailure {
            code: "validation",
            errors,
        },
        EditError::Secret(VaultError::Locked) => SaveFailure {
            code: "secrets_locked",
            errors: vec![],
        },
        EditError::Secret(VaultError::Missing) => SaveFailure {
            code: "secrets_missing",
            errors: vec![],
        },
        EditError::Secret(_) => SaveFailure {
            code: "secrets_unavailable",
            errors: vec![],
        },
        EditError::Invalid => SaveFailure {
            code: "validation",
            errors: vec!["Invalid notification settings".into()],
        },
        _ => SaveFailure {
            code: "save_failed",
            errors: vec![],
        },
    }
}
#[tauri::command]
async fn load_notification_editor(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    id: Option<String>,
    revision: String,
) -> Result<ceh_desktop::notification_editor::NotificationEditor, SaveFailure> {
    authorize(&window).map_err(|code| SaveFailure {
        code,
        errors: vec![],
    })?;
    let root = runtime.root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        ceh_desktop::notification_editor::editor(
            &root,
            id.as_deref(),
            &revision,
            &ceh_services::secrets::Vault::new(ceh_services::secrets::native::NativeBackend::new()),
        )
    })
    .await
    .map_err(|_| SaveFailure {
        code: "secrets_unavailable",
        errors: vec![],
    })?
    .map_err(edit_failure)
}

#[tauri::command]
async fn notification_action(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    request: ceh_desktop::notification_actions::Action,
) -> Result<serde_json::Value, ceh_desktop::notification_actions::ActionError> {
    authorize(&window).map_err(|_| ceh_desktop::notification_actions::ActionError {
        code: "untrusted_window",
        status: None,
    })?;
    let actions = runtime.notifications.clone();
    tauri::async_runtime::spawn_blocking(move || actions.execute(request))
        .await
        .map_err(|_| ceh_desktop::notification_actions::ActionError {
            code: "unavailable",
            status: None,
        })?
}
#[tauri::command]
async fn open_diagnostics(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
) -> Result<(), &'static str> {
    authorize(&window)?;
    let root = runtime.root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = ceh_desktop::diagnostics_directory(&root)?;
        ceh_desktop::notification_actions::open_directory(&path)
            .map_err(|_| "diagnostics_unavailable")
    })
    .await
    .map_err(|_| "diagnostics_unavailable")?
}
#[tauri::command]
async fn save_preferences(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    preferences: ceh_store::profiles::DesktopPreferences,
) -> Result<(), &'static str> {
    authorize(&window)?;
    let channel_changed=ceh_desktop::bootstrap(&runtime.root,"ru")?.preferences.beta_updates!=preferences.beta_updates;
    if channel_changed && runtime.update.status().phase==ceh_desktop::update_job::Phase::Downloading{return Err("update_busy");}
    let dark = preferences.dark;
    let sound_enabled = preferences.sound;
    let player = runtime.sound.clone();
    let path = runtime.root.join("tasks.yml");
    tauri::async_runtime::spawn_blocking(move || {
        ceh_store::profile_write::save_preferences(&path, &preferences, ceh_desktop::catalog())
    })
    .await
    .map_err(|_| "preferences_failed")?
    .map_err(|_| "preferences_failed")?;
    if channel_changed{runtime.update.clear_ready();}
    if !sound_enabled && let Ok(mut playback) = player.lock() {
        let _ = playback.stop();
    }
    // A cosmetic OS failure cannot turn an already committed write into a false failure.
    let _ = ceh_desktop::window_theme::apply(&window, dark);
    Ok(())
}

#[tauri::command]
async fn select_profile(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    id: String,
) -> Result<(), &'static str> {
    authorize(&window)?;
    let path = runtime.root.join("tasks.yml");
    tauri::async_runtime::spawn_blocking(move || {
        ceh_store::profile_write::save_selection(&path, &id, ceh_desktop::catalog())
    })
    .await
    .map_err(|_| "profile_selection_failed")?
    .map_err(|_| "profile_selection_failed")
}
#[tauri::command]
fn search_snapshot(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
) -> Result<ceh_desktop::search::SearchSnapshot, &'static str> {
    authorize(&window)?;
    runtime.search.snapshot()
}
#[tauri::command]
fn search_start(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    id: String,
) -> Result<(), &'static str> {
    authorize(&window)?;
    runtime
        .search
        .start(&runtime.root, &id, window, runtime.sound.clone())
}
#[tauri::command]
fn search_control(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    command: ceh_desktop::search::Control,
) -> Result<(), &'static str> {
    authorize(&window)?;
    let result = runtime.search.control(command);
    if result.is_ok()
        && let Ok(mut playback) = runtime.sound.lock()
    {
        let _ = playback.stop();
    }
    result
}
/// Installed browsers and whether the app can drive them; never exposes install paths.
#[tauri::command]
fn installed_browsers(window: WebviewWindow) -> Result<serde_json::Value, &'static str> {
    authorize(&window)?;
    // Preview of a machine without a compatible browser (`CEH_PREVIEW_NO_BROWSERS=1`): only browsers the app cannot drive.
    if std::env::var_os("CEH_PREVIEW_NO_BROWSERS").is_some() {
        return Ok(serde_json::json!([
            {"id":"edge","name":"Microsoft Edge","version":"142.0","support":"planned"},
            {"id":"safari","name":"Safari","version":"","support":"unsupported"}
        ]));
    }
    let list = ceh_browser::discovery::discover_installed();
    Ok(serde_json::json!(list
        .iter()
        .map(|b| serde_json::json!({"id":b.family.id(),"name":b.name,"version":b.version,"support":b.support.id()}))
        .collect::<Vec<_>>()))
}
/// The profile's earlier work log (rows of its previous searches), oldest first.
#[tauri::command]
fn journal_history(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    id: String,
) -> Result<Vec<serde_json::Value>, &'static str> {
    authorize(&window)?;
    Ok(ceh_desktop::journal_log::load_history(&runtime.root, &id, 250))
}
#[tauri::command]
fn appointment_budget(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    id: String,
) -> Result<Option<ceh_core::appointment_budget::BudgetSummary>, &'static str> {
    authorize(&window)?;
    let snapshot = runtime.search.snapshot()?;
    if snapshot.active && snapshot.profile.as_deref() == Some(id.as_str()) {
        return Ok(snapshot.appointment_budget);
    }
    ceh_desktop::appointment_budget_view(&runtime.root, &id, ceh_desktop::current_time()?)
}
/// Today's manual budget: a running search of this profile applies it at its next tick, otherwise the
/// profile's stored budget is edited directly.
#[tauri::command]
fn appointment_daily_target(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    id: Option<String>,
    target: Option<u32>,
) -> Result<(), &'static str> {
    authorize(&window)?;
    let snapshot = runtime.search.snapshot()?;
    match id {
        Some(id) if !(snapshot.active && snapshot.profile.as_deref() == Some(id.as_str())) => {
            ceh_desktop::set_idle_daily_target(&runtime.root, &id, target, ceh_desktop::current_time()?).map(|_| ())
        }
        _ => runtime
            .search
            .budget_command(ceh_core::budget_commands::BudgetCommand::SetDailyTarget(target)),
    }
}
#[tauri::command]
fn appointment_distribution(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    disabled: bool,
) -> Result<(), &'static str> {
    authorize(&window)?;
    runtime
        .search
        .budget_command(ceh_core::budget_commands::BudgetCommand::SetDistribution { disabled })
}
#[tauri::command]
fn appointment_try_now(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
) -> Result<(), &'static str> {
    authorize(&window)?;
    runtime
        .search
        .budget_command(ceh_core::budget_commands::BudgetCommand::TryNow)
}
#[tauri::command]
fn search_resume(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
) -> Result<(), &'static str> {
    authorize(&window)?;
    runtime.search.resume_search()
}
#[tauri::command]
fn search_show(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
) -> Result<(), &'static str> {
    authorize(&window)?;
    let pid = runtime
        .search
        .snapshot()?
        .browser_pid
        .ok_or("browser_not_open")?;
    ceh_desktop::window_theme::show_firefox(&window,pid)
}

/// Put text the window composed (the journal table as shown) on the clipboard; bounded like any IPC text.
#[tauri::command]
async fn copy_text(
    window: WebviewWindow,
    text: String,
) -> Result<(), &'static str> {
    authorize(&window)?;
    if text.is_empty() || text.len() > 1024 * 1024 {
        return Err("clipboard_unavailable");
    }
    tauri::async_runtime::spawn_blocking(move || {
        arboard::Clipboard::new()
            .and_then(|mut c| c.set_text(text))
            .map_err(|_| "clipboard_unavailable")
    })
    .await
    .map_err(|_| "clipboard_unavailable")?
}

/// Reset the browser session of profile `id` (Python `reset_session`): a running search of this profile
/// restarts its pass, otherwise the app-owned browser profile is deleted. Returns what was done.
#[tauri::command]
fn reset_session(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    id: String,
) -> Result<&'static str, &'static str> {
    authorize(&window)?;
    let snapshot = runtime.search.snapshot()?;
    if snapshot.active {
        if snapshot.profile.as_deref() != Some(id.as_str()) {
            return Err("search_already_running");
        }
        if snapshot.manual {
            return Err("search_manual");
        }
        runtime.search.restart()?;
        return Ok("restarted");
    }
    // The profile may have been used with any of the browsers it could choose; reset each one's own copy.
    for kind in [ceh_browser::firefox::ProfileKind::Firefox, ceh_browser::firefox::ProfileKind::Chrome, ceh_browser::firefox::ProfileKind::Brave, ceh_browser::firefox::ProfileKind::Edge, ceh_browser::firefox::ProfileKind::Chromium] {
        ceh_browser::firefox::ProfileLease::reset_persistent_kind(&runtime.root, &id, kind).map_err(|error| match error {
            ceh_browser::firefox::FirefoxError::ProfileBusy => "browser_profile_busy",
            _ => "browser_profile_unavailable",
        })?;
    }
    Ok("reset")
}
/// Python data folders that can be imported; the window picks one by name, never by path.
#[tauri::command]
fn legacy_candidates(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
) -> Result<Vec<String>, &'static str> {
    authorize(&window)?;
    let own = runtime.root.canonicalize().ok();
    Ok(ceh_store::legacy_import::candidates(&ceh_desktop::legacy_support_dirs())
        .into_iter()
        // This app's own data folder is never offered as a source of itself.
        .filter(|c| c.path().canonicalize().ok() != own)
        .map(|c| c.name)
        .collect())
}
/// Copy the named Python data folder into this runtime without overwriting anything.
#[tauri::command]
async fn legacy_import(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    name: String,
) -> Result<serde_json::Value, &'static str> {
    authorize(&window)?;
    if runtime.search.snapshot()?.active {
        return Err("search_already_running");
    }
    let root = runtime.root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let own = root.canonicalize().ok();
        let found = ceh_store::legacy_import::candidates(&ceh_desktop::legacy_support_dirs())
            .into_iter()
            .filter(|c| c.path().canonicalize().ok() != own)
            .find(|c| c.name == name)
            .ok_or("legacy_not_found")?;
        let report = ceh_store::legacy_import::import(found.path(), &root, ceh_desktop::catalog()).map_err(|error| match error {
            ceh_store::legacy_import::ImportError::InUse => "legacy_in_use",
            ceh_store::legacy_import::ImportError::NoProfiles => "legacy_not_found",
            _ => "legacy_import_failed",
        })?;
        Ok(serde_json::json!({"copied": report.copied.len(), "kept": report.kept.len()}))
    })
    .await
    .map_err(|_| "legacy_import_failed")?
}
/// Copy the sanitized startup report with `id` (the id comes from the search snapshot).
#[tauri::command]
async fn startup_report_copy(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    id: String,
) -> Result<(), &'static str> {
    authorize(&window)?;
    let root = runtime.root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let home = std::env::var("HOME").unwrap_or_default();
        let text = ceh_desktop::startup_report::clipboard_text(&root, &home, &id)?;
        arboard::Clipboard::new()
            .and_then(|mut c| c.set_text(text))
            .map_err(|_| "clipboard_unavailable")
    })
    .await
    .map_err(|_| "clipboard_unavailable")?
}

fn main() {
    // Just updated: the previous program is still shutting down; wait for it, and do not pass the marker on.
    if std::env::var_os("CEH_AFTER_UPDATE").is_some() {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        // SAFETY: nothing else runs yet; this only removes a marker for the child processes started later.
        unsafe { std::env::remove_var("CEH_AFTER_UPDATE") };
    }
    // The whole update cycle without a release: swap the program for the zip, start the new one, leave.
    let arguments: Vec<String> = std::env::args().collect();
    if let Some(zip) = ceh_desktop::local_update_request(
        &arguments,
        std::env::var_os("CEH_ALLOW_LOCAL_UPDATE").is_some(),
        ceh_services::updates::is_portable_install(),
    ) {
        let outcome = std::env::current_exe().map_err(|_| "update_install_failed").and_then(|exe| {
            ceh_desktop::apply_portable_update(&zip, &exe)?;
            std::process::Command::new(&exe).env("CEH_AFTER_UPDATE", "1").spawn().map_err(|_| "update_install_failed")?;
            Ok(())
        });
        std::process::exit(if outcome.is_ok() { 0 } else { 3 });
    }
    // A portable install removes what the previous update left beside it.
    if ceh_services::updates::is_portable_install()
        && let Ok(exe) = std::env::current_exe()
    {
        ceh_services::portable_update::remove_old(&exe);
        if let Some(directory) = exe.parent() {
            let _ = std::fs::remove_dir_all(directory.join("update-staging"));
        }
    }
    tauri::Builder::default()
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                use std::sync::atomic::{AtomicBool, Ordering};
                static CLOSING: AtomicBool = AtomicBool::new(false);
                let runtime = window.state::<DesktopRuntime>();
                if runtime.search.shutdown() == ceh_desktop::search::ClosePlan::CloseNow {
                    return;
                }
                // The search thread ends first (its browser closed or kept); then the window goes.
                api.prevent_close();
                if !CLOSING.swap(true, Ordering::AcqRel) {
                    let window = window.clone();
                    std::thread::spawn(move || {
                        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                        let runtime = window.state::<DesktopRuntime>();
                        while std::time::Instant::now() < deadline
                            && runtime.search.snapshot().map(|s| s.active).unwrap_or(false)
                        {
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        }
                        if window.destroy().is_err() {
                            CLOSING.store(false, Ordering::Release);
                        }
                    });
                }
            }
        })
        .setup(|app| {
            let base =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.local/builds/runtime");
            let source = if cfg!(debug_assertions) {
                std::env::var("CEH_PREVIEW_LEGACY_COPY").ok()
            } else {
                None
            };
            let ui_review =
                cfg!(debug_assertions) && std::env::var_os("CEH_PREVIEW_UI_REVIEW").is_some();
            let firefox_preview =
                cfg!(debug_assertions) && std::env::var_os("CEH_PREVIEW_FIREFOX").is_some();
            let (root, preview_source) = if firefox_preview {
                let runtime = match std::env::var("CEH_PREVIEW_FIREFOX").as_deref() {
                    Ok("manual") => "rust-firefox-manual",
                    Ok("applicant") => "rust-firefox-applicant",
                    _ => "rust-firefox",
                };
                (base.join(runtime), "legacy_copy")
            } else if ui_review {
                (base.join("rust-ui-review"), "isolated")
            } else if let Some(directory) = std::env::var_os("CEH_DATA_DIR") {
                // A developer-chosen folder (never a window-supplied path).
                (PathBuf::from(directory), "isolated")
            } else if cfg!(debug_assertions) {
                ceh_desktop::preview_runtime_root(&base, source.as_deref())
                    .map_err(std::io::Error::other)?
            } else {
                let portable = cfg!(windows)
                    .then(|| std::env::current_exe().ok())
                    .flatten()
                    .and_then(|exe| exe.parent().and_then(ceh_desktop::portable_data_root));
                let root = portable
                    .or_else(ceh_desktop::installed_data_root)
                    .ok_or_else(|| std::io::Error::other("data_folder_unavailable"))?;
                std::fs::create_dir_all(&root)?;
                // Transparent first-run import of the Python version's data (no question asked).
                let _ = ceh_desktop::auto_import(&root, &ceh_desktop::legacy_support_dirs(), ceh_desktop::catalog());
                (root, "installed")
            };
            let dark = bootstrap(&root, "ru")
                .map(|data| data.preferences.dark)
                .unwrap_or(true);
            app.manage(DesktopRuntime {
                search: Default::default(),
                root,
                preview_source,
                sound: std::sync::Arc::new(Default::default()),
                exports: std::sync::Arc::new(Default::default()),
                notifications: std::sync::Arc::new(Default::default()),
                update: Default::default(),
            });
            let compact =
                cfg!(debug_assertions) && std::env::var_os("CEH_PREVIEW_COMPACT").is_some();
            let window =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .title(if firefox_preview {
                        "SantaCita — Firefox Preview"
                    } else if ui_review {
                        "SantaCita — Rust UI Review"
                    } else {
                        "SantaCita"
                    })
                    .theme(Some(if dark {
                        tauri::Theme::Dark
                    } else {
                        tauri::Theme::Light
                    }))
                    .inner_size(
                        if compact { 640.0 } else { 1120.0 },
                        if compact { 620.0 } else { 850.0 },
                    )
                    .min_inner_size(640.0, 620.0)
                    .on_navigation(|url| trusted_origin("main", url.as_str()))
                    .build()?;
            let _ = ceh_desktop::window_theme::apply(&window, dark);
            let _ = ceh_desktop::presentation::apply_icon(
                &window,
                ceh_desktop::presentation::IconStatus::Idle,
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            search_snapshot,
            search_start,
            search_control,
            search_resume,
            appointment_budget,
            journal_history,
            installed_browsers,
            appointment_daily_target,
            appointment_distribution,
            appointment_try_now,
            search_show,
            copy_text,
            startup_report_copy,
            reset_session,
            legacy_candidates,
            legacy_import,
            desktop_bootstrap,
            validate_profile,
            profile_editor,
            save_profile,
            save_preferences,
            select_profile,
            load_notification_editor,
            notification_action,
            open_diagnostics,
            check_updates,
            update_download,
            update_status,
            update_cancel,
            update_install,
            profile_statistics,
            profile_history,
            profile_selection,
            appointment_context,
            save_appointment,
            copy_appointment_field,
            achievement_public,
            achievement_calendar,
            achievement_export,
            open_public_link
        ])
        .build(tauri::generate_context!())
        .expect("desktop runtime failed")
        .run(|app, event| {
            // Cmd+Q / Quit bypasses the window's close request: stop an automatic search the same way.
            if let tauri::RunEvent::ExitRequested { api, code: None, .. } = event {
                use std::sync::atomic::{AtomicBool, Ordering};
                static QUITTING: AtomicBool = AtomicBool::new(false);
                let runtime = app.state::<DesktopRuntime>();
                if runtime.search.shutdown() == ceh_desktop::search::ClosePlan::CloseNow {
                    return;
                }
                api.prevent_exit();
                if !QUITTING.swap(true, Ordering::AcqRel) {
                    let app = app.clone();
                    std::thread::spawn(move || {
                        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                        let runtime = app.state::<DesktopRuntime>();
                        while std::time::Instant::now() < deadline
                            && runtime.search.snapshot().map(|s| s.active).unwrap_or(false)
                        {
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        }
                        app.exit(0);
                    });
                }
            }
        });
}
