//! M2 — installation. Nothing is executed without the frontend first showing
//! `InstallPlan.display_command` and the user confirming (`start` only ever runs the exact
//! program/args of a plan the UI handed back).
//!
//! Flow
//! - [`plan`] gathers machine facts ([`plan::PlanContext`]) and delegates to the pure
//!   [`plan::build_plan`]:
//!     * Node      → no command; `download_url` = download page of the chosen mirror
//!                   (macOS with Homebrew: `brew install node@<lts>` alternative)
//!     * Codex / ClaudeCode → `npm install -g <pkg> --registry <chosen mirror>`;
//!                   `requires_admin` = npm prefix not writable by the user (we never elevate —
//!                   PRD #7; the UI shows how to switch to a per-user prefix)
//!     * CcSwitch  → no command; the UI calls `fetch_cc_switch_release` then `download_file`
//! - [`start`] runs a command plan through `process::run_streaming` in a tokio task, emitting
//!   `install://output` per line (redacted) and `install://done` at the end; the job is
//!   cancellable through [`JobRegistry`].
//! - [`latest_cc_switch_release`] / [`download_installer`]: see [`cc_switch`] and `net`.

pub mod cc_switch;
pub mod plan;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::watch;

use crate::error::{AppError, AppResult};
use crate::events::{DOWNLOAD_PROGRESS, INSTALL_DONE, INSTALL_OUTPUT};
use crate::models::{
    AppConfig, CcSwitchRelease, DownloadProgressEvent, DownloadRequest, DownloadResult,
    InstallDoneEvent, InstallJob, InstallOutputEvent, InstallPlan, InstallTarget, MirrorChoice,
    OutputStream, Platform,
};
use crate::process::{self, CommandSpec};
use crate::redact::redact_secrets;
use crate::{net, platform};

pub use plan::PlanContext;

/// Upper bound for one install job (`npm install -g` on a slow mirror can take minutes).
pub const INSTALL_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Sub-directory of the app cache dir that receives downloaded installers.
pub const DOWNLOADS_DIR: &str = "downloads";

// ---------------------------------------------------------------------------
// Job registry
// ---------------------------------------------------------------------------

/// Tracks running install jobs so they can be cancelled. Cheap to clone (shared handle).
#[derive(Debug, Clone, Default)]
pub struct JobRegistry {
    inner: Arc<Mutex<HashMap<String, watch::Sender<bool>>>>,
}

impl JobRegistry {
    /// Registers `job_id` and returns the receiver that flips to `true` on [`cancel`](Self::cancel).
    pub fn register(&self, job_id: &str) -> watch::Receiver<bool> {
        let (tx, rx) = watch::channel(false);
        if let Ok(mut map) = self.inner.lock() {
            map.insert(job_id.to_owned(), tx);
        }
        rx
    }

    /// Requests cancellation of a running job.
    pub fn cancel(&self, job_id: &str) -> AppResult<()> {
        let map = self
            .inner
            .lock()
            .map_err(|_| AppError::Other("job registry poisoned".into()))?;
        match map.get(job_id) {
            Some(tx) => {
                let _ = tx.send(true);
                Ok(())
            }
            None => Err(AppError::JobNotFound(job_id.to_owned())),
        }
    }

    /// Forgets a job (called when its task ends, whatever the outcome).
    pub fn finish(&self, job_id: &str) {
        if let Ok(mut map) = self.inner.lock() {
            map.remove(job_id);
        }
    }

    /// `true` while `job_id` is registered.
    pub fn is_running(&self, job_id: &str) -> bool {
        self.inner.lock().is_ok_and(|map| map.contains_key(job_id))
    }
}

// ---------------------------------------------------------------------------
// plan
// ---------------------------------------------------------------------------

