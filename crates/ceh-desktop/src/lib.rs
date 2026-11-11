//! Trusted desktop view models; no browser starts or native secrets read here.
pub mod achievement_exports;
pub mod appointment;
pub mod history;
pub mod journal_log;
pub mod notification_worker;
pub mod presentation;
pub mod session_presentation;
pub mod sound;
pub mod update_job;
pub mod window_theme;
use ceh_core::profiles::ProfileCatalog;
use ceh_store::profiles::{DesktopPreferences, read_profiles};
use serde::Serialize;
use serde_json::Value;
use std::{path::Path, sync::LazyLock};
#[derive(Serialize)]
pub struct ProfileSummary {
    pub id: String,
    pub label: String,
    pub procedure: String,
    pub province: String,
    pub valid: bool,
}
#[derive(Serialize)]
pub struct Bootstrap {
    pub version: &'static str,
    pub preview_source: &'static str,
    pub profiles: Vec<ProfileSummary>,
    pub preferences: DesktopPreferences,
    pub selected_profile: Option<String>,
    pub browser_connected: bool,
}
static CATALOG: LazyLock<ProfileCatalog> = LazyLock::new(|| {
    let source: Value =
        serde_json::from_str(include_str!("../../../catalogs/snapshot-2026-08-12.json"))
            .expect("shipped public catalog");
    let options = |key: &str| {
        source["selectors"][key]["options"]
            .as_array()
            .expect("catalog options")
            .iter()
            .filter_map(|v| v["label"].as_str().map(str::to_owned))
            .collect()
    };
    ProfileCatalog {
        provinces: options("provinces"),
        nationalities: options("nationalities"),
        barcelona_offices: options("barcelona_offices"),
        timezones: chrono_tz::TZ_VARIANTS
            .iter()
            .map(ToString::to_string)
            .collect(),
    }
});
pub mod restart;
pub mod session_timeline;
pub mod startup_report;
/// Where the Python application kept its data folders on this system.
pub fn legacy_support_dirs() -> Vec<std::path::PathBuf> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library/Application Support")).into_iter().collect()
    } else if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from).into_iter().collect()
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home.map(|h| h.join(".local/share")))
            .into_iter()
            .collect()
    }
}
/// The installed application's own data folder, next to where the Python version kept its folder.
pub fn application_data_root(support: &std::path::Path) -> std::path::PathBuf {
    support.join("SantaCita")
}
/// `None` only when the system has no per-user data location at all.
pub fn installed_data_root() -> Option<std::path::PathBuf> {
    legacy_support_dirs().first().map(|support| application_data_root(support))
}
/// Portable use (Windows ZIP): a plain `portable.flag` file beside the program keeps all data in `data/` next
/// to it instead of the per-user folder.
pub fn portable_data_root(program_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let flag = program_dir.join("portable.flag");
    std::fs::symlink_metadata(flag)
        .is_ok_and(|meta| meta.file_type().is_file())
        .then(|| program_dir.join("data"))
}
/// Update a portable install in place: unpack the verified ZIP beside the program, swap the program file (the running
/// one is renamed aside), refresh the licence, and remove the unpacked copy and the download. Nothing changes when
/// the package is not a portable release. The caller starts the new program and exits.
pub fn apply_portable_update(zip: &std::path::Path, exe: &std::path::Path) -> Result<(), &'static str> {
    use ceh_services::portable_update::{extract_program, swap_program};
    let directory = exe.parent().ok_or("update_install_failed")?;
    let name = exe.file_name().and_then(|n| n.to_str()).ok_or("update_install_failed")?;
    // Beside the program, so the file swap stays on one volume.
    let staging = directory.join("update-staging");
    let _ = std::fs::remove_dir_all(&staging);
    let result = (|| {
        let new_exe = extract_program(zip, &staging, name).map_err(|_| "update_install_failed")?;
        swap_program(exe, &new_exe).map_err(|_| "update_install_failed")?;
        let _ = std::fs::copy(staging.join("LICENSE.txt"), directory.join("LICENSE.txt"));
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&staging);
    if result.is_ok() {
        if let Some(downloads) = zip.parent() {
            let sum = zip.with_file_name(format!("{}.sha256", zip.file_name().and_then(|n| n.to_str()).unwrap_or("")));
            let _ = std::fs::remove_file(zip);
            let _ = std::fs::remove_file(sum);
            let _ = std::fs::remove_dir(downloads);
        }
    }
    result
}
/// First start of the installed application: when it has no profile list yet and a Python data folder exists
/// beside it, bring that data over (merging, never overwriting; the old folder is only read). Returns the
/// folder name on success; any problem (data in use, unreadable) leaves nothing behind and is retried next start.
pub fn auto_import(
    root: &std::path::Path,
    support_dirs: &[std::path::PathBuf],
    catalog: &ProfileCatalog,
) -> Option<String> {
    if root.join("tasks.yml").symlink_metadata().is_ok() {
        return None;
    }
    let own = root.canonicalize().ok();
    let found = ceh_store::legacy_import::candidates(support_dirs)
        .into_iter()
        .find(|c| own.is_none() || c.path().canonicalize().ok() != own)?;
    let before_existed = root.exists();
    match ceh_store::legacy_import::import(found.path(), root, catalog) {
        Ok(_) => Some(found.name),
        Err(_) => {
            if !before_existed {
                let _ = std::fs::remove_dir(root);
            }
            None
        }
    }
}
/// What a rejected first navigation means: `None` when the person closed the window while the page loaded
/// (a stop, nothing to report), otherwise the error to show. Without the prepared certificate chain the
/// strict HTTPS check rejects the site, which is told apart from other failures.
pub fn navigation_failure(page_open: bool, certificate_cache_deferred: bool) -> Option<&'static str> {
    match (page_open, certificate_cache_deferred) {
        (false, _) => None,
        (true, true) => Some("browser_certificate_cache_failed"),
        (true, false) => Some("browser_navigation_failed"),
    }
}
/// A browser error after the person closed the window is a stop (`finished`), not an error: nothing to report,
/// Start returns. `page_open` is asked only for browser errors.
pub fn closed_window_stop<T>(
    result: Result<T, &'static str>,
    finished: T,
    page_open: impl FnOnce() -> bool,
) -> Result<T, &'static str> {
    match result {
        Err(code) if code.starts_with("browser_") && !page_open() => Ok(finished),
        other => other,
    }
}
/// The certificate window of the browser (a native dialog) blocks reading the page, so the core cannot tell
/// the person in time: five seconds after the click the worker rings the reminder itself, once.
pub fn certificate_reminder_due(clicked_at_ms: Option<u64>, already_rung: bool, now_ms: u64) -> bool {
    !already_rung && clicked_at_ms.is_some_and(|at| now_ms >= at.saturating_add(5_000))
}
/// Python counts a check when the availability request (`Solicitar cita`, POST `acCitar`) is sent; waiting on
/// the applicant form and reloading it is not a check.
pub fn counts_as_check(action: ceh_core::model::ActionKind) -> bool {
    action == ceh_core::model::ActionKind::Request
}
pub fn catalog() -> &'static ProfileCatalog {
    &CATALOG
}
pub fn bootstrap(root: &Path, language: &str) -> Result<Bootstrap, &'static str> {
    let bundle = read_profiles(&root.join("tasks.yml"), catalog(), language)
        .map_err(|_| "profiles_unavailable")?;
    let profiles: Vec<_> = bundle
        .profiles
        .iter()
        .map(|profile| {
            let draft = &profile.validation.normalized;
            ProfileSummary {
                id: draft.id.clone(),
                label: draft.label.clone(),
                procedure: draft.procedure.clone(),
                province: draft.province.clone(),
                valid: profile.validation.errors.is_empty(),
            }
        })
        .collect();
    let selected_profile = bundle
        .last_profile_id
        .filter(|id| profiles.iter().any(|p| &p.id == id))
        .or_else(|| profiles.first().map(|p| p.id.clone()));
    Ok(Bootstrap {
        version: env!("CARGO_PKG_VERSION"),
        preview_source: "isolated",
        profiles,
        preferences: bundle.preferences,
        selected_profile,
        browser_connected: false,
    })
}
/// Both navigation and every command enforce this; no remote capability or caller paths.
pub fn trusted_origin(label: &str, value: &str) -> bool {
    if label != "main" {
        return false;
    }
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    matches!(
        (url.scheme(), url.host_str(), url.port()),
        ("tauri", Some("localhost"), None) | ("http" | "https", Some("tauri.localhost"), None)
    ) || (cfg!(debug_assertions)
        && url.scheme() == "http"
        && url.host_str() == Some("127.0.0.1")
        && url.port() == Some(1420))
}

