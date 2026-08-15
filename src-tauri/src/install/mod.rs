//! M2 — installation. Nothing is executed without the frontend first showing
//! `InstallPlan.display_command` and the user confirming.
//!
//! TODO(impl):
//! - `plan`: build the `InstallPlan` for a target
//!     * Node      → no command; plan carries the download page of the chosen mirror
//!                   (macOS with Homebrew detected: `brew install node@<lts>` alternative)
//!     * Codex / ClaudeCode → `npm install -g <pkg>` with `--registry <chosen mirror>`;
//!                   `requires_admin=false` when npm prefix is user-writable, else `true`
//!                   with a hint to switch prefix (never elevate silently — PRD #7)
//!     * CcSwitch  → no npm command; plan carries the release download URL (see `cc_switch`)
//! - `start`: spawn `process::run_streaming` in a tokio task, emit `install://output` per line
//!   and `install://done` at the end; register a cancel sender in `JobRegistry`
//! - `cancel`: flip the watch sender for the job
//! - `cc_switch::latest_release`: query `releases_api` (or intranet mirror), pick the asset
//!   for platform/arch (Windows: *_x64-setup.exe / .msi; macOS: *_aarch64.dmg / *_x64.dmg),
//!   read SHA-256 from a sibling `.sha256` asset or release notes when available
//! - `download_installer`: `net::download` into app cache dir emitting `download://progress`

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::AppHandle;
use tokio::sync::watch;

use crate::error::{AppError, AppResult};
use crate::models::{
    AppConfig, CcSwitchRelease, DownloadRequest, DownloadResult, InstallJob, InstallPlan,
    InstallTarget, MirrorChoice, Platform,
};

/// Tracks running install jobs so they can be cancelled.
#[derive(Debug, Default)]
pub struct JobRegistry {
    inner: Mutex<HashMap<String, watch::Sender<bool>>>,
}

impl JobRegistry {
    pub fn register(&self, job_id: &str) -> watch::Receiver<bool> {
        let (tx, rx) = watch::channel(false);
        if let Ok(mut map) = self.inner.lock() {
            map.insert(job_id.to_owned(), tx);
        }
        rx
    }

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

    pub fn finish(&self, job_id: &str) {
        if let Ok(mut map) = self.inner.lock() {
            map.remove(job_id);
        }
    }
}

pub fn plan(
    target: InstallTarget,
    config: &AppConfig,
    mirrors: &MirrorChoice,
) -> AppResult<InstallPlan> {
    let _ = (target, config, mirrors);
    Err(AppError::Other("install::plan not implemented".into()))
}

pub async fn start(app: AppHandle, jobs: &JobRegistry, plan: InstallPlan) -> AppResult<InstallJob> {
    let _ = (app, jobs, plan);
    Err(AppError::Other("install::start not implemented".into()))
}

pub async fn latest_cc_switch_release(
    client: &reqwest::Client,
    config: &AppConfig,
    platform: Platform,
    arch: &str,
) -> AppResult<CcSwitchRelease> {
    let _ = (client, config, platform, arch);
    Err(AppError::Other(
        "install::latest_cc_switch_release not implemented".into(),
    ))
}

pub async fn download_installer(
    app: AppHandle,
    client: &reqwest::Client,
    req: DownloadRequest,
) -> AppResult<DownloadResult> {
    let _ = (app, client, req);
    Err(AppError::Other(
        "install::download_installer not implemented".into(),
    ))
}