/// Builds the plan for `target` on this machine (see module docs and [`plan::build_plan`]).
pub async fn plan(
    target: InstallTarget,
    config: &AppConfig,
    mirrors: &MirrorChoice,
) -> AppResult<InstallPlan> {
    let ctx = plan_context().await;
    plan::build_plan(target, config, mirrors, &ctx)
}

/// Collects the machine facts the pure plan builder needs.
async fn plan_context() -> PlanContext {
    let platform = platform::platform();
    PlanContext {
        platform,
        homebrew: resolve_brew(),
        npm_program: resolve_npm(platform),
        npm_prefix_writable: npm_prefix_writable().await,
    }
}

/// Full path of `brew` when Homebrew is usable (macOS), else `None`.
fn resolve_brew() -> Option<String> {
    if !platform::homebrew_available() {
        return None;
    }
    platform::find_on_path("brew")
        .or_else(|| {
            ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
                .iter()
                .map(PathBuf::from)
                .find(|p| p.is_file())
        })
        .map(|p| p.to_string_lossy().into_owned())
}

/// Full path of the `npm` shim, or the bare name when it cannot be found.
fn resolve_npm(platform: Platform) -> String {
    platform::find_binary("npm", &node_install_dirs(platform)).map_or_else(
        || "npm".to_owned(),
        |(path, _)| path.to_string_lossy().into_owned(),
    )
}

/// Standard Node.js install locations searched when `npm` is not on this process' `PATH`
/// (typically: Node was installed a moment ago and the app has not been restarted).
fn node_install_dirs(platform: Platform) -> Vec<PathBuf> {
    match platform {
        Platform::Windows => ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
            .iter()
            .filter_map(std::env::var_os)
            .map(|base| PathBuf::from(base).join("nodejs"))
            .collect(),
        Platform::Macos | Platform::Linux => {
            vec![
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
            ]
        }
        Platform::Unknown => Vec::new(),
    }
}

/// `true` when the current user can write into the npm global prefix (so `npm install -g`
/// works without elevation). Unknown prefix → assumed writable (npm reports a clear `EACCES`
/// otherwise; we never elevate).
pub async fn npm_prefix_writable() -> bool {
    let Some(prefix) = platform::npm_global_prefix().await else {
        log::info!("npm prefix unknown; assuming writable");
        return true;
    };
    let probe = writable_probe_dir(&prefix, platform::platform());
    let writable = dir_writable(&probe);
    log::info!(
        "npm prefix {} ({}): {}",
        prefix.display(),
        probe.display(),
        if writable { "writable" } else { "read-only" }
    );
    writable
}

/// Directory whose writability decides whether `npm install -g` needs elevation: the deepest
/// existing one of `<prefix>/lib/node_modules`, `<prefix>/lib`, `<prefix>` (Unix) or
/// `<prefix>/node_modules`, `<prefix>` (Windows); when the prefix does not exist yet, its
/// nearest existing ancestor (npm creates the rest).
pub fn writable_probe_dir(prefix: &Path, platform: Platform) -> PathBuf {
    let candidates: Vec<PathBuf> = match platform {
        Platform::Windows => vec![prefix.join("node_modules"), prefix.to_path_buf()],
        _ => vec![
            prefix.join("lib").join("node_modules"),
            prefix.join("lib"),
            prefix.to_path_buf(),
        ],
    };
    if let Some(existing) = candidates.into_iter().find(|p| p.is_dir()) {
        return existing;
    }
    prefix
        .ancestors()
        .find(|p| p.is_dir())
        .map_or_else(|| prefix.to_path_buf(), Path::to_path_buf)
}

/// Creates and immediately deletes an empty probe file in `dir`.
pub fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".codex-onboarding-{}.tmp", uuid::Uuid::new_v4()));
    let created = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .is_ok();
    if created {
        let _ = std::fs::remove_file(&probe);
    }
    created
}

// ---------------------------------------------------------------------------
// start
// ---------------------------------------------------------------------------

