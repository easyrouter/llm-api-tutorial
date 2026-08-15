//! M1 — environment checks. Each check is an independent async function returning a
//! [`Verdict`] (status + code + params + details + fixes); `run_one` stamps id and duration,
//! `run_all` executes every check concurrently, streams each [`CheckResult`] on the
//! `checks://progress` event as it completes and returns the aggregated [`EnvSnapshot`].
//!
//! Sub-modules (one file each):
//! - `os`        : platform + minimum version (Requirements.os) — Fail below minimum
//! - `node`      : `node --version` >= node_min_version, `npm --version`; distinguishes
//!                 "not installed" from "installed but not on PATH" (common install dirs)
//! - `tools`     : per `ToolSpec` (codex, claude-code): binary on PATH → version; else look in
//!                 npm global bin → `on_path=false` (guide fault E: PATH not refreshed)
//! - `cc_switch` : app installed? (Windows: uninstall registry keys / LocalAppData;
//!                 macOS: /Applications/CC Switch.app, ~/Applications) and data dir present?
//! - `env_vars`  : for each `env_vars_to_inspect`: present in this process? masked value;
//!                 sources — Windows HKCU/HKLM Environment; macOS shell rc files with line
//!                 numbers; `launchctl getenv`. Any *_API_KEY / *_BASE_URL / *_AUTH_TOKEN /
//!                 *_API_BASE present → Warn with an `Instructions` fix (the tool must NOT
//!                 modify env vars — PRD #17); proxies are informational only
//! - `network`   : probes npm registries, the service gateway origin, GitHub releases API
//!
//! Codes (frontend key = `checks:<code>`), keep in sync with `src/i18n/locales/*/checks.json`:
//!   os.ok, os.too_old, os.unknown                       (params: platform, version, build,
//!                                                        arch, shell, minBuild, minVersion, label)
//!   node.ok, node.missing, node.too_old, node.not_on_path, node.broken
//!                                                       (params: version, minVersion,
//!                                                        recommended, path, dir, exitCode)
//!   npm.ok, npm.missing, npm.broken                     (params: version, path, exitCode)
//!   tool.ok, tool.missing, tool.not_on_path, tool.broken, tool.not_configured
//!                                                       (params: tool, toolId, version, path,
//!                                                        dir, exitCode)
//!   cc_switch.ok, cc_switch.missing, cc_switch.data_only (params: version, path, dataDir)
//!   env_vars.clean, env_vars.conflicts                  (params: names)
//!   network.ok, network.mirror_only, network.unreachable (params: target)
//!   check.internal_error                                (a check task could not complete)
//!
//! `FixAction::Instructions` codes are fully qualified (the UI renders `t(code, params)`):
//!   checks:node.instructions.not_on_path                (params: path, dir)
//!   checks:tool.instructions.not_on_path                (params: tool, dir)
//!   checks:env_vars.instructions.windows                (params: names)
//!   checks:env_vars.instructions.macos                  (params: names)
//! `FixAction::OpenUrl` label codes: node_download, cc_switch_download.

pub mod cc_switch;
pub mod env_vars;
pub mod network;
pub mod node;
pub mod os;
pub mod tools;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::events::CHECK_PROGRESS;
use crate::models::{
    AppConfig, CheckId, CheckResult, CheckStatus, EnvSnapshot, EnvVarFinding, FixAction, Params,
    ToolId, ToolInfo,
};
use crate::process::{self, CommandSpec};
use crate::redact::{redact_secrets, tail_redacted};

pub use env_vars::{inspect_env_var_names, inspect_env_vars};
pub use tools::detect_tool;

/// Shared context passed to every check.
#[derive(Debug, Clone)]
pub struct CheckContext {
    pub config: AppConfig,
    pub http: reqwest::Client,
}

/// Outcome of a check before it is stamped with its id and duration (see [`CheckResult`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub status: CheckStatus,
    /// i18n code without namespace, e.g. `node.too_old`.
    pub code: String,
    pub params: Params,
    pub details: Vec<String>,
    pub fixes: Vec<FixAction>,
}

