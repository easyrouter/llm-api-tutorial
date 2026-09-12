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
use crate::{checks, net, platform};

/// Archive shipped as `resources/codex-fast-ui/<name>` (see `tauri.conf.json`).
pub const TOOLKIT_ARCHIVE: &str = "CodexFastUI-Minimal-2026.09.12.zip";
/// Top-level directory inside the archive.
pub const TOOLKIT_DIR_NAME: &str = "CodexFastUI-Minimal-2026.09.12";
/// Toolkit version as the toolkit itself reports it (`patch-install.json` → `toolkitVersion`).
pub const TOOLKIT_VERSION: &str = "2026.09.12-minimal";
/// SHA-256 of [`TOOLKIT_ARCHIVE`]. Replacing the toolkit means replacing this constant.
pub const TOOLKIT_SHA256: &str = "42033cdeb7f415fd8bbce6e2d64ab1edc6dbc2dc3c3ee3a6203ab4afe004b6d1";
/// Codex Windows builds the toolkit was tested against, shown verbatim in the UI caveat.
///
/// `26.903.8094.0` is the offline MSIX (`ChatGPT-x64.msix`; Electron package 26.903.61454,
/// build 8378) and `26.908.4834.0` the Microsoft Store / auto-updated client (26.908.40834,
/// build 8881). Newer builds may not be patchable — the patcher refuses to guess and stops
/// instead (toolkit README).
pub const TESTED_CODEX_BUILD: &str = "OpenAI.Codex 26.903.8094.0 / 26.908.4834.0";

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