#[derive(Serialize)]
pub struct ProfileEditor {
    pub draft: ceh_core::profiles::ProfileDraft,
    pub revision: String,
    pub timings: ceh_core::timings::AssistTimings,
    /// Appointment protection of the profile (defaults for a new one).
    pub protection: ceh_core::appointment_budget::AppointmentPolicy,
    pub provinces: Vec<String>,
    pub nationalities: Vec<String>,
}
pub fn profile_editor(root: &Path, id: Option<&str>) -> Result<ProfileEditor, &'static str> {
    use ceh_core::profiles::ProfileDraft;
    let bundle = read_profiles(&root.join("tasks.yml"), catalog(), "ru")
        .map_err(|_| "profiles_unavailable")?;
    let draft = if let Some(id) = id {
        bundle
            .profiles
            .iter()
            .find(|p| p.validation.normalized.id == id)
            .map(|p| p.validation.normalized.clone())
            .ok_or("profile_missing")?
    } else {
        ProfileDraft {
            id: String::new(),
            label: String::new(),
            enabled: true,
            procedure: "huellas".into(),
            province: String::new(),
            office: String::new(),
            document_type: "NIE".into(),
            document_number: String::new(),
            full_name: String::new(),
            nationality: String::new(),
            phone: String::new(),
            email: String::new(),
            access_mode: "certificate".into(),
            clave_method: "movil".into(),
            fill_mode: "manual".into(),
            no_slots_mode: "salir".into(),
            applicant_no_slots_mode: "wait".into(),
            subprocedure: None,
            timezone: bundle
                .raw()
                .root()
                .get("settings")
                .and_then(|v| v.get("timezone"))
                .and_then(|v| v.scalar_text())
                .unwrap_or_else(|| "Europe/Madrid".into()),
            browser_session: "persistent".into(),
            browser: "firefox".into(),
        }
    };
    let timings = if let Some(id) = id {
        bundle
            .profiles
            .iter()
            .find(|p| p.validation.normalized.id == id)
            .and_then(|p| p.timings.clone())
            .ok_or("timings_unavailable")?
    } else {
        Default::default()
    };
    let protection = id
        .and_then(|id| bundle.profiles.iter().find(|p| p.validation.normalized.id == id))
        .and_then(|p| p.appointment_policy.clone())
        .unwrap_or_default();
    Ok(ProfileEditor {
        timings,
        protection,
        draft,
        revision: bundle.revision().to_owned(),
        provinces: catalog().provinces.clone(),
        nationalities: catalog().nationalities.clone(),
    })
}

