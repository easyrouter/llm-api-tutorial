//! M1 — environment checks. Each check is an independent async function returning a
//! [`CheckResult`]; `run_all` executes them concurrently and streams each result on the
//! `checks://progress` event as it completes, then returns the aggregated [`EnvSnapshot`].
//!
//! Sub-modules (TODO(impl) — one file each):
//! - `os`        : platform + minimum version (Requirements.os) — Fail below minimum
//! - `node`      : `node --version` >= node_min_version, `npm --version`; distinguishes
//!                 "not installed" from "installed but not on PATH" (see platform / npm prefix)
//! - `tools`     : per `ToolSpec` (codex, claude-code): binary on PATH → version; else look in
//!                 npm global bin → `on_path=false` (guide fault E: PATH not refreshed)
//! - `cc_switch` : app installed? (Windows: uninstall registry keys / Program Files / LocalAppData;
//!                 macOS: /Applications/CC Switch.app, ~/Applications) and data dir present?
//! - `env_vars`  : for each `env_vars_to_inspect`: present in this process? masked value;
//!                 sources — Windows HKCU/HKLM Environment; macOS shell rc files (~/.zshrc,
//!                 ~/.zprofile, ~/.zshenv, ~/.bash_profile, ~/.bashrc, ~/.profile) with line
//!                 numbers; `launchctl getenv`. Any *_API_KEY / *_BASE_URL present → Warn with an
//!                 `Instructions` fix (tool must NOT modify env vars — PRD #17)
//! - `network`   : probes npm registries, GitHub releases API, company gateway base URL
//!
//! Codes (frontend key = `checks.<code>`), keep in sync with `src/i18n/locales/*.json`:
//!   os.ok, os.too_old, os.unknown
//!   node.ok, node.missing, node.too_old, node.not_on_path
//!   npm.ok, npm.missing
//!   tool.ok, tool.missing, tool.not_on_path            (params: tool, version, path)
//!   cc_switch.ok, cc_switch.missing, cc_switch.data_only
//!   env_vars.clean, env_vars.conflicts                 (params: names)
//!   network.ok, network.mirror_only, network.unreachable (params: target)

use std::time::Instant;

use tauri::{AppHandle, Emitter};

use crate::events::CHECK_PROGRESS;
use crate::models::{
    AppConfig, CheckId, CheckResult, CheckStatus, EnvSnapshot, EnvVarFinding, FixAction, Params,
    ToolInfo,
};

/// Shared context passed to every check.
#[derive(Debug, Clone)]
pub struct CheckContext {
    pub config: AppConfig,
    pub http: reqwest::Client,
}

/// Runs a single check by id.
pub async fn run_one(id: CheckId, ctx: &CheckContext) -> CheckResult {
    let started = Instant::now();
    // TODO(impl): dispatch to sub-module implementations
    let _ = ctx;
    CheckResult {
        id,
        status: CheckStatus::Skipped,
        code: "not_implemented".into(),
        params: Params::new(),
        details: Vec::new(),
        fixes: vec![FixAction::Rerun],
        duration_ms: elapsed_ms(started),
    }
}

/// Runs all checks concurrently, streaming each `CheckResult` on `checks://progress`.
pub async fn run_all(app: Option<&AppHandle>, ctx: &CheckContext) -> EnvSnapshot {
    let mut checks = Vec::with_capacity(CheckId::ALL.len());
    for id in CheckId::ALL {
        let result = run_one(id, ctx).await;
        if let Some(app) = app {
            let _ = app.emit(CHECK_PROGRESS, &result);
        }
        checks.push(result);
    }
    // TODO(impl): populate `tools` and `env_vars` from the respective sub-modules
    let tools: Vec<ToolInfo> = Vec::new();
    let env_vars: Vec<EnvVarFinding> = Vec::new();
    let overall = overall_status(&checks);
    EnvSnapshot {
        os: crate::platform::os_info(),
        tools,
        env_vars,
        checks,
        overall,
        generated_at: chrono::Utc::now().to_rfc3339(),
    }
}

/// Worst status wins (Fail > Warn > Pass); `Skipped` is ignored.
pub fn overall_status(checks: &[CheckResult]) -> CheckStatus {
    checks
        .iter()
        .map(|c| c.status)
        .filter(|s| *s != CheckStatus::Skipped)
        .max()
        .unwrap_or(CheckStatus::Skipped)
}

pub(crate) fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cr(id: CheckId, status: CheckStatus) -> CheckResult {
        CheckResult {
            id,
            status,
            code: String::new(),
            params: Params::new(),
            details: vec![],
            fixes: vec![],
            duration_ms: 0,
        }
    }

    #[test]
    fn overall_is_worst_non_skipped() {
        let checks = vec![
            cr(CheckId::Os, CheckStatus::Pass),
            cr(CheckId::Node, CheckStatus::Warn),
            cr(CheckId::Npm, CheckStatus::Skipped),
        ];
        assert_eq!(overall_status(&checks), CheckStatus::Warn);
        assert_eq!(overall_status(&[]), CheckStatus::Skipped);
    }
}
