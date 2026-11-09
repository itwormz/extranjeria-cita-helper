fn main() {
    #[cfg(feature = "shell")]
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "search_snapshot",
            "search_start",
            "search_control",
            "appointment_daily_target",
            "appointment_distribution",
            "appointment_try_now",
            "search_show",
            "search_copy",
            "desktop_bootstrap",
            "validate_profile",
            "profile_editor",
            "save_profile",
            "save_preferences",
            "select_profile",
            "load_notification_editor",
            "notification_action",
            "open_diagnostics",
            "profile_statistics",
            "profile_history",
            "profile_selection",
            "appointment_context",
            "save_appointment",
            "copy_appointment_field",
            "achievement_public",
            "achievement_calendar",
            "achievement_export",
            "open_public_link",
        ]),
    ))
    .expect("desktop build configuration");
}
