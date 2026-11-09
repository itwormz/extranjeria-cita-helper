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
) -> Result<ceh_store::profile_write::ProfileSaved, SaveFailure> {
    authorize(&window).map_err(|code| SaveFailure {
        code,
        errors: vec![],
    })?;
    let path = runtime.root.join("tasks.yml");
    let saved = tauri::async_runtime::spawn_blocking(move || {
        ceh_desktop::notification_editor::save(
            path.parent().expect("runtime root"),
            id.as_deref(),
            draft,
            &revision,
            timings.as_ref(),
            notifications.as_ref(),
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
#[tauri::command]
fn appointment_daily_target(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
    target: Option<u32>,
) -> Result<(), &'static str> {
    authorize(&window)?;
    runtime
        .search
        .budget_command(ceh_core::budget_commands::BudgetCommand::SetDailyTarget(target))
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

#[tauri::command]
async fn search_copy(
    window: WebviewWindow,
    runtime: State<'_, DesktopRuntime>,
) -> Result<(), &'static str> {
    authorize(&window)?;
    let state = runtime.search.snapshot()?;
    tauri::async_runtime::spawn_blocking(move || {
        let text =
            serde_json::to_string_pretty(&state.rows).map_err(|_| "clipboard_unavailable")?;
        arboard::Clipboard::new()
            .and_then(|mut c| c.set_text(text))
            .map_err(|_| "clipboard_unavailable")
    })
    .await
    .map_err(|_| "clipboard_unavailable")?
}

fn main() {
    tauri::Builder::default()
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
            } else {
                ceh_desktop::preview_runtime_root(&base, source.as_deref())
                    .map_err(std::io::Error::other)?
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
            });
            let compact =
                cfg!(debug_assertions) && std::env::var_os("CEH_PREVIEW_COMPACT").is_some();
            let window =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .title(if firefox_preview {
                        "Cita Extranjeria Helper — Firefox Preview"
                    } else if ui_review {
                        "Cita Extranjeria Helper — Rust UI Review"
                    } else {
                        "Cita Extranjeria Helper — Rust Preview"
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
            appointment_daily_target,
            appointment_distribution,
            appointment_try_now,
            search_show,
            search_copy,
            desktop_bootstrap,
            validate_profile,
            profile_editor,
            save_profile,
            save_preferences,
            select_profile,
            load_notification_editor,
            notification_action,
            open_diagnostics,
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
        .run(tauri::generate_context!())
        .expect("desktop runtime failed");
}