impl Verdict {
    pub fn new(status: CheckStatus, code: impl Into<String>) -> Self {
        Self {
            status,
            code: code.into(),
            params: Params::new(),
            details: Vec::new(),
            fixes: Vec::new(),
        }
    }

    pub fn pass(code: impl Into<String>) -> Self {
        Self::new(CheckStatus::Pass, code)
    }

    pub fn warn(code: impl Into<String>) -> Self {
        Self::new(CheckStatus::Warn, code)
    }

    pub fn fail(code: impl Into<String>) -> Self {
        Self::new(CheckStatus::Fail, code)
    }

    /// Adds an interpolation parameter (builder style).
    #[must_use]
    pub fn param(mut self, key: &str, value: impl Into<String>) -> Self {
        self.params.insert(key.to_owned(), value.into());
        self
    }

    /// Adds an optional interpolation parameter; `None` is skipped.
    #[must_use]
    pub fn param_opt(self, key: &str, value: Option<impl Into<String>>) -> Self {
        match value {
            Some(v) => self.param(key, v),
            None => self,
        }
    }

    /// Adds a non-secret fact shown verbatim.
    #[must_use]
    pub fn detail(mut self, line: impl Into<String>) -> Self {
        self.details.push(line.into());
        self
    }

    /// Adds a remediation the UI can offer.
    #[must_use]
    pub fn fix(mut self, fix: FixAction) -> Self {
        self.fixes.push(fix);
        self
    }

    /// Adds several remediations.
    #[must_use]
    pub fn fixes(mut self, fixes: impl IntoIterator<Item = FixAction>) -> Self {
        self.fixes.extend(fixes);
        self
    }

    /// Stamps the verdict with its check id and elapsed time.
    pub fn into_result(self, id: CheckId, started: Instant) -> CheckResult {
        CheckResult {
            id,
            status: self.status,
            code: self.code,
            params: self.params,
            details: self.details,
            fixes: self.fixes,
            duration_ms: elapsed_ms(started),
        }
    }
}

/// Side product of a check that feeds the [`EnvSnapshot`] besides the [`CheckResult`].
#[derive(Debug, Clone)]
enum Extra {
    None,
    Tool(ToolInfo),
    EnvVars(Vec<EnvVarFinding>),
}

/// Runs the check for `id` and returns its verdict plus any snapshot side product.
async fn run_inner(id: CheckId, ctx: &CheckContext) -> (Verdict, Extra) {
    let cfg = &ctx.config;
    match id {
        CheckId::Os => (os::check(&cfg.requirements.os), Extra::None),
        CheckId::Node => (
            node::check_node(&cfg.requirements, &cfg.mirrors).await,
            Extra::None,
        ),
        CheckId::Npm => (node::check_npm(&cfg.mirrors).await, Extra::None),
        CheckId::Codex => run_tool(ToolId::Codex, cfg).await,
        CheckId::ClaudeCode => run_tool(ToolId::ClaudeCode, cfg).await,
        CheckId::CcSwitch => (cc_switch::check(&cfg.cc_switch), Extra::None),
        CheckId::EnvVars => {
            let (verdict, findings) = env_vars::check(cfg).await;
            (verdict, Extra::EnvVars(findings))
        }
        CheckId::NetworkNpm => (
            network::check_npm(&ctx.http, &cfg.mirrors).await,
            Extra::None,
        ),
        CheckId::NetworkGateway => (
            network::check_gateway(&ctx.http, &cfg.gateway, &cfg.mirrors).await,
            Extra::None,
        ),
        CheckId::NetworkGithub => (
            network::check_github(&ctx.http, &cfg.cc_switch, &cfg.mirrors).await,
            Extra::None,
        ),
    }
}

