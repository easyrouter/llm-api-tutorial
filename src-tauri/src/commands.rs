//! Tauri command layer — thin, argument-validating wrappers around the domain modules.
//! Command names and argument shapes are mirrored in `src/lib/tauri.ts`.
//!
//! Conventions
//! - Every command returns `AppResult<T>`; `AppError` serialises to `{code,message,params}`.
//! - Commands never block the async runtime: CPU-light work runs inline, anything that spawns
//!   processes or does I/O is `async`.
//! - No command accepts a path outside the app's own cache/config dirs except
//!   `save_diagnostic_report`, whose path comes from the native save dialog.

use std::path::{Component, Path, PathBuf};

use tauri::{AppHandle, Manager, State};

use crate::checks::{self, CheckContext};
use crate::diagnose::{self, report};
use crate::docs::{self, DocsContext};
use crate::error::{AppError, AppResult};
use crate::guide;
use crate::install;
use crate::models::{
    AppConfig, AppInfo, CcSwitchImportPreview, CcSwitchImportRequest, CheckId, CheckResult,
    CodexConfigApplyRequest, CodexConfigApplyResult, CodexConfigRequest, CodexConfigStatus,
    ConfigGuide, ConnectivityReport, DiagnoseRequest, Diagnosis, DiagnosticReport, DocPage,
    DocsIndex, DownloadRequest, DownloadResult, EnvCleanupPlan, EnvCleanupResult, EnvSnapshot,
    GatewayProbeRequest, InstallJob, InstallPlan, InstallTarget, InstallerRelease, KeyValidation,
    MirrorChoice, ModelList, PathRepairPlan, PathRepairResult, SystemUri, TelemetryEvent,
    TelemetryStatus, TerminalProcess, ToolId, UrlPreview, UrlRule, VerifyRequest, VerifyResult,
};
use crate::platform::expand_tilde;
use crate::remediate;
use crate::state::AppState;
use crate::verify;

fn app_info(app: &AppHandle, state: &AppState) -> AppInfo {
    let cfg = state.config_snapshot();
    AppInfo {
        name: app.package_info().name.clone(),
        version: app.package_info().version.to_string(),
        platform: crate::platform::platform(),
        arch: std::env::consts::ARCH.to_owned(),
        locale_hint: locale_hint(app),
        config_source: cfg.source,
        log_dir: app
            .path()
            .app_log_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
    }
}

