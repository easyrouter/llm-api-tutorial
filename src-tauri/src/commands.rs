//! Tauri command layer — thin, argument-validating wrappers around the domain modules.
//! Command names and argument shapes are mirrored in `src/lib/tauri.ts`.
//!
//! Conventions
//! - Every command returns `AppResult<T>`; `AppError` serialises to `{code,message,params}`.
//! - Commands never block the async runtime: CPU-light work runs inline, anything that spawns
//!   processes or does I/O is `async`.
//! - No command accepts a path outside the app's own cache/config dirs except
//!   `save_diagnostic_report`, whose path comes from the native save dialog.

use tauri::{AppHandle, Manager, State};

use crate::checks::{self, CheckContext};
use crate::diagnose::{self, report};
use crate::docs::{self, DocsContext};
use crate::error::{AppError, AppResult};
use crate::guide;
use crate::install;
use crate::models::{
    AppConfig, AppInfo, CcSwitchRelease, CheckId, CheckResult, ConfigGuide, DiagnoseRequest,
    Diagnosis, DiagnosticReport, DocPage, DocsIndex, DownloadRequest, DownloadResult, EnvSnapshot,
    InstallJob, InstallPlan, InstallTarget, KeyValidation, MirrorChoice, TelemetryEvent,
    TelemetryStatus, TerminalProcess, ToolId, UrlPreview, VerifyRequest, VerifyResult,
};
use crate::state::AppState;
use crate::verify;

fn app_info(app: &AppHandle, state: &AppState) -> AppInfo {
    let cfg = state.config_snapshot();
    AppInfo {
        name: app.package_info().name.clone(),
        version: app.package_info().version.to_string(),
        platform: crate::platform::platform(),
        arch: std::env::consts::ARCH.to_owned(),
        locale_hint: sys_locale_hint(),
        config_source: cfg.source,
        log_dir: app
            .path()
            .app_log_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
    }
}

/// `zh-CN` when the OS UI language looks Chinese, otherwise `en`. The frontend may override.
fn sys_locale_hint() -> String {
    let raw = std::env::var("LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    if raw.starts_with("zh") {
        "zh-CN".into()
    } else if raw.is_empty() {
        // Windows has no LANG; the frontend falls back to navigator.language.
        String::new()
    } else {
        "en".into()
    }
}

// ---------------------------------------------------------------------------
// app / config
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_app_info(app: AppHandle, state: State<'_, AppState>) -> AppResult<AppInfo> {
    Ok(app_info(&app, &state))
}

#[tauri::command]
pub fn get_app_config(state: State<'_, AppState>) -> AppResult<AppConfig> {
    Ok(state.config_snapshot().config)
}

/// Opens an `http(s)` URL in the default browser. Other schemes are rejected.
#[tauri::command]
pub fn open_external(app: AppHandle, url: String) -> AppResult<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(AppError::InvalidInput(
            "only http(s) URLs may be opened".into(),
        ));
    }
    tauri_plugin_opener::OpenerExt::opener(&app)
        .open_url(url, None::<&str>)
        .map_err(|e| AppError::Other(e.to_string()))
}

// ---------------------------------------------------------------------------
// M1 — checks
// ---------------------------------------------------------------------------

fn check_ctx(state: &AppState) -> CheckContext {
    CheckContext {
        config: state.config_snapshot().config,
        http: state.http.clone(),
    }
}

#[tauri::command]
pub async fn run_env_checks(app: AppHandle, state: State<'_, AppState>) -> AppResult<EnvSnapshot> {
    let ctx = check_ctx(&state);
    Ok(checks::run_all(Some(&app), &ctx).await)
}

#[tauri::command]
pub async fn run_env_check(id: CheckId, state: State<'_, AppState>) -> AppResult<CheckResult> {
    let ctx = check_ctx(&state);
    Ok(checks::run_one(id, &ctx).await)
}

#[tauri::command]
pub async fn probe_mirrors(state: State<'_, AppState>) -> AppResult<MirrorChoice> {
    let cfg = state.config_snapshot().config;
    Ok(crate::net::choose_mirrors(&state.http, &cfg.mirrors).await)
}

// ---------------------------------------------------------------------------
// M2 — install
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn plan_install(
    target: InstallTarget,
    state: State<'_, AppState>,
) -> AppResult<InstallPlan> {
    let cfg = state.config_snapshot().config;
    let mirrors = crate::net::choose_mirrors(&state.http, &cfg.mirrors).await;
    install::plan(target, &cfg, &mirrors).await
}

#[tauri::command]
pub async fn start_install(
    app: AppHandle,
    plan: InstallPlan,
    state: State<'_, AppState>,
) -> AppResult<InstallJob> {
    install::start(app, &state.jobs, plan).await
}

#[tauri::command]
pub fn cancel_install(job_id: String, state: State<'_, AppState>) -> AppResult<()> {
    state.jobs.cancel(&job_id)
}

#[tauri::command]
pub async fn fetch_cc_switch_release(state: State<'_, AppState>) -> AppResult<CcSwitchRelease> {
    let cfg = state.config_snapshot().config;
    install::latest_cc_switch_release(
        &state.http,
        &cfg,
        crate::platform::platform(),
        std::env::consts::ARCH,
    )
    .await
}