/// Tool check for `id`; `tool.not_configured` (Skipped) when the preset has no such tool.
async fn run_tool(id: ToolId, cfg: &AppConfig) -> (Verdict, Extra) {
    match cfg.tools.iter().find(|t| t.id == id) {
        Some(spec) => {
            let (verdict, info) = tools::check(spec).await;
            (verdict, Extra::Tool(info))
        }
        None => (
            Verdict::new(CheckStatus::Skipped, "tool.not_configured")
                .param("toolId", tool_id_str(id)),
            Extra::None,
        ),
    }
}

/// Wire name of a [`ToolId`] (`codex`, `claude-code`).
pub fn tool_id_str(id: ToolId) -> &'static str {
    match id {
        ToolId::Codex => "codex",
        ToolId::ClaudeCode => "claude-code",
    }
}

/// Runs a single check by id.
pub async fn run_one(id: CheckId, ctx: &CheckContext) -> CheckResult {
    let started = Instant::now();
    let (verdict, _) = run_inner(id, ctx).await;
    verdict.into_result(id, started)
}

/// Result used when a spawned check task did not complete (panic/abort) — never expected.
fn internal_error(id: CheckId, started: Instant) -> CheckResult {
    Verdict::fail("check.internal_error")
        .fix(FixAction::Rerun)
        .into_result(id, started)
}

/// Emits a finished check on `checks://progress` (no-op without an app handle).
fn emit_progress(app: Option<&AppHandle>, result: &CheckResult) {
    if let Some(app) = app {
        if let Err(e) = app.emit(CHECK_PROGRESS, result) {
            log::warn!("cannot emit {CHECK_PROGRESS}: {e}");
        }
    }
}

/// Runs all checks concurrently, streaming each `CheckResult` on `checks://progress` as it
/// completes. Results keep the order of [`CheckId::ALL`]; `tools` and `env_vars` of the
/// snapshot are filled from the tool and env-var checks.
///
/// Each check runs in its own tokio task and reports back over a channel; emitting happens
/// here, on the borrowed handle (the handle is deliberately never cloned into tasks).
pub async fn run_all(app: Option<&AppHandle>, ctx: &CheckContext) -> EnvSnapshot {
    let started = Instant::now();
    let ctx = Arc::new(ctx.clone());
    let (tx, mut rx) = mpsc::unbounded_channel::<(CheckId, CheckResult, Extra)>();
    for id in CheckId::ALL {
        let ctx = Arc::clone(&ctx);
        let tx = tx.clone();
        tokio::spawn(async move {
            let started = Instant::now();
            let (verdict, extra) = run_inner(id, &ctx).await;
            let _ = tx.send((id, verdict.into_result(id, started), extra));
        });
    }
    drop(tx);

    let mut slots: Vec<Option<(CheckResult, Extra)>> = vec![None; CheckId::ALL.len()];
    while let Some((id, result, extra)) = rx.recv().await {
        emit_progress(app, &result);
        if let Some(slot) = CheckId::ALL.iter().position(|c| *c == id) {
            slots[slot] = Some((result, extra));
        }
    }

    let mut checks = Vec::with_capacity(CheckId::ALL.len());
    let mut tools = Vec::new();
    let mut env_vars = Vec::new();
    for (id, slot) in CheckId::ALL.into_iter().zip(slots) {
        let (result, extra) = slot.unwrap_or_else(|| {
            log::error!("check {id:?} task did not report a result");
            let result = internal_error(id, started);
            emit_progress(app, &result);
            (result, Extra::None)
        });
        match extra {
            Extra::None => {}
            Extra::Tool(info) => tools.push(info),
            Extra::EnvVars(findings) => env_vars = findings,
        }
        checks.push(result);
    }

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

// ---------------------------------------------------------------------------
// Binary lookup shared by node / tools
// ---------------------------------------------------------------------------

/// Locates `binary` the way a *freshly opened terminal* would: the current process `PATH`
/// first, then the fresh-session `PATH` (`process::fresh_session_env` — Windows registry /
/// macOS login shell; a GUI app launched from Finder or before an install does not see
/// those), both yielding `(path, true)`; finally `extra_dirs` (e.g. the npm global bin dir),
/// yielding `(path, false)` = installed but not reachable from a terminal (guide fault E).
pub async fn locate_binary(binary: &str, extra_dirs: &[PathBuf]) -> Option<(PathBuf, bool)> {
    if let Some(path) = crate::platform::find_on_path(binary) {
        return Some((path, true));
    }
    if let Some(path) = find_on_fresh_path(binary).await {
        return Some((path, true));
    }
    crate::platform::find_binary(binary, extra_dirs)
}

/// `binary` resolved against the `PATH` a new terminal would have (`None` when absent).
async fn find_on_fresh_path(binary: &str) -> Option<PathBuf> {
    let env = match process::fresh_session_env().await {
        Ok(env) => env,
        Err(e) => {
            log::warn!("fresh session environment unavailable: {e}");
            return None;
        }
    };
    let path = process::get_env_var(&env, "PATH")?;
    process::resolve_program_in(binary, path)
}

// ---------------------------------------------------------------------------
// Version helpers shared by node / tools / os
// ---------------------------------------------------------------------------

/// Extracts the first `major.minor[.patch][-pre][+build]` found in `text` as a semver
/// version, tolerating prefixes and suffixes: `v22.1.0`, `codex-cli 0.5.1`,
/// `1.0.0 (Claude Code)`, `12.0` (patch defaults to 0), `10.0.19045.1` (extra components
/// dropped). `None` when no `digits.digits` sequence exists.
pub fn parse_version(text: &str) -> Option<semver::Version> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let starts_number = bytes[i].is_ascii_digit() && (i == 0 || !bytes[i - 1].is_ascii_digit());
        if starts_number {
            let end = bytes[i..]
                .iter()
                .position(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+')))
                .map_or(bytes.len(), |n| i + n);
            if let Some(v) = parse_version_token(&text[i..end]) {
                return Some(v);
            }
            i = end.max(i + 1);
        } else {
            i += 1;
        }
    }
    None
}

