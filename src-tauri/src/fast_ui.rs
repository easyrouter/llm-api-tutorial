//! Optional Codex "Fast UI" toolkit — Windows only, opt-in (ADR-0007).
//!
//! Users who cannot sign in with a ChatGPT account (no OpenAI auth available) do not see the
//! speed / service-tier option in the Codex client, so the same gateway configuration gives
//! them a different experience. IT ships a minimal, MIT-licensed toolkit that produces an
//! **independent copy** of the officially installed Codex client with that single UI gate
//! opened.
//!
//! Boundaries this module keeps (see ADR-0007 for the full argument):
//! - The archive ships as an app resource; its SHA-256 is verified against [`TOOLKIT_SHA256`]
//!   before every use, and it is re-extracted from the verified archive each time (hard rule 7).
//! - The toolkit's own `install.ps1` copies the user's official Codex app to [`INSTALL_SUBDIR`]
//!   under the local app-data directory and patches the copy. The system installation under
//!   `WindowsApps` is never modified, and the copy runs on its own user-data directory, so the
//!   official client keeps working unchanged next to it.
//! - `~/.codex`, `~/.claude` and `~/.cc-switch` are never written (hard rule 1): the patch
//!   touches application assets only, never configuration.
//! - Nothing runs before the UI showed `FastUiPlan::display_command` and the user confirmed
//!   (hard rule 4), nothing is elevated (PRD #7), and every command goes through `process`
//!   (hard rule 6).
//! - Reversible: `restore.ps1` puts the official `app.asar` back, and deleting the install
//!   root removes the copy entirely.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tauri::path::BaseDirectory;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::watch;

use crate::error::{AppError, AppResult};
use crate::events::{FAST_UI_DONE, FAST_UI_OUTPUT};
use crate::install::JobRegistry;
use crate::models::{
    FastUiAction, FastUiJob, FastUiPlan, FastUiStatus, InstallDoneEvent, InstallOutputEvent,
    OutputStream, Platform,
};
use crate::process::{self, CommandSpec};
use crate::redact::redact_secrets;
use crate::{net, platform};

/// Archive shipped as `resources/codex-fast-ui/<name>` (see `tauri.conf.json`).
pub const TOOLKIT_ARCHIVE: &str = "CodexFastUI-Minimal-2026.07.30.zip";
/// Top-level directory inside the archive.
pub const TOOLKIT_DIR_NAME: &str = "CodexFastUI-Minimal-2026.07.30";
/// Toolkit version as the toolkit itself reports it (`patch-install.json` → `toolkitVersion`).
pub const TOOLKIT_VERSION: &str = "2026.07.30-minimal";
/// SHA-256 of [`TOOLKIT_ARCHIVE`]. Replacing the toolkit means replacing this constant.
pub const TOOLKIT_SHA256: &str = "3f1e6ae7c46ad0dbb1849f4f3ebab6798becb485bb1f10e73085cd702e48dc2a";
/// Codex Windows build the toolkit was tested against. Newer builds may not be patchable — the
/// patcher refuses to guess and stops instead (toolkit README).
pub const TESTED_CODEX_BUILD: &str = "OpenAI.Codex 26.721.11231.0";

/// Where the independent patched copy lives, relative to the local app-data directory.
pub const INSTALL_SUBDIR: [&str; 2] = ["SeedRouter", "CodexFastUI"];
/// Sub-directory of the app cache dir the archive is extracted into.
pub const EXTRACT_DIR: &str = "codex-fast-ui";
/// Marker file `install.ps1` writes on success.
const INSTALL_MARKER: &str = "patch-install.json";
/// Shortcut `install.ps1` creates in the install root.
const SHORTCUT_NAME: &str = "Codex Fast UI.lnk";

/// Copying an Electron app and repacking its ASAR takes minutes on a slow disk.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(20 * 60);
/// Verify / restore only touch the already-copied app.
const MAINTENANCE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// `Get-AppxPackage` is slow on a cold package cache.
const APPX_TIMEOUT: Duration = Duration::from_secs(30);
/// Unpacking ~300 small files.
const EXTRACT_TIMEOUT: Duration = Duration::from_secs(120);

// ---------------------------------------------------------------------------
// Blocking reasons — i18n codes, never prose (hard rule 5)
// ---------------------------------------------------------------------------