/// Language hint for a first run: the installer's language selection wins over the OS UI
/// language, so an app installed in 简体中文 opens in 简体中文 even on an English Windows.
/// A language the user picked inside the app still wins over both (the frontend keeps it).
fn locale_hint(app: &AppHandle) -> String {
    let cfg = app.config();
    // The NSIS installer keys the registry by publisher, falling back to the identifier's
    // second segment ("com.company.app" -> "company") — mirror that derivation here.
    let manufacturer = cfg
        .bundle
        .publisher
        .clone()
        .or_else(|| cfg.identifier.split('.').nth(1).map(ToOwned::to_owned))
        .unwrap_or_default();
    if let Some(lang) = crate::platform::installer_language(&manufacturer, &app.package_info().name)
    {
        return lang;
    }
    sys_locale_hint()
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

/// Opens one of the well-known OS URIs (Microsoft Store page of the Codex app, Windows region
/// settings). The URI is built here from the enum — the webview never passes a scheme.
#[tauri::command]
pub fn open_system_uri(
    app: AppHandle,
    uri: SystemUri,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let target = system_uri(uri, &state.config_snapshot().config)?;
    tauri_plugin_opener::OpenerExt::opener(&app)
        .open_url(target, None::<&str>)
        .map_err(|e| AppError::Other(e.to_string()))
}

/// The actual URI for a [`SystemUri`] (Windows only — both are `ms-*:` schemes).
pub fn system_uri(uri: SystemUri, config: &AppConfig) -> AppResult<String> {
    if crate::platform::platform() != crate::models::Platform::Windows {
        return Err(AppError::Unsupported(
            "ms-windows-store / ms-settings URIs exist on Windows only".into(),
        ));
    }
    match uri {
        SystemUri::MsStoreCodexApp => {
            let id = config.codex_app.store_product_id.trim();
            if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
                return Err(AppError::Config(
                    "codexApp.storeProductId is not configured".into(),
                ));
            }
            Ok(format!("ms-windows-store://pdp/?productid={id}"))
        }
        SystemUri::WindowsRegionSettings => Ok("ms-settings:regionformatting".to_owned()),
    }
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

/// `exclude_registry` (optional, additive): id of the npm registry a previous attempt failed
/// on; the new plan uses another configured registry when there is one.
#[tauri::command]
pub async fn plan_install(
    target: InstallTarget,
    exclude_registry: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<InstallPlan> {
    let cfg = state.config_snapshot().config;
    let mirrors =
        crate::net::choose_mirrors_avoiding(&state.http, &cfg.mirrors, exclude_registry.as_deref())
            .await;
    install::plan(target, &cfg, &mirrors).await
}

#[tauri::command]
pub async fn start_install(
    app: AppHandle,
    plan: InstallPlan,
    state: State<'_, AppState>,
) -> AppResult<InstallJob> {
    let cfg = state.config_snapshot().config;
    install::start(app, &state.jobs, &cfg, plan).await
}

#[tauri::command]
pub fn cancel_install(job_id: String, state: State<'_, AppState>) -> AppResult<()> {
    state.jobs.cancel(&job_id)
}

/// The installer file to download for `target` (CC Switch, Node.js LTS from the chosen dist
/// mirror, Codex desktop client). npm targets are rejected.
#[tauri::command]
pub async fn fetch_installer_release(
    target: InstallTarget,
    state: State<'_, AppState>,
) -> AppResult<InstallerRelease> {
    let cfg = state.config_snapshot().config;
    let mirrors = crate::net::choose_mirrors(&state.http, &cfg.mirrors).await;
    install::fetch_release(
        target,
        &state.http,
        &cfg,
        &mirrors,
        crate::platform::platform(),
        std::env::consts::ARCH,
    )
    .await
}

/// The command that runs a downloaded installer (Node MSI / pkg, Codex app MSIX / DMG) —
/// shown to the user; `start_install` re-validates it against the file.
#[tauri::command]
pub fn plan_installer_run(
    app: AppHandle,
    target: InstallTarget,
    path: String,
) -> AppResult<InstallPlan> {
    install::plan_installer_run(&app, target, &path)
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

/// Opens a file previously downloaded into `<app cache dir>/downloads` (installer hand-off).
/// The path is canonicalised and must be an existing regular file directly inside that
/// directory — `..` components or symlinks pointing elsewhere are rejected.
#[tauri::command]
pub fn open_downloaded_file(app: AppHandle, path: String) -> AppResult<()> {
    let downloads = install::downloads_dir(&app)?;
    // Validate on canonical paths, but hand the plain path to the opener (Windows canonical
    // paths carry the verbatim `\\?\` prefix, which shell APIs do not always accept).
    install::downloaded_file_in(&downloads, Path::new(&path))?;
    tauri_plugin_opener::OpenerExt::opener(&app)
        .open_path(path, None::<&str>)
        .map_err(|e| AppError::Other(e.to_string()))
}

// ---------------------------------------------------------------------------
// One-click remediation (ADR-0008)
// ---------------------------------------------------------------------------

/// What adding `dir` to the user's persistent PATH would change (shown before `apply`).
#[tauri::command]
pub async fn plan_path_repair(dir: String) -> AppResult<PathRepairPlan> {
    remediate::plan_path_repair(&dir).await
}

/// Applies a confirmed PATH repair plan (re-derived and compared first).
#[tauri::command]
pub async fn apply_path_repair(plan: PathRepairPlan) -> AppResult<PathRepairResult> {
    remediate::apply_path_repair(&plan).await
}

/// Every persistent source of the given conflicting variables, with full values, and how each
/// would be removed. Values are returned once for the confirmation dialog and never logged.
#[tauri::command]
pub async fn plan_env_cleanup(
    names: Vec<String>,
    state: State<'_, AppState>,
) -> AppResult<EnvCleanupPlan> {
    let cfg = state.config_snapshot().config;
    remediate::plan_env_cleanup(&names, &cfg).await
}

/// Removes the confirmed sources (items whose value changed since the preview are skipped).
#[tauri::command]
pub async fn apply_env_cleanup(
    plan: EnvCleanupPlan,
    state: State<'_, AppState>,
) -> AppResult<EnvCleanupResult> {
    let cfg = state.config_snapshot().config;
    remediate::apply_env_cleanup(&plan, &cfg).await
}

/// State of `~/.codex/config.toml` (exists? backups? equals `template`?).
#[tauri::command]
pub fn codex_config_status(
    template: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<CodexConfigStatus> {
    let cfg = state.config_snapshot().config;
    remediate::codex_config_status(&cfg, template.as_deref())
}

/// Writes the confirmed `config.toml` (backup first). The content may carry the real key and
/// is never logged.
#[tauri::command]
pub fn apply_codex_config(
    request: CodexConfigApplyRequest,
    state: State<'_, AppState>,
) -> AppResult<CodexConfigApplyResult> {
    let cfg = state.config_snapshot().config;
    remediate::apply_codex_config(&cfg, &request.content)
}

/// Puts the newest backup back (the replaced file is itself backed up).
#[tauri::command]
pub fn restore_codex_config(state: State<'_, AppState>) -> AppResult<CodexConfigApplyResult> {
    let cfg = state.config_snapshot().config;
    remediate::restore_codex_config(&cfg)
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

/// In-place connectivity test on the configure screen: URL rules + key format always run; the
/// live gateway probe only when the URL is valid and the key has no blocking format issues.
/// The key lives in memory for this one request and is never logged or stored (hard rule 3).
#[tauri::command]
pub async fn test_connectivity(
    request: GatewayProbeRequest,
    state: State<'_, AppState>,
) -> AppResult<ConnectivityReport> {
    let cfg = state.config_snapshot().config;
    let url = guide::preview_url(&request.base_url, &cfg);
    let key = guide::validate_api_key(&request.api_key);
    let gateway = if url.rule != UrlRule::Invalid && key.valid {
        Some(verify::probe_gateway(&state.http, &request).await)
    } else {
        None
    };
    Ok(ConnectivityReport { url, key, gateway })
}

/// Fetches the gateway's model list (`GET {base}/models`); `request.model` is ignored.
#[tauri::command]
pub async fn list_gateway_models(
    request: GatewayProbeRequest,
    state: State<'_, AppState>,
) -> AppResult<ModelList> {
    Ok(verify::list_models(&state.http, &request).await)
}

/// Renders the recommended Codex `config.toml` template (editable in the UI before the user
/// pastes it into CC Switch). Carries no key: the template contains a placeholder the UI
/// substitutes at copy time.
#[tauri::command]
pub fn get_codex_config_template(
    request: CodexConfigRequest,
    state: State<'_, AppState>,
) -> AppResult<String> {
    Ok(guide::codex_config_template(
        &request,
        &state.config_snapshot().config,
    ))
}

/// The masked CC Switch import deep link for the confirmation dialog ("show before run").
#[tauri::command]
pub fn preview_cc_switch_import(
    request: CcSwitchImportRequest,
    state: State<'_, AppState>,
) -> AppResult<CcSwitchImportPreview> {
    let cfg = state.config_snapshot().config;
    Ok(CcSwitchImportPreview {
        display_url: guide::masked_import_url(&request, &cfg)?,
        app: guide::cc_switch_app(request.tool).to_owned(),
    })
}

/// Opens the `ccswitch://` provider-import deep link after the user confirmed the masked
/// preview. CC Switch shows its own confirmation dialog and writes its own data — this app
/// never touches `~/.cc-switch` (ADR-0003/0006). The URL (which contains the key) is never
/// logged; opener errors are scrubbed before they leave this function.
#[tauri::command]
pub fn open_cc_switch_import(
    app: AppHandle,
    request: CcSwitchImportRequest,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let cfg = state.config_snapshot().config;
    let url = guide::build_import_url(&request, &cfg)?;
    tauri_plugin_opener::OpenerExt::opener(&app)
        .open_url(url, None::<&str>)
        .map_err(|e| {
            AppError::Other(crate::redact::redact_secrets(&verify::scrub_known_secret(
                &e.to_string(),
                &request.api_key,
            )))
        })
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

/// `verify` (optional, additive) carries the latest verification results so the report also
/// contains the raw error information when no diagnosis rule matched.
#[tauri::command]
pub fn build_diagnostic_report(
    app: AppHandle,
    snapshot: Option<EnvSnapshot>,
    diagnoses: Vec<Diagnosis>,
    verify: Option<Vec<VerifyResult>>,
    state: State<'_, AppState>,
) -> AppResult<DiagnosticReport> {
    let info = app_info(&app, &state);
    let cfg = state.config_snapshot().config;
    Ok(report::build(&report::ReportInput {
        app: &info,
        config: &cfg,
        snapshot: snapshot.as_ref(),
        diagnoses: &diagnoses,
        verify: verify.as_deref().unwrap_or_default(),
    }))
}

/// Writes `markdown` to `path` (chosen by the user via the native save dialog). Paths inside
/// the tool config directories (`~/.codex`, `~/.claude`, `~/.cc-switch`) are refused — hard
/// rule 1 holds even if the dialog is pointed there.
#[tauri::command]
pub async fn save_diagnostic_report(
    path: String,
    markdown: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    if !path.to_ascii_lowercase().ends_with(".md") {
        return Err(AppError::InvalidInput("report must be saved as .md".into()));
    }
    let protected = protected_dirs(&state.config_snapshot().config);
    if is_under_any(Path::new(&path), &protected) {
        return Err(AppError::InvalidInput(
            "the report must not be saved inside a tool configuration directory".into(),
        ));
    }
    tokio::fs::write(&path, crate::redact::redact_secrets(&markdown)).await?;
    Ok(())
}

/// Directories the app must never write into (`ToolSpec.config_dir` + CC Switch data dir),
/// tilde-expanded.
fn protected_dirs(config: &AppConfig) -> Vec<PathBuf> {
    config
        .tools
        .iter()
        .map(|t| t.config_dir.as_str())
        .chain(std::iter::once(config.cc_switch.data_dir.as_str()))
        .filter(|d| !d.trim().is_empty())
        .map(expand_tilde)
        .collect()
}

/// `true` when `path` lies inside any of `dirs`, compared on normalised paths (see
/// [`normalize_path`]) so `dir/../x` cannot escape and `dir/./sub` cannot hide.
fn is_under_any(path: &Path, dirs: &[PathBuf]) -> bool {
    let target = normalize_path(path);
    dirs.iter().any(|dir| {
        let dir = normalize_path(dir);
        !dir.as_os_str().is_empty() && target.starts_with(&dir)
    })
}

/// Lexical normalisation (`.` dropped, `..` applied component-wise), then the longest existing
/// prefix is canonicalised so symlinked or differently-cased spellings compare equal. Pure
/// apart from read-only `exists` / `canonicalize` probes.
fn normalize_path(path: &Path) -> PathBuf {
    let mut lexical = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                lexical.pop();
            }
            other => lexical.push(other.as_os_str()),
        }
    }
    let existing = lexical
        .ancestors()
        .find(|p| p.exists())
        .map(Path::to_path_buf);
    let canonical = existing.and_then(|p| std::fs::canonicalize(&p).ok().map(|c| (p, c)));
    match canonical {
        Some((prefix, canonical)) => match lexical.strip_prefix(&prefix) {
            Ok(rest) => canonical.join(rest),
            Err(_) => lexical,
        },
        None => lexical,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloaded_file_must_be_a_file_directly_inside_the_downloads_dir() {
        let root = tempfile::tempdir().expect("tempdir");
        let downloads = root.path().join(install::DOWNLOADS_DIR);
        std::fs::create_dir_all(downloads.join("nested")).expect("mkdir");
        let ok = downloads.join("setup.exe");
        std::fs::write(&ok, b"x").expect("write");
        let nested = downloads.join("nested").join("setup.exe");
        std::fs::write(&nested, b"x").expect("write");
        let outside = root.path().join("setup.exe");
        std::fs::write(&outside, b"x").expect("write");

        let accepted = install::downloaded_file_in(&downloads, &ok).expect("inside");
        assert!(accepted.ends_with("setup.exe"));
        // `.` is normalised away by `Path::components`; the file is still the right one
        install::downloaded_file_in(&downloads, &downloads.join(".").join("setup.exe"))
            .expect("dot");

        let traversal = downloads.join("..").join("setup.exe");
        let sneaky = downloads.join("nested").join("..").join("setup.exe");
        for bad in [
            traversal.as_path(),
            sneaky.as_path(),
            outside.as_path(),
            nested.as_path(),
            downloads.as_path(),
            downloads.join("missing.exe").as_path(),
        ] {
            assert_eq!(
                install::downloaded_file_in(&downloads, bad)
                    .expect_err(&format!("{}", bad.display()))
                    .code(),
                "invalid_input"
            );
        }
    }

    #[test]
    fn report_paths_inside_tool_config_dirs_are_detected() {
        let root = tempfile::tempdir().expect("tempdir");
        let codex = root.path().join(".codex");
        std::fs::create_dir_all(&codex).expect("mkdir");
        let dirs = vec![codex.clone(), root.path().join(".claude")];

        assert!(is_under_any(&codex.join("report.md"), &dirs));
        assert!(is_under_any(&codex.join("sub").join("deep.md"), &dirs));
        assert!(is_under_any(
            &root.path().join("x").join("..").join(".codex").join("r.md"),
            &dirs
        ));
        // `.claude` does not exist yet — lexical comparison still catches it
        assert!(is_under_any(
            &root.path().join(".claude").join("r.md"),
            &dirs
        ));
        assert!(!is_under_any(&root.path().join("report.md"), &dirs));
        assert!(!is_under_any(&codex.join("..").join("report.md"), &dirs));
        assert!(!is_under_any(
            &root.path().join(".codex-reports").join("r.md"),
            &dirs
        ));
        assert!(!is_under_any(&codex.join("r.md"), &[]));
    }

    #[test]
    fn protected_dirs_come_from_the_config() {
        let cfg = crate::config::embedded().expect("config");
        let dirs = protected_dirs(&cfg);
        assert_eq!(dirs.len(), 3, "{dirs:?}");
        assert!(dirs.iter().any(|d| d.ends_with(".codex")));
        assert!(dirs.iter().any(|d| d.ends_with(".claude")));
        assert!(dirs.iter().any(|d| d.ends_with(".cc-switch")));
    }
}