/// Parses one candidate token (`1.2.3-rc.1+build`), see [`parse_version`].
fn parse_version_token(token: &str) -> Option<semver::Version> {
    let core_end = token.find(['-', '+']).unwrap_or(token.len());
    let (core, rest) = token.split_at(core_end);
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let major = parts.next().flatten()?;
    let minor = parts.next().flatten()?;
    let patch = parts.next().flatten().unwrap_or(0);
    let base = semver::Version::new(major, minor, patch);
    if rest.is_empty() {
        return Some(base);
    }
    semver::Version::parse(&format!("{base}{rest}"))
        .ok()
        .or(Some(base))
}

/// Outcome of a `<program> --version`-style probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionRun {
    /// Exit code 0. `raw` is the redacted first non-empty output line, `version` the parsed
    /// semver when recognisable.
    Ok {
        raw: String,
        version: Option<semver::Version>,
    },
    /// The program exists but did not succeed (`timed_out`, non-zero `exit_code`, or a
    /// spawn error other than "not found"). `tail` is the redacted output/error tail.
    Failed {
        exit_code: Option<i32>,
        timed_out: bool,
        tail: String,
    },
    /// The program could not be found.
    NotFound,
}

impl VersionRun {
    /// Display version: parsed semver when available, else the raw first line.
    pub fn display_version(&self) -> Option<String> {
        match self {
            VersionRun::Ok { raw, version } => Some(
                version
                    .as_ref()
                    .map_or_else(|| raw.clone(), ToString::to_string),
            ),
            _ => None,
        }
    }
}

/// Number of trailing output lines kept in `VersionRun::Failed::tail`.
const FAILURE_TAIL_LINES: usize = 5;