/// Not Windows: the toolkit patches a Windows AppX Electron build.
const BLOCKED_NOT_WINDOWS: &str = "guide:fastui.blocked.not_windows";
/// The archive is missing, unreadable, or its hash does not match the pinned one.
const BLOCKED_TOOLKIT_MISSING: &str = "guide:fastui.blocked.toolkit_missing";
/// The official Codex client is not installed for this user, so there is nothing to copy.
const BLOCKED_CODEX_APP_MISSING: &str = "guide:fastui.blocked.codex_app_missing";
/// Verify / restore need a previous install.
const BLOCKED_NOT_INSTALLED: &str = "guide:fastui.blocked.not_installed";

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// `<local app data>/SeedRouter/CodexFastUI` — the only directory this feature writes to.
pub fn install_root() -> AppResult<PathBuf> {
    let base = dirs::data_local_dir()
        .ok_or_else(|| AppError::Other("local app-data directory unavailable".to_owned()))?;
    Ok(INSTALL_SUBDIR
        .iter()
        .fold(base, |path, part| path.join(part)))
}

/// The extracted toolkit directory under the app cache dir.
fn toolkit_dir(app: &AppHandle) -> AppResult<PathBuf> {
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|e| AppError::Other(format!("app cache dir: {e}")))?;
    Ok(cache
        .join(EXTRACT_DIR)
        .join(TOOLKIT_VERSION)
        .join(TOOLKIT_DIR_NAME))
}

/// The bundled archive inside the app resources.
fn archive_path(app: &AppHandle) -> AppResult<PathBuf> {
    app.path()
        .resolve(
            format!("resources/{EXTRACT_DIR}/{TOOLKIT_ARCHIVE}"),
            BaseDirectory::Resource,
        )
        .map_err(|e| AppError::Other(format!("toolkit resource: {e}")))
}

/// Lower-case hex SHA-256 of a file.
pub fn sha256_file(path: &Path) -> AppResult<String> {
    let bytes = std::fs::read(path)?;
    Ok(net::sha256_hex(&bytes))
}

/// `true` when the archive exists and matches [`TOOLKIT_SHA256`]. A mismatch is logged and
/// treated as "not available": a tampered or half-copied resource must never be executed.
fn archive_verified(path: &Path) -> bool {
    match sha256_file(path) {
        Ok(actual) if net::hashes_match(TOOLKIT_SHA256, &actual) => true,
        Ok(actual) => {
            log::warn!("codex fast-ui toolkit hash mismatch, got {actual}");
            false
        }
        Err(e) => {
            log::warn!(
                "codex fast-ui toolkit unreadable at {}: {e}",
                path.display()
            );
            false
        }
    }
}

/// Script `action` runs. Install uses the freshly extracted toolkit; verify and restore use the
/// copies `install.ps1` placed in the install root (what the toolkit README documents).
fn script_path(action: FastUiAction, toolkit: &Path, root: &Path) -> PathBuf {
    match action {
        FastUiAction::Install => toolkit.join("install.ps1"),
        FastUiAction::Verify => root.join("verify.ps1"),
        FastUiAction::Restore => root.join("restore.ps1"),
    }
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

/// Decides whether `action` can run, purely from already-collected facts. `None` = it can.
pub fn blocked_code(action: FastUiAction, state: &FastUiStatus) -> Option<&'static str> {
    if !state.supported {
        return Some(BLOCKED_NOT_WINDOWS);
    }
    if !state.toolkit_available {
        return Some(BLOCKED_TOOLKIT_MISSING);
    }
    match action {
        FastUiAction::Install if !state.codex_app_found => Some(BLOCKED_CODEX_APP_MISSING),
        FastUiAction::Verify | FastUiAction::Restore if !state.installed => {
            Some(BLOCKED_NOT_INSTALLED)
        }
        _ => None,
    }
}

/// Availability of the feature on this machine. Cheap except for the AppX probe, which only
/// runs on Windows.
pub async fn status(app: &AppHandle) -> AppResult<FastUiStatus> {
    let supported = platform::platform() == Platform::Windows;
    let root = install_root().unwrap_or_default();
    let codex_app = if supported {
        codex_app_path().await
    } else {
        None
    };

    let mut state = FastUiStatus {
        supported,
        toolkit_version: TOOLKIT_VERSION.to_owned(),
        tested_codex_build: TESTED_CODEX_BUILD.to_owned(),
        toolkit_available: supported && archive_path(app).is_ok_and(|p| archive_verified(&p)),
        codex_app_found: codex_app.is_some(),
        codex_app_path: codex_app.unwrap_or_default(),
        installed: supported && root.join(INSTALL_MARKER).is_file(),
        install_root: root.to_string_lossy().into_owned(),
        shortcut_path: root.join(SHORTCUT_NAME).to_string_lossy().into_owned(),
        blocked_code: None,
    };
    state.blocked_code = blocked_code(FastUiAction::Install, &state).map(str::to_owned);
    Ok(state)
}