/// Script `action` runs — always the one from the freshly extracted, hash-verified toolkit.
/// `install.ps1` also drops copies of `verify.ps1` / `restore.ps1` into the install root for
/// manual use (what the toolkit README documents), but the app does not run those: a root
/// created by an older toolkit keeps the older scripts, and the ones shipped before
/// `2026.09.12-minimal` failed on start-up when launched without `-Root` — so the copy was the
/// one thing the Verify / Restore buttons could not rely on. The install root is passed
/// explicitly instead ([`command_for`]).
fn script_path(action: FastUiAction, toolkit: &Path) -> PathBuf {
    let name = match action {
        FastUiAction::Install => "install.ps1",
        FastUiAction::Verify => "verify.ps1",
        FastUiAction::Restore => "restore.ps1",
    };
    toolkit.join(name)
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

/// Install location of the official Codex client (Windows only). Shares the environment
/// check's `Get-AppxPackage` probe: a second one here meant a second timeout to keep in sync,
/// and 30 s was not enough on a package cache still cold from installing the client — which
/// the caller could only read as "the client is not installed".
async fn codex_app_path() -> Option<String> {
    let app = checks::codex_app::windows_appx().await?;
    // The application itself lives in the package's `app` sub-directory (see install.ps1).
    if app.path.is_empty() || !Path::new(&app.path).join("app").is_dir() {
        return None;
    }
    Some(app.path)
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
        script_path(action, toolkit).to_string_lossy().into_owned(),
    ];
    let root_arg = root.to_string_lossy().into_owned();
    let mut timeout = MAINTENANCE_TIMEOUT;
    match action {
        FastUiAction::Install => {
            timeout = INSTALL_TIMEOUT;
            args.push("-OutputRoot".to_owned());
            args.push(root_arg);
            if reinstall {
                args.push("-Force".to_owned());
            }
        }
        // The scripts default `-Root` to their own directory; that is the install root only
        // for the copies left there by `install.ps1`, so name it explicitly.
        FastUiAction::Verify | FastUiAction::Restore => {
            args.push("-Root".to_owned());
            args.push(root_arg);
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
    let script = script_path(action, &toolkit);
    if !script.is_file() {
        return Err(AppError::Other(format!(
            "toolkit archive did not contain {}",
            script.display()
        )));
    }
    // Verify / restore act on an existing copy; `plan` already refused when there is none, but
    // the marker can disappear between the plan and the click.
    if action != FastUiAction::Install && !root.join(INSTALL_MARKER).is_file() {
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
            PathBuf::from(r"C:\cache\codex-fast-ui\v\CodexFastUI-Minimal-2026.09.12"),
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

    /// Verify / restore run the scripts of the freshly extracted (hash-verified) toolkit and
    /// name the install root explicitly. Running the copies `install.ps1` left in the root would
    /// tie the buttons to whatever toolkit made that root — and the copies shipped before
    /// `2026.09.12-minimal` failed on start-up when launched without `-Root`.
    #[test]
    fn verify_and_restore_run_the_toolkit_scripts_against_the_install_root() {
        let (toolkit, root) = paths();
        for (action, name) in [
            (FastUiAction::Verify, "verify.ps1"),
            (FastUiAction::Restore, "restore.ps1"),
        ] {
            let spec = command_for(action, &toolkit, &root, false);
            assert_eq!(spec.timeout, MAINTENANCE_TIMEOUT, "{action:?}");
            let script = spec
                .args
                .iter()
                .find(|a| a.ends_with(name))
                .unwrap_or_else(|| panic!("{action:?} runs {name}: {:?}", spec.args));
            assert!(
                script.starts_with(&*toolkit.to_string_lossy()),
                "{action:?} must run the extracted toolkit's script, got {script}"
            );
            let root_pos = spec
                .args
                .iter()
                .position(|a| a == "-Root")
                .unwrap_or_else(|| panic!("{action:?} names the install root: {:?}", spec.args));
            assert_eq!(
                spec.args.get(root_pos + 1),
                Some(&root.to_string_lossy().into_owned())
            );
            let display = spec.display();
            assert!(!display.contains("install.ps1"), "{action:?}: {display}");
            assert!(!display.contains("-OutputRoot"), "{action:?}: {display}");
        }
    }

    /// `toolkit_dir` / `script_path` assume the archive unpacks to `TOOLKIT_DIR_NAME/`; a
    /// re-pin that renames the folder but not the constant would fail at run time only. Zip
    /// stores entry names uncompressed, so the archive bytes must contain the path.
    #[test]
    fn the_shipped_archive_unpacks_to_the_pinned_directory() {
        let archive = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(EXTRACT_DIR)
            .join(TOOLKIT_ARCHIVE);
        let bytes = std::fs::read(&archive).expect("read the archive");
        for script in [
            "install.ps1",
            "verify.ps1",
            "restore.ps1",
            "patch-fast-ui.mjs",
        ] {
            let entry = format!("{TOOLKIT_DIR_NAME}/{script}");
            assert!(
                bytes
                    .windows(entry.len())
                    .any(|window| window == entry.as_bytes()),
                "{} must contain the entry {entry}",
                archive.display()
            );
        }
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

    /// The archive only reaches users if `bundle.resources` lists it. A merge once dropped
    /// that entry; local builds kept working from a stale staged copy, so a clean CI build
    /// would have shipped the feature with no toolkit at all.
    #[test]
    fn the_toolkit_archive_is_declared_as_a_bundle_resource() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        let resources = conf["bundle"]["resources"]
            .as_array()
            .expect("bundle.resources array");
        let prefix = format!("resources/{EXTRACT_DIR}/");
        let declared = resources
            .iter()
            .filter_map(serde_json::Value::as_str)
            .any(|entry| {
                entry.starts_with(&prefix)
                    && (entry.ends_with(TOOLKIT_ARCHIVE) || entry.ends_with("*.zip"))
            });
        assert!(
            declared,
            "bundle.resources must ship {prefix}{TOOLKIT_ARCHIVE}: {resources:?}"
        );
    }

    /// Replacing the toolkit means replacing [`TOOLKIT_SHA256`]; a stale hash fails every
    /// extraction at run time (`toolkit_missing`) instead of here.
    #[test]
    fn the_shipped_archive_matches_the_pinned_hash() {
        let archive = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(EXTRACT_DIR)
            .join(TOOLKIT_ARCHIVE);
        assert_eq!(
            sha256_file(&archive).expect("hash the archive"),
            TOOLKIT_SHA256
        );
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