/// Starts the command of a confirmed plan and returns immediately with the job id. Output
/// arrives on `install://output` (first line: the display command, stream `system`), the
/// outcome on `install://done`. Plans without a command (`program == ""`) are rejected.
pub async fn start(app: AppHandle, jobs: &JobRegistry, plan: InstallPlan) -> AppResult<InstallJob> {
    let spec = command_spec(&plan)?;
    ensure_program_exists(&spec.program)?;
    let job_id = uuid::Uuid::new_v4().to_string();
    let cancel = jobs.register(&job_id);
    log::info!(
        "install job {job_id} ({:?}) started: {}",
        plan.target,
        redact_secrets(&plan.display_command)
    );
    let job = InstallJob {
        job_id: job_id.clone(),
        target: plan.target,
    };
    tokio::spawn(run_job(
        app,
        jobs.clone(),
        job_id,
        spec,
        plan.display_command,
        cancel,
    ));
    Ok(job)
}

/// The exact command of `plan` with the install timeout; the inherited environment is kept
/// and extended by `plan.env` (never cleared).
pub fn command_spec(plan: &InstallPlan) -> AppResult<CommandSpec> {
    if plan.program.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "install plan has no command to execute".to_owned(),
        ));
    }
    Ok(CommandSpec {
        program: plan.program.clone(),
        args: plan.args.clone(),
        env: plan.env.clone(),
        clear_env: false,
        cwd: None,
        timeout: INSTALL_TIMEOUT,
    })
}

/// Pre-flight so a missing program surfaces as `AppError::CommandNotFound` from `start`
/// (with its i18n code) instead of a failed job.
pub fn ensure_program_exists(program: &str) -> AppResult<()> {
    let found = if program.contains(['/', '\\']) {
        Path::new(program).is_file()
    } else {
        process::resolve_program(program).is_some()
    };
    if found {
        Ok(())
    } else {
        Err(AppError::CommandNotFound {
            program: program.to_owned(),
        })
    }
}

fn emit_output(app: &AppHandle, job_id: &str, stream: OutputStream, line: String) {
    let _ = app.emit(
        INSTALL_OUTPUT,
        InstallOutputEvent {
            job_id: job_id.to_owned(),
            stream,
            line,
        },
    );
}

/// Body of the spawned task: stream, then report `install://done` and free the job.
async fn run_job(
    app: AppHandle,
    jobs: JobRegistry,
    job_id: String,
    spec: CommandSpec,
    display_command: String,
    cancel: watch::Receiver<bool>,
) {
    let started = Instant::now();
    emit_output(
        &app,
        &job_id,
        OutputStream::System,
        redact_secrets(&display_command),
    );
    let cancel_flag = cancel.clone();
    let (line_app, line_job) = (app.clone(), job_id.clone());
    let result = process::run_streaming(&spec, cancel, move |stream, line| {
        emit_output(&line_app, &line_job, stream, redact_secrets(&line));
    })
    .await;

    let done = match result {
        Ok(output) => {
            let cancelled =
                output.exit_code.is_none() && !output.timed_out && *cancel_flag.borrow();
            if output.timed_out {
                log::warn!(
                    "install job {job_id} timed out after {} ms",
                    output.duration_ms
                );
            }
            InstallDoneEvent {
                job_id: job_id.clone(),
                success: output.success(),
                exit_code: output.exit_code,
                duration_ms: output.duration_ms,
                cancelled,
            }
        }
        Err(e) => {
            let message = redact_secrets(&e.to_string());
            log::warn!("install job {job_id} could not run: {message}");
            emit_output(&app, &job_id, OutputStream::System, message);
            InstallDoneEvent {
                job_id: job_id.clone(),
                success: false,
                exit_code: None,
                duration_ms: elapsed_ms(started),
                cancelled: false,
            }
        }
    };
    log::info!(
        "install job {job_id} finished: success={} exit_code={:?} cancelled={} ({} ms)",
        done.success,
        done.exit_code,
        done.cancelled,
        done.duration_ms
    );
    jobs.finish(&job_id);
    let _ = app.emit(INSTALL_DONE, &done);
}