pub mod notification_editor;

pub mod notification_pairing;

#[cfg(feature = "shell")]
pub mod notification_actions;

/// Fixed preview-owned directory; never takes a caller-provided path or follows its symlink.
pub fn diagnostics_directory(root: &Path) -> Result<std::path::PathBuf, &'static str> {
    // Where the search writes its session folders (`sessions/<id>/timeline.jsonl`, snapshots, screenshots).
    let path = root.join("data").join("diagnostics");
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(&path)
        .map_err(|_| "diagnostics_unavailable")?;
    let metadata = std::fs::symlink_metadata(&path).map_err(|_| "diagnostics_unavailable")?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("diagnostics_unavailable");
    }
    Ok(path)
}

/// Fixed storage and an existing profile only; no caller-controlled paths or migration.
pub fn profile_statistics(
    root: &Path,
    id: &str,
) -> Result<Option<ceh_store::statistics::Dashboard>, &'static str> {
    let now = current_time()?;
    profile_statistics_at(root, id, now)
}
/// Injected clock for deterministic local-day boundaries; the IPC supplies its own clock.
pub fn profile_statistics_at(
    root: &Path,
    id: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Option<ceh_store::statistics::Dashboard>, &'static str> {
    let profiles = bootstrap(root, "ru")?;
    if !profiles.profiles.iter().any(|profile| profile.id == id) {
        return Err("profile_not_found");
    }
    // Notices seen before the database knew their session are matched now. Journals live in the Python
    // debug folder (`<runtime>/debug`) or in this app's diagnostics folder. Best effort: only an existing,
    // migrated database is written, and a failure never blocks the dashboard.
    for journals in [root.join("debug"), root.join("data/diagnostics")] {
        let _ = ceh_store::notices::restore_applicant_notices_file(
            &root.join("data/profile-statistics.sqlite3"),
            id,
            &journals,
        );
    }
    let mut data = ceh_store::statistics::dashboard_file(
        &root.join("data/profile-statistics.sqlite3"),
        id,
        now,
    )
    .map_err(|_| "statistics_unavailable")?;
    let path = root.join("data/achievements.json");
    let records = match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(_) => return Err("statistics_unavailable"),
        Ok(metadata) if !metadata.file_type().is_file() => return Err("statistics_unavailable"),
        Ok(_) => ceh_services::exports::AchievementStore::new(&path)
            .list(id)
            .map_err(|_| "statistics_unavailable")?,
    };
    let appointment_budget = ceh_store::appointment_budget::BudgetStore::load_profile(&root.join("data"), id)
        .ok().flatten().and_then(|mut budget| {
            let loaded = ceh_store::profiles::read_profiles(&root.join("tasks.yml"), catalog(), "ru").ok()?;
            let policy = loaded.profiles.into_iter().find(|p| p.validation.normalized.id == id)?.appointment_policy?;
            serde_json::to_value(budget.summary(now, &policy, false)).ok()
        });
    if data.is_none() && records.is_empty() && appointment_budget.is_none() {
        return Ok(None);
    }
    let bounds = ceh_store::periods::bounds(ceh_store::periods::Period::Today, now)
        .map_err(|_| "statistics_unavailable")?;
    let mut today = 0_u64;
    for record in &records {
        let date = record
            .get("recorded_at")
            .and_then(Value::as_str)
            .and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok())
            .map(|date| date.to_utc())
            .ok_or("statistics_unavailable")?;
        if bounds.start.is_some_and(|start| date >= start)
            && bounds.end.is_some_and(|end| date < end)
        {
            today += 1;
        }
    }
    let data = data.get_or_insert_with(|| ceh_store::statistics::Dashboard {
        all: serde_json::json!({}),
        today: serde_json::json!({}),
        offices: Vec::new(),
    });
    data.all["successes"] = serde_json::json!(records.len());
    data.today["successes"] = serde_json::json!(today);
    if let Some(budget) = appointment_budget {
        data.today["appointment_budget"] = budget;
    }
    Ok(Some(data.clone()))
}