#[tauri::command]
pub async fn download_file(
    app: AppHandle,
    request: DownloadRequest,
    state: State<'_, AppState>,
) -> AppResult<DownloadResult> {
    if !request.url.starts_with("https://") {
        return Err(AppError::InvalidInput("downloads must use https".into()));
    }
    install::download_installer(app, &state.http, request).await
}

/// Opens a file previously downloaded into the app cache directory (installer hand-off).
#[tauri::command]
pub fn open_downloaded_file(app: AppHandle, path: String) -> AppResult<()> {
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|e| AppError::Other(e.to_string()))?;
    let target = std::path::Path::new(&path);
    if !target.starts_with(&cache) {
        return Err(AppError::InvalidInput(
            "path is outside the app cache directory".into(),
        ));
    }
    tauri_plugin_opener::OpenerExt::opener(&app)
        .open_path(path, None::<&str>)
        .map_err(|e| AppError::Other(e.to_string()))
}

// ---------------------------------------------------------------------------
// M3 — guide
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_config_guide(tool: ToolId, state: State<'_, AppState>) -> AppResult<ConfigGuide> {
    Ok(guide::build_guide(tool, &state.config_snapshot().config))
}

/// Validates key *format* only. The key is not stored, logged or returned.
#[tauri::command]
pub fn validate_api_key(key: String) -> AppResult<KeyValidation> {
    Ok(guide::validate_api_key(&key))
}

#[tauri::command]
pub fn preview_effective_url(url: String, state: State<'_, AppState>) -> AppResult<UrlPreview> {
    Ok(guide::preview_url(&url, &state.config_snapshot().config))
}

// ---------------------------------------------------------------------------
// M4 — verify
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn verify_setup(
    request: VerifyRequest,
    state: State<'_, AppState>,
) -> AppResult<VerifyResult> {
    let cfg = state.config_snapshot().config;
    Ok(verify::verify(&state.http, &cfg, request).await)
}

#[tauri::command]
pub fn list_running_terminals() -> AppResult<Vec<TerminalProcess>> {
    Ok(verify::running_terminals())
}

// ---------------------------------------------------------------------------
// M5 — diagnose
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn diagnose(request: DiagnoseRequest, state: State<'_, AppState>) -> AppResult<Vec<Diagnosis>> {
    Ok(diagnose::diagnose(
        &request,
        &state.config_snapshot().config,
    ))
}

#[tauri::command]
pub fn build_diagnostic_report(
    app: AppHandle,
    snapshot: Option<EnvSnapshot>,
    diagnoses: Vec<Diagnosis>,
    state: State<'_, AppState>,
) -> AppResult<DiagnosticReport> {
    let info = app_info(&app, &state);
    let cfg = state.config_snapshot().config;
    Ok(report::build(&report::ReportInput {
        app: &info,
        config: &cfg,
        snapshot: snapshot.as_ref(),
        diagnoses: &diagnoses,
    }))
}

/// Writes `markdown` to `path` (chosen by the user via the native save dialog).
#[tauri::command]
pub async fn save_diagnostic_report(path: String, markdown: String) -> AppResult<()> {
    if !path.to_ascii_lowercase().ends_with(".md") {
        return Err(AppError::InvalidInput("report must be saved as .md".into()));
    }
    tokio::fs::write(&path, crate::redact::redact_secrets(&markdown)).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// M6 — docs
// ---------------------------------------------------------------------------

fn docs_ctx(app: &AppHandle, state: &AppState) -> AppResult<DocsContext> {
    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| AppError::Other(e.to_string()))?
        .join("docs");
    Ok(DocsContext {
        config: state.config_snapshot().config.docs,
        http: state.http.clone(),
        cache_dir,
        resource_dir: app
            .path()
            .resource_dir()
            .ok()
            .map(|p| p.join("resources").join("docs")),
    })
}

#[tauri::command]
pub async fn fetch_docs_index(
    app: AppHandle,
    lang: String,
    state: State<'_, AppState>,
) -> AppResult<DocsIndex> {
    let ctx = docs_ctx(&app, &state)?;
    docs::fetch_index(&ctx, &lang).await
}

#[tauri::command]
pub async fn fetch_doc_page(
    app: AppHandle,
    id: String,
    lang: String,
    state: State<'_, AppState>,
) -> AppResult<DocPage> {
    let ctx = docs_ctx(&app, &state)?;
    docs::fetch_page(&ctx, &id, &lang).await
}

// ---------------------------------------------------------------------------
// telemetry
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn track_event(event: TelemetryEvent, state: State<'_, AppState>) -> AppResult<()> {
    state.telemetry.track(event);
    Ok(())
}

#[tauri::command]
pub fn set_telemetry_enabled(
    enabled: bool,
    state: State<'_, AppState>,
) -> AppResult<TelemetryStatus> {
    state.telemetry.set_enabled(enabled);
    Ok(state.telemetry.status())
}

#[tauri::command]
pub fn get_telemetry_status(state: State<'_, AppState>) -> AppResult<TelemetryStatus> {
    Ok(state.telemetry.status())
}