fn elapsed_ms(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

// ---------------------------------------------------------------------------
// CC Switch release + downloads
// ---------------------------------------------------------------------------

/// Latest CC Switch installer for `platform`/`arch` (intranet mirror or GitHub — see
/// [`cc_switch`]).
pub async fn latest_cc_switch_release(
    client: &reqwest::Client,
    config: &AppConfig,
    platform: Platform,
    arch: &str,
) -> AppResult<CcSwitchRelease> {
    cc_switch::latest_release(client, config, platform, arch).await
}

/// Downloads `req` (https only, SHA-256 verified when `expected_sha256` is set) into
/// `<app cache dir>/downloads`, streaming `download://progress` events for `req.job_id`.
pub async fn download_installer(
    app: AppHandle,
    client: &reqwest::Client,
    req: DownloadRequest,
) -> AppResult<DownloadResult> {
    let dest_dir = downloads_dir(&app)?;
    let downloader = download_client(client);
    let job_id = req.job_id.clone();
    let emitter = app.clone();
    net::download(&downloader, &req, dest_dir, move |downloaded, total| {
        let _ = emitter.emit(
            DOWNLOAD_PROGRESS,
            DownloadProgressEvent {
                job_id: job_id.clone(),
                downloaded,
                total,
            },
        );
    })
    .await
}

/// `<app cache dir>/downloads` — the only place installers are written to.
fn downloads_dir(app: &AppHandle) -> AppResult<PathBuf> {
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|e| AppError::Other(format!("app cache dir: {e}")))?;
    Ok(cache.join(DOWNLOADS_DIR))
}