fn budget_profile(
    root: &Path,
    id: &str,
) -> Result<(ceh_core::profiles::ProfileDraft, Option<ceh_core::appointment_budget::AppointmentPolicy>), &'static str> {
    let bundle = read_profiles(&root.join("tasks.yml"), catalog(), "ru").map_err(|_| "profiles_unavailable")?;
    let loaded = bundle
        .profiles
        .into_iter()
        .find(|p| p.validation.normalized.id == id)
        .ok_or("profile_not_found")?;
    Ok((loaded.validation.normalized, loaded.appointment_policy))
}
fn is_lock_contention(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::WouldBlock
}
/// Today's budget of a profile while no search runs (Python `current_daily_budget`). A profile that never
/// searched shows the base budget and nothing is written; a stored file whose day passed is saved reset.
pub fn appointment_budget_view(
    root: &Path,
    id: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Option<ceh_core::appointment_budget::BudgetSummary>, &'static str> {
    let (draft, policy) = budget_profile(root, id)?;
    let Some(policy) = policy else { return Ok(None) };
    let data = root.join("data");
    let stored = ceh_store::appointment_budget::BudgetStore::load_profile(&data, id).map_err(|_| "appointment_budget_unavailable")?;
    let existed = stored.is_some();
    let mut budget = stored.unwrap_or_default();
    let day = |budget: &ceh_core::appointment_budget::AppointmentBudget| serde_json::to_value(budget).ok().map(|v| v["day"].clone());
    let before = day(&budget);
    let summary = budget.summary(now, &policy, false);
    if existed
        && day(&budget) != before
        && let Ok(store) = ceh_store::appointment_budget::BudgetStore::open_saved(&data, &draft)
    {
        let _ = store.save(&budget);
    }
    Ok(Some(summary))
}
/// Set (or clear with `None`) today's manual budget while no search runs.
pub fn set_idle_daily_target(
    root: &Path,
    id: &str,
    target: Option<u32>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<ceh_core::appointment_budget::BudgetSummary, &'static str> {
    if target.is_some_and(|value| value == 0 || value > i32::MAX as u32) {
        return Err("invalid_target");
    }
    let (draft, policy) = budget_profile(root, id)?;
    let policy = policy.ok_or("profile_invalid")?;
    let data = root.join("data");
    let store = ceh_store::appointment_budget::BudgetStore::open_saved(&data, &draft).map_err(|error| {
        if is_lock_contention(&error) { "budget_in_use" } else { "appointment_budget_unavailable" }
    })?;
    let mut budget = store.load().map_err(|_| "appointment_budget_unavailable")?;
    budget.set_daily_target(target, now, &policy).map_err(|_| "invalid_target")?;
    store.save(&budget).map_err(|_| "appointment_budget_unavailable")?;
    store.remember_profile(&data, id).map_err(|_| "appointment_budget_unavailable")?;
    Ok(budget.summary(now, &policy, false))
}