/// Install location of the official Codex client via `Get-AppxPackage` (Windows only). The
/// command is a fixed string; no user input is interpolated into it.
async fn codex_app_path() -> Option<String> {
    let query = "(Get-AppxPackage -Name OpenAI.Codex | Sort-Object Version -Descending \
                 | Select-Object -First 1).InstallLocation";
    let spec = CommandSpec::new(
        "powershell.exe",
        [
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            query,
        ],
    )
    .with_timeout(APPX_TIMEOUT);
    let out = process::run(&spec).await.ok()?;
    if !out.success() {
        log::info!("codex appx probe exited with {:?}", out.exit_code);
        return None;
    }
    let path = out.stdout.trim();
    // The application itself lives in the package's `app` sub-directory (see install.ps1).
    if path.is_empty() || !Path::new(path).join("app").is_dir() {
        return None;
    }
    Some(path.to_owned())
}

// ---------------------------------------------------------------------------
// Plan — shown before anything runs (hard rule 4)
// ---------------------------------------------------------------------------

/// The exact command for `action`. Pure: paths in, spec out, so the string shown to the user
/// and the spec that is executed can never drift apart.
pub fn command_for(
    action: FastUiAction,
    toolkit: &Path,
    root: &Path,
    reinstall: bool,
) -> CommandSpec {
    let mut args = vec![
        "-NoProfile".to_owned(),
        "-NonInteractive".to_owned(),
        "-ExecutionPolicy".to_owned(),
        "Bypass".to_owned(),
        "-File".to_owned(),
        script_path(action, toolkit, root)
            .to_string_lossy()
            .into_owned(),
    ];
    let mut timeout = MAINTENANCE_TIMEOUT;
    if action == FastUiAction::Install {
        timeout = INSTALL_TIMEOUT;
        args.push("-OutputRoot".to_owned());
        args.push(root.to_string_lossy().into_owned());
        if reinstall {
            args.push("-Force".to_owned());
        }
    }
    CommandSpec::new("powershell.exe", args).with_timeout(timeout)
}

/// Builds the plan the UI must show before [`start`] may run `action`.
pub async fn plan(app: &AppHandle, action: FastUiAction) -> AppResult<FastUiPlan> {
    let state = status(app).await?;
    if let Some(code) = blocked_code(action, &state) {
        return Err(AppError::Unsupported(code.to_owned()));
    }
    let reinstall = state.installed && action == FastUiAction::Install;
    let spec = command_for(action, &toolkit_dir(app)?, &install_root()?, reinstall);
    Ok(FastUiPlan {
        action,
        display_command: spec.display(),
        install_root: state.install_root,
        shortcut_path: state.shortcut_path,
        toolkit_version: TOOLKIT_VERSION.to_owned(),
        toolkit_sha256: TOOLKIT_SHA256.to_owned(),
        reinstall,
    })
}

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

/// Extracts the verified archive into the app cache dir with the system `tar` (shipped with
/// Windows since build 17063; this app requires 19041+). Always re-extracts, so the files that
/// run are the ones the pinned hash covers.
async fn extract_toolkit(app: &AppHandle) -> AppResult<PathBuf> {
    let archive = archive_path(app)?;
    if !archive_verified(&archive) {
        return Err(AppError::Unsupported(BLOCKED_TOOLKIT_MISSING.to_owned()));
    }
    let toolkit = toolkit_dir(app)?;
    let dest = toolkit
        .parent()
        .ok_or_else(|| AppError::Other("toolkit cache path has no parent".to_owned()))?
        .to_path_buf();
    tokio::fs::create_dir_all(&dest).await?;
    let spec = CommandSpec::new(
        "tar",
        [
            "-xf".to_owned(),
            archive.to_string_lossy().into_owned(),
            "-C".to_owned(),
            dest.to_string_lossy().into_owned(),
        ],
    )
    .with_timeout(EXTRACT_TIMEOUT);
    let out = process::run(&spec).await?;
    if !out.success() {
        return Err(AppError::CommandFailed {
            program: "tar".to_owned(),
            code: out.exit_code,
            stderr_tail: redact_secrets(out.stderr.trim()),
        });
    }
    if !toolkit.join("install.ps1").is_file() {
        return Err(AppError::Other(
            "toolkit archive did not contain install.ps1".to_owned(),
        ));
    }
    Ok(toolkit)
}