/// Client for installer downloads: same rustls / proxy / user-agent setup as the shared client
/// but without its 60 s overall request timeout (installers are tens of MB on slow links; the
/// 30 s read timeout still bounds stalls). Falls back to `shared` when building fails.
fn download_client(shared: &reqwest::Client) -> reqwest::Client {
    net::client_with_timeout(0).unwrap_or_else(|e| {
        log::warn!("download client unavailable, using shared client: {e}");
        shared.clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn plan_with(program: &str, args: &[&str]) -> InstallPlan {
        InstallPlan {
            target: InstallTarget::Codex,
            program: program.to_owned(),
            args: args.iter().map(|s| (*s).to_owned()).collect(),
            env: BTreeMap::new(),
            display_command: String::new(),
            registry: None,
            requires_admin: false,
            explanation_code: plan::CODE_NPM_GLOBAL.to_owned(),
            download_url: None,
        }
    }

    #[test]
    fn registry_cancel_flips_receiver_and_finish_forgets() {
        let jobs = JobRegistry::default();
        let rx = jobs.register("job-1");
        assert!(!*rx.borrow());
        assert!(jobs.is_running("job-1"));

        jobs.cancel("job-1").expect("known job");
        assert!(*rx.borrow(), "receiver sees the cancel flag");

        jobs.finish("job-1");
        assert!(!jobs.is_running("job-1"));
        assert_eq!(
            jobs.cancel("job-1").expect_err("finished job").code(),
            "job_not_found"
        );
    }

    #[test]
    fn registry_cancel_unknown_job_is_an_error() {
        let jobs = JobRegistry::default();
        assert_eq!(
            jobs.cancel("nope").expect_err("unknown").code(),
            "job_not_found"
        );
    }

    #[test]
    fn registry_clones_share_state() {
        let jobs = JobRegistry::default();
        let handle = jobs.clone();
        let rx = jobs.register("shared");
        handle.cancel("shared").expect("visible through the clone");
        assert!(*rx.borrow());
        handle.finish("shared");
        assert!(!jobs.is_running("shared"));
    }

    #[test]
    fn command_spec_rejects_manual_plans() {
        let err = command_spec(&plan_with("  ", &[])).expect_err("no program");
        assert_eq!(err.code(), "invalid_input");
    }

    #[test]
    fn command_spec_keeps_program_args_env_and_timeout() {
        let mut plan = plan_with("npm", &["install", "-g", "@openai/codex"]);
        plan.env
            .insert("NPM_CONFIG_LOGLEVEL".into(), "verbose".into());
        let spec = command_spec(&plan).expect("spec");
        assert_eq!(spec.program, "npm");
        assert_eq!(spec.args, vec!["install", "-g", "@openai/codex"]);
        assert_eq!(
            spec.env.get("NPM_CONFIG_LOGLEVEL").map(String::as_str),
            Some("verbose")
        );
        assert!(!spec.clear_env, "inherited environment must be kept");
        assert!(spec.cwd.is_none());
        assert_eq!(spec.timeout, INSTALL_TIMEOUT);
    }

    #[test]
    fn ensure_program_exists_reports_missing_path_and_name() {
        let missing_path = if cfg!(windows) {
            r"C:\definitely\missing\npm.cmd"
        } else {
            "/definitely/missing/npm"
        };
        assert_eq!(
            ensure_program_exists(missing_path)
                .expect_err("path")
                .code(),
            "command_not_found"
        );
        assert_eq!(
            ensure_program_exists("codex-onboarding-no-such-program-xyz")
                .expect_err("name")
                .code(),
            "command_not_found"
        );
    }

    #[test]
    fn ensure_program_exists_accepts_existing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("tool.exe");
        std::fs::write(&file, b"").expect("write");
        ensure_program_exists(&file.to_string_lossy()).expect("existing file");
    }

    #[test]
    fn dir_writable_true_for_tempdir_and_false_for_missing_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(dir_writable(dir.path()));
        assert!(
            std::fs::read_dir(dir.path())
                .expect("read")
                .next()
                .is_none(),
            "probe file removed"
        );
        assert!(!dir_writable(&dir.path().join("does-not-exist")));
    }

    #[test]
    fn writable_probe_dir_prefers_node_modules_then_ancestors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let prefix = dir.path().join("prefix");

        // Nothing exists yet → nearest existing ancestor.
        assert_eq!(writable_probe_dir(&prefix, Platform::Windows), dir.path());
        assert_eq!(writable_probe_dir(&prefix, Platform::Macos), dir.path());

        std::fs::create_dir_all(prefix.join("lib")).expect("mkdir lib");
        assert_eq!(
            writable_probe_dir(&prefix, Platform::Macos),
            prefix.join("lib")
        );
        assert_eq!(writable_probe_dir(&prefix, Platform::Windows), prefix);

        std::fs::create_dir_all(prefix.join("lib").join("node_modules")).expect("mkdir");
        std::fs::create_dir_all(prefix.join("node_modules")).expect("mkdir");
        assert_eq!(
            writable_probe_dir(&prefix, Platform::Macos),
            prefix.join("lib").join("node_modules")
        );
        assert_eq!(
            writable_probe_dir(&prefix, Platform::Windows),
            prefix.join("node_modules")
        );
    }

    #[test]
    fn node_install_dirs_are_platform_specific() {
        assert!(node_install_dirs(Platform::Macos)
            .iter()
            .any(|p| p == Path::new("/opt/homebrew/bin")));
        assert!(node_install_dirs(Platform::Unknown).is_empty());
        for dir in node_install_dirs(Platform::Windows) {
            assert!(dir.ends_with("nodejs"));
        }
    }

    #[test]
    fn resolve_npm_falls_back_to_bare_name() {
        let resolved = resolve_npm(platform::platform());
        assert!(
            resolved == "npm" || Path::new(&resolved).is_file(),
            "either the bare name or an existing shim: {resolved}"
        );
    }
}