pub fn current_time() -> Result<chrono::DateTime<chrono::Utc>, &'static str> {
    chrono::DateTime::from_timestamp_millis(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|duration| i64::try_from(duration.as_millis()).ok())
            .ok_or("statistics_unavailable")?,
    )
    .ok_or("statistics_unavailable")
}

/// Debug preview accepts named, previously prepared copies, never an arbitrary runtime path.
pub fn preview_runtime_root(
    base: &Path,
    source: Option<&str>,
) -> Result<(std::path::PathBuf, &'static str), &'static str> {
    let Some(source) = source else {
        return Ok((base.join("rust-preview"), "isolated"));
    };
    if !["stable", "historical", "json-only", "experimental"].contains(&source) {
        return Err("preview_copy_invalid");
    }
    let parent = base.canonicalize().map_err(|_| "preview_copy_missing")?;
    let root = parent
        .join("legacy-compat")
        .join(source)
        .join("rust")
        .canonicalize()
        .map_err(|_| "preview_copy_missing")?;
    if !root.starts_with(&parent)
        || !root
            .join("tasks.yml")
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_file())
    {
        return Err("preview_copy_invalid");
    }
    Ok((root, "legacy_copy"))
}

#[cfg(feature = "shell")]
pub mod search;
#[cfg(feature = "shell")]
mod search_diagnostics;