/// Runs `program args…` with `timeout` and classifies the outcome (see [`VersionRun`]).
/// `program` may be a bare name (resolved on `PATH`) or a full path.
pub async fn run_version(program: &str, args: &[String], timeout: Duration) -> VersionRun {
    let spec = CommandSpec::new(program, args.iter().cloned()).with_timeout(timeout);
    match process::run(&spec).await {
        Ok(out) if out.success() => {
            let raw = first_line(&out.stdout)
                .or_else(|| first_line(&out.stderr))
                .unwrap_or_default();
            let version = parse_version(&raw);
            VersionRun::Ok { raw, version }
        }
        Ok(out) => {
            let combined = format!("{}\n{}", out.stdout, out.stderr);
            VersionRun::Failed {
                exit_code: out.exit_code,
                timed_out: out.timed_out,
                tail: tail_redacted(combined.trim(), FAILURE_TAIL_LINES),
            }
        }
        Err(crate::error::AppError::CommandNotFound { .. }) => VersionRun::NotFound,
        Err(e) => VersionRun::Failed {
            exit_code: None,
            timed_out: false,
            tail: redact_secrets(&e.to_string()),
        },
    }
}

/// First non-empty line of `text`, trimmed and redacted.
fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(redact_secrets)
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
        let failing = vec![
            cr(CheckId::Os, CheckStatus::Fail),
            cr(CheckId::Node, CheckStatus::Pass),
        ];
        assert_eq!(overall_status(&failing), CheckStatus::Fail);
        assert_eq!(
            overall_status(&[cr(CheckId::Os, CheckStatus::Skipped)]),
            CheckStatus::Skipped
        );
    }

    #[test]
    fn parse_version_table() {
        let v = |s: &str| semver::Version::parse(s).expect("semver");
        let cases: &[(&str, Option<semver::Version>)] = &[
            ("v22.1.0", Some(v("22.1.0"))),
            ("v22.1.0\n", Some(v("22.1.0"))),
            ("codex-cli 0.5.1", Some(v("0.5.1"))),
            ("1.0.0 (Claude Code)", Some(v("1.0.0"))),
            ("12.0", Some(v("12.0.0"))),
            ("10.0.19045.1", Some(v("10.0.19045"))),
            ("18.0.0", Some(v("18.0.0"))),
            ("Node.js v20.11.1 (LTS)", Some(v("20.11.1"))),
            ("1.2.3-beta.1+build.5", Some(v("1.2.3-beta.1+build.5"))),
            ("1.2.3-rc.", Some(v("1.2.3"))),
            ("10.5.0-x64", Some(v("10.5.0-x64"))),
            ("", None),
            ("Node.js", None),
            ("22", None),
            ("no digits here", None),
            ("abc 7 def", None),
        ];
        for (input, expected) in cases {
            assert_eq!(&parse_version(input), expected, "input {input:?}");
        }
    }

    #[test]
    fn semver_comparison_orders_prerelease_and_releases() {
        let min = parse_version("18.0.0").expect("min");
        assert!(parse_version("v22.1.0").expect("v") >= min);
        assert!(parse_version("v18.0.0").expect("v") >= min);
        assert!(parse_version("v16.20.2").expect("v") < min);
        assert!(parse_version("18.0.0-nightly").expect("v") < min);
        assert!(parse_version("12.7.1").expect("v") >= parse_version("12.0").expect("m"));
        assert!(parse_version("11.6").expect("v") < parse_version("12.0").expect("m"));
    }

    #[test]
    fn version_run_display_prefers_parsed_version() {
        let ok = VersionRun::Ok {
            raw: "codex-cli 0.5.1".into(),
            version: parse_version("0.5.1"),
        };
        assert_eq!(ok.display_version().as_deref(), Some("0.5.1"));
        let raw_only = VersionRun::Ok {
            raw: "unknown build".into(),
            version: None,
        };
        assert_eq!(raw_only.display_version().as_deref(), Some("unknown build"));
        assert_eq!(VersionRun::NotFound.display_version(), None);
    }

    #[test]
    fn first_line_skips_blank_lines_and_redacts() {
        assert_eq!(
            first_line("\n  \n v1.2.3 \nmore").as_deref(),
            Some("v1.2.3")
        );
        assert_eq!(first_line("   \n"), None);
        assert_eq!(
            first_line("key sk-abcdefghijklmnop").as_deref(),
            Some("key [REDACTED]")
        );
    }

    #[test]
    fn verdict_builder_and_result_stamp() {
        let v = Verdict::warn("x.y")
            .param("a", "1")
            .param_opt("b", None::<String>)
            .param_opt("c", Some("3"))
            .detail("d")
            .fix(FixAction::Rerun);
        assert_eq!(v.status, CheckStatus::Warn);
        assert_eq!(v.params.get("a").map(String::as_str), Some("1"));
        assert!(!v.params.contains_key("b"));
        assert_eq!(v.params.get("c").map(String::as_str), Some("3"));
        let r = v.into_result(CheckId::Os, Instant::now());
        assert_eq!(r.id, CheckId::Os);
        assert_eq!(r.code, "x.y");
        assert_eq!(r.details, vec!["d".to_owned()]);
        assert_eq!(r.fixes, vec![FixAction::Rerun]);
    }

    #[test]
    fn tool_id_names_match_wire_format() {
        assert_eq!(tool_id_str(ToolId::Codex), "codex");
        assert_eq!(tool_id_str(ToolId::ClaudeCode), "claude-code");
    }

    #[tokio::test]
    async fn locate_binary_prefers_path_then_extra_dirs() {
        let name = "codex-onboarding-locate-me-42";
        assert!(locate_binary(name, &[]).await.is_none());
        let dir = tempfile::tempdir().expect("tempdir");
        let file_name = crate::platform::candidate_names_for(crate::platform::platform(), name)
            .into_iter()
            .next()
            .expect("candidate");
        std::fs::write(dir.path().join(&file_name), b"").expect("write");
        let (path, on_path) = locate_binary(name, &[dir.path().to_path_buf()])
            .await
            .expect("found in extra dir");
        assert!(!on_path);
        assert_eq!(path, dir.path().join(file_name));
        // A shell is always reachable through PATH on every supported platform.
        let shell = if cfg!(windows) { "cmd" } else { "sh" };
        let (_, on_path) = locate_binary(shell, &[]).await.expect("shell");
        assert!(on_path);
    }

    #[tokio::test]
    async fn run_version_reports_missing_program() {
        let run = run_version(
            "definitely-not-a-real-binary-xyz-42",
            &["--version".to_owned()],
            Duration::from_secs(5),
        )
        .await;
        assert_eq!(run, VersionRun::NotFound);
    }

    #[tokio::test]
    async fn run_all_returns_every_check_in_order() {
        let ctx = CheckContext {
            config: crate::config::embedded().expect("config"),
            http: crate::net::build_client().expect("client"),
        };
        let snapshot = run_all(None, &ctx).await;
        let ids: Vec<CheckId> = snapshot.checks.iter().map(|c| c.id).collect();
        assert_eq!(ids, CheckId::ALL.to_vec());
        assert_eq!(snapshot.tools.len(), 2);
        assert_eq!(snapshot.tools[0].id, ToolId::Codex);
        assert_eq!(snapshot.tools[1].id, ToolId::ClaudeCode);
        assert_eq!(
            snapshot.env_vars.len(),
            ctx.config.env_vars_to_inspect.len()
        );
        assert!(snapshot
            .checks
            .iter()
            .all(|c| c.code != "check.internal_error"));
        assert_eq!(snapshot.overall, overall_status(&snapshot.checks));
    }

    #[tokio::test]
    async fn run_one_skips_unconfigured_tool() {
        let mut config = crate::config::embedded().expect("config");
        config.tools.retain(|t| t.id != ToolId::ClaudeCode);
        let ctx = CheckContext {
            config,
            http: crate::net::build_client().expect("client"),
        };
        let r = run_one(CheckId::ClaudeCode, &ctx).await;
        assert_eq!(r.status, CheckStatus::Skipped);
        assert_eq!(r.code, "tool.not_configured");
    }
}