/// Runs a confirmed plan and returns immediately with the job id. Output arrives on
/// `fastui://output`, the outcome on `fastui://done`. The plan is re-derived here and compared
/// with the one the UI hands back, so the show-before-run guarantee does not rest on the
/// webview alone.
pub async fn start(
    app: AppHandle,
    jobs: &JobRegistry,
    confirmed: FastUiPlan,
) -> AppResult<FastUiJob> {
    let action = confirmed.action;
    let expected = plan(&app, action).await?;
    if expected.display_command != confirmed.display_command {
        return Err(AppError::InvalidInput(
            "fast-ui plan does not match the current machine state".to_owned(),
        ));
    }
    let root = install_root()?;
    let toolkit = extract_toolkit(&app).await?;
    let spec = command_for(action, &toolkit, &root, expected.reinstall);
    let script = script_path(action, &toolkit, &root);
    if !script.is_file() {
        return Err(AppError::Unsupported(BLOCKED_NOT_INSTALLED.to_owned()));
    }

    let job_id = uuid::Uuid::new_v4().to_string();
    let cancel = jobs.register(&job_id);
    log::info!("codex fast-ui job {job_id} ({action:?}) started");
    tokio::spawn(run_job(
        app,
        jobs.clone(),
        job_id.clone(),
        spec,
        expected.display_command,
        cancel,
    ));
    Ok(FastUiJob { job_id, action })
}

fn emit_output(app: &AppHandle, job_id: &str, stream: OutputStream, line: String) {
    let _ = app.emit(
        FAST_UI_OUTPUT,
        InstallOutputEvent {
            job_id: job_id.to_owned(),
            stream,
            line,
        },
    );
}

/// Body of the spawned task: stream output, then report `fastui://done` and free the job.
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
            InstallDoneEvent {
                job_id: job_id.clone(),
                success: output.success(),
                exit_code: output.exit_code,
                duration_ms: output.duration_ms,
                cancelled,
                timed_out: output.timed_out,
            }
        }
        Err(e) => {
            let message = redact_secrets(&e.to_string());
            log::warn!("codex fast-ui job {job_id} could not run: {message}");
            emit_output(&app, &job_id, OutputStream::System, message);
            InstallDoneEvent {
                job_id: job_id.clone(),
                success: false,
                exit_code: None,
                duration_ms: elapsed_ms(started),
                cancelled: false,
                timed_out: false,
            }
        }
    };
    log::info!(
        "codex fast-ui job {job_id} finished: success={} exit_code={:?} ({} ms)",
        done.success,
        done.exit_code,
        done.duration_ms
    );
    jobs.finish(&job_id);
    let _ = app.emit(FAST_UI_DONE, &done);
}

fn elapsed_ms(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_ACTIONS: [FastUiAction; 3] = [
        FastUiAction::Install,
        FastUiAction::Verify,
        FastUiAction::Restore,
    ];

    fn paths() -> (PathBuf, PathBuf) {
        (
            PathBuf::from(r"C:\cache\codex-fast-ui\v\CodexFastUI-Minimal-2026.07.30"),
            PathBuf::from(r"C:\Users\me\AppData\Local\SeedRouter\CodexFastUI"),
        )
    }

    /// A machine on which installing would work; individual facts are flipped per test.
    fn ready() -> FastUiStatus {
        FastUiStatus {
            supported: true,
            toolkit_version: TOOLKIT_VERSION.to_owned(),
            tested_codex_build: TESTED_CODEX_BUILD.to_owned(),
            toolkit_available: true,
            codex_app_found: true,
            codex_app_path: r"C:\Program Files\WindowsApps\OpenAI.Codex".to_owned(),
            installed: false,
            install_root: String::new(),
            shortcut_path: String::new(),
            blocked_code: None,
        }
    }

    #[test]
    fn install_root_ends_in_the_documented_sub_directory() {
        let root = install_root().expect("local app data");
        let tail: PathBuf = INSTALL_SUBDIR.iter().collect();
        assert!(root.ends_with(&tail), "{}", root.display());
    }

    #[test]
    fn blocked_code_reports_the_first_blocking_reason() {
        // Not Windows outranks everything else.
        let elsewhere = FastUiStatus {
            supported: false,
            toolkit_available: false,
            codex_app_found: false,
            ..ready()
        };
        assert_eq!(
            blocked_code(FastUiAction::Install, &elsewhere),
            Some(BLOCKED_NOT_WINDOWS)
        );
        // A missing or tampered archive blocks every action.
        let no_toolkit = FastUiStatus {
            toolkit_available: false,
            installed: true,
            ..ready()
        };
        for action in ALL_ACTIONS {
            assert_eq!(
                blocked_code(action, &no_toolkit),
                Some(BLOCKED_TOOLKIT_MISSING),
                "{action:?}"
            );
        }
        // Install needs the official client; verify and restore need a previous install.
        let no_client = FastUiStatus {
            codex_app_found: false,
            ..ready()
        };
        assert_eq!(
            blocked_code(FastUiAction::Install, &no_client),
            Some(BLOCKED_CODEX_APP_MISSING)
        );
        for action in [FastUiAction::Verify, FastUiAction::Restore] {
            assert_eq!(
                blocked_code(action, &ready()),
                Some(BLOCKED_NOT_INSTALLED),
                "{action:?}"
            );
        }
    }

    #[test]
    fn blocked_code_allows_a_ready_machine() {
        assert_eq!(blocked_code(FastUiAction::Install, &ready()), None);
        let installed = FastUiStatus {
            codex_app_found: false,
            installed: true,
            ..ready()
        };
        for action in [FastUiAction::Verify, FastUiAction::Restore] {
            assert_eq!(blocked_code(action, &installed), None, "{action:?}");
        }
    }

    #[test]
    fn install_command_runs_the_toolkit_script_into_our_own_output_root() {
        let (toolkit, root) = paths();
        let spec = command_for(FastUiAction::Install, &toolkit, &root, false);
        assert_eq!(spec.program, "powershell.exe");
        assert_eq!(spec.timeout, INSTALL_TIMEOUT);
        let display = spec.display();
        assert!(display.contains("-ExecutionPolicy Bypass"), "{display}");
        assert!(display.contains("install.ps1"), "{display}");
        assert!(display.contains("-OutputRoot"), "{display}");
        assert!(
            display.contains(r"AppData\Local\SeedRouter\CodexFastUI"),
            "the copy never lands in the official install location: {display}"
        );
        assert!(
            !display.contains("-Force"),
            "a first install must not force"
        );
    }

    #[test]
    fn reinstall_adds_force_only_for_install() {
        let (toolkit, root) = paths();
        let install = command_for(FastUiAction::Install, &toolkit, &root, true);
        assert!(install.display().contains("-Force"));
        for action in [FastUiAction::Verify, FastUiAction::Restore] {
            let display = command_for(action, &toolkit, &root, true).display();
            assert!(!display.contains("-Force"), "{action:?}: {display}");
            assert!(!display.contains("-OutputRoot"), "{action:?}: {display}");
        }
    }

    #[test]
    fn verify_and_restore_run_the_scripts_inside_the_install_root() {
        let (toolkit, root) = paths();
        // The path separator differs per host, so match on the parts, not on the joined string.
        let verify = command_for(FastUiAction::Verify, &toolkit, &root, false).display();
        assert!(
            verify.contains("CodexFastUI") && verify.ends_with("verify.ps1"),
            "{verify}"
        );
        assert!(!verify.contains("install.ps1"), "{verify}");
        let restore = command_for(FastUiAction::Restore, &toolkit, &root, false).display();
        assert!(restore.ends_with("restore.ps1"), "{restore}");
        assert_eq!(
            command_for(FastUiAction::Verify, &toolkit, &root, false).timeout,
            MAINTENANCE_TIMEOUT
        );
    }

    #[test]
    fn no_command_ever_touches_a_config_or_system_directory() {
        let (toolkit, root) = paths();
        for action in ALL_ACTIONS {
            let display = command_for(action, &toolkit, &root, true).display();
            for forbidden in [".codex", ".claude", ".cc-switch", "WindowsApps"] {
                assert!(
                    !display.contains(forbidden),
                    "{action:?} mentions {forbidden}"
                );
            }
        }
    }

    #[test]
    fn blocking_codes_are_fully_qualified_guide_keys() {
        for code in [
            BLOCKED_NOT_WINDOWS,
            BLOCKED_TOOLKIT_MISSING,
            BLOCKED_CODEX_APP_MISSING,
            BLOCKED_NOT_INSTALLED,
        ] {
            assert!(code.starts_with("guide:fastui.blocked."), "{code}");
        }
    }

    #[test]
    fn pinned_hash_is_a_lower_case_sha256() {
        assert_eq!(TOOLKIT_SHA256.len(), 64);
        assert!(TOOLKIT_SHA256
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn sha256_file_matches_the_shared_helper() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("blob.bin");
        std::fs::write(&file, b"seedrouter").expect("write");
        assert_eq!(
            sha256_file(&file).expect("hash"),
            net::sha256_hex(b"seedrouter")
        );
    }
}
