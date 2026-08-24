//! M5 — rule engine translating the guide's fault table (A–G) into actionable diagnoses,
//! plus the redacted diagnostic report (replaces "attach a screenshot" in tickets).
//!
//! The engine is pure: it looks at the reported [`Symptom`]s and the optional
//! [`EnvSnapshot`], never touches the machine (the only side channel is the current platform
//! and the npm global bin guess used as a fallback for the `dir` param of rule E).
//!
//! Rule mapping (assumption pending confirmation against the original guide — see
//! docs/OPEN_QUESTIONS.md Q-D1; codes → frontend key `diagnose:<code>.title` /
//! `diagnose:<code>.explanation`):
//!
//! | rule  | trigger                                                  | code                     | severity | params                          | actions                             |
//! |-------|----------------------------------------------------------|--------------------------|----------|---------------------------------|-------------------------------------|
//! | A     | HttpStatus 401/403                                       | auth.key_checklist       | Blocking | tool, status                    | GoToStep(configure)                 |
//! | B     | HttpStatus 404                                           | url.rule_recheck         | Blocking | tool, status, expected_base_url | GoToStep(configure)                 |
//! | C     | NoEffectAfterConfig (with or without snapshot)           | session.restart_terminal | Warning  | tool                            | Rerun                               |
//! | D     | EnvVarConflict / snapshot `env_vars` check Warn or Fail  | env.conflict             | Warning  | names                           | Instructions(env.conflict.instructions.<platform>) |
//! | E     | CommandNotFound / snapshot tool `installed && !on_path`  | path.not_refreshed       | Blocking | tool, binary, dir               | GoToStep(install), Rerun            |
//! | F     | ProtocolMismatch / HttpStatus 400/422                    | protocol.mismatch        | Blocking | tool, status?, protocol (of that tool) | GoToStep(configure)          |
//! | G     | AppBlockedByOs                                           | os.app_blocked           | Warning  | app                             | Instructions(os.app_blocked.instructions.<platform>) |
//! | NET   | NetworkError / Timeout / snapshot network check unreachable | network.unreachable   | Blocking (Warning when only Warn-level checks) | target | Rerun                  |
//! | GW    | HttpStatus 429 / 5xx                                     | gateway.upstream         | Warning  | status                          | Rerun                               |
//!
//! `CommandFailed { output_tail }` is classified best-effort from the (already redacted)
//! output: "command not found" wording → E, DNS/connection errors → NET, 401/403/"unauthorized"
//! → A, 404 → B; anything else yields no diagnosis (the UI shows the raw tail).
//!
//! Every diagnosis carries `checklist` step keys (frontend key `diagnose:<code>.steps.<key>`)
//! and `FixAction`s. Env-var fixes are always `Instructions` — the app never edits them
//! (PRD #17). `<platform>` is `windows` or `macos` (macOS wording is also used for other
//! Unix-likes), decided from `snapshot.os.platform` when a snapshot is present, otherwise from
//! the current platform. Results are deduplicated by `rule_id` (first occurrence wins,
//! symptom-derived before snapshot-derived; env-var names from all sources are merged into a
//! single D) and sorted Blocking > Warning > Info.
//!
//! Complete list of i18n keys the UI must provide (namespace `diagnose`):
//!
//! ```text
//! auth.key_checklist.{title,explanation,steps.whitespace,steps.provider_selected,steps.key_valid}
//! url.rule_recheck.{title,explanation,steps.trailing_slash,steps.v1_suffix,steps.hash_literal}
//! session.restart_terminal.{title,explanation,steps.close_all_terminals,steps.close_ide_terminals,steps.reopen_and_retry}
//! env.conflict.{title,explanation,steps.inspect,steps.remove_manually,steps.restart_terminal,instructions.windows,instructions.macos}
//! path.not_refreshed.{title,explanation,steps.restart_terminal,steps.check_npm_prefix_on_path,steps.reinstall}
//! protocol.mismatch.{title,explanation,steps.confirm_gateway_protocol,steps.use_responses,steps.use_cc_switch_proxy,steps.check_anthropic_base_url}
//! os.app_blocked.{title,explanation,steps.smartscreen_more_info,steps.run_anyway,steps.gatekeeper_open_anyway,steps.remove_quarantine,instructions.windows,instructions.macos}
//! network.unreachable.{title,explanation,steps.check_vpn_proxy,steps.try_mirror,steps.retry}
//! gateway.upstream.{title,explanation,steps.wait_retry,steps.contact_support}
//! ```
//!
//! `report::build` renders Markdown: app/OS info, check table, tool versions, masked env vars,
//! diagnoses, and (read-only) presence + top-level key names of `~/.codex/config.toml` /
//! `~/.claude/settings.json` with all values redacted. Reading is allowed; writing is not.

pub mod report;

use std::collections::{BTreeSet, HashSet};

use crate::checks::env_vars;
use crate::guide::{protocol_key, tool_key};
use crate::models::{
    AppConfig, CheckId, CheckStatus, DiagnoseRequest, Diagnosis, EnvSnapshot, FixAction, Params,
    Platform, Protocol, Severity, Symptom, ToolId, WizardStep,
};
use crate::redact::redact_secrets;

/// Rule ids (guide fault letters plus two non-guide rules).
pub const RULE_AUTH: &str = "A";
pub const RULE_URL: &str = "B";
pub const RULE_SESSION: &str = "C";
pub const RULE_ENV: &str = "D";
pub const RULE_PATH: &str = "E";
pub const RULE_PROTOCOL: &str = "F";
pub const RULE_APP_BLOCKED: &str = "G";
pub const RULE_NETWORK: &str = "NET";
pub const RULE_UPSTREAM: &str = "GW";

/// Everything a rule may consult besides the symptom itself.
struct Context<'a> {
    config: &'a AppConfig,
    snapshot: Option<&'a EnvSnapshot>,
    platform: Platform,
}

/// Runs the rule engine over `req` (see module docs for the rule table).
pub fn diagnose(req: &DiagnoseRequest, config: &AppConfig) -> Vec<Diagnosis> {
    let ctx = Context {
        config,
        snapshot: req.snapshot.as_ref(),
        platform: req
            .snapshot
            .as_ref()
            .map_or_else(crate::platform::platform, |s| s.os.platform),
    };

    let mut out: Vec<Diagnosis> = req
        .symptoms
        .iter()
        .filter_map(|s| rule_for_symptom(s, &ctx))
        .collect();
    if let Some(snapshot) = ctx.snapshot {
        out.extend(tools_not_on_path(snapshot, &ctx));
        out.extend(network_from_snapshot(snapshot));
    }
    let env_names = env_conflict_names(&req.symptoms, ctx.snapshot);
    if !env_names.is_empty() {
        out.push(env_conflict(&env_names, ctx.platform));
    }

    dedupe_by_rule(&mut out);
    sort_by_severity(&mut out);
    out
}

// ---------------------------------------------------------------------------
// Symptom → rule
// ---------------------------------------------------------------------------

fn rule_for_symptom(symptom: &Symptom, ctx: &Context<'_>) -> Option<Diagnosis> {
    match symptom {
        Symptom::HttpStatus { status, tool } => rule_for_status(*status, *tool, ctx),
        Symptom::CommandNotFound { tool } => Some(path_not_refreshed(*tool, ctx)),
        Symptom::CommandFailed { tool, output_tail } => rule_for_output(*tool, output_tail, ctx),
        Symptom::NoEffectAfterConfig { tool } => Some(restart_terminal(*tool)),
        Symptom::NetworkError { target } | Symptom::Timeout { target } => {
            Some(network_unreachable(target))
        }
        Symptom::AppBlockedByOs { app } => Some(app_blocked(app, ctx.platform)),
        // Merged into a single D by `env_conflict_names`.
        Symptom::EnvVarConflict { .. } => None,
        Symptom::ProtocolMismatch { tool } => Some(protocol_mismatch(*tool, None, ctx)),
    }
}

fn rule_for_status(status: u16, tool: ToolId, ctx: &Context<'_>) -> Option<Diagnosis> {
    match status {
        401 | 403 => Some(auth_key_checklist(tool, status)),
        404 => Some(url_rule_recheck(tool, status, ctx)),
        400 | 422 => Some(protocol_mismatch(tool, Some(status), ctx)),
        429 | 500..=599 => Some(gateway_upstream(status)),
        _ => None,
    }
}

/// Best-effort classification of a failed command's (redacted) output tail.
fn rule_for_output(tool: ToolId, output_tail: &str, ctx: &Context<'_>) -> Option<Diagnosis> {
    const NOT_FOUND: [&str; 4] = [
        "is not recognized",
        "command not found",
        "no such file or directory",
        "not found on path",
    ];
    const NETWORK: [&str; 8] = [
        "enotfound",
        "econnrefused",
        "econnreset",
        "etimedout",
        "eai_again",
        "getaddrinfo",
        "fetch failed",
        "network error",
    ];
    const AUTH: [&str; 5] = [
        "401",
        "403",
        "unauthorized",
        "invalid api key",
        "incorrect api key",
    ];
    let lower = output_tail.to_lowercase();
    let hit = |patterns: &[&str]| patterns.iter().any(|p| lower.contains(p));
    if hit(&NOT_FOUND) {
        Some(path_not_refreshed(tool, ctx))
    } else if hit(&NETWORK) {
        Some(network_unreachable(&gateway_host(ctx.config)))
    } else if hit(&AUTH) {
        Some(auth_key_checklist(tool, 0))
    } else if lower.contains("404") {
        Some(url_rule_recheck(tool, 404, ctx))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Snapshot → rules
// ---------------------------------------------------------------------------

/// Rule E for every tool that is installed but only reachable through the npm global bin.
fn tools_not_on_path(snapshot: &EnvSnapshot, ctx: &Context<'_>) -> Vec<Diagnosis> {
    snapshot
        .tools
        .iter()
        .filter(|t| t.installed && !t.on_path)
        .map(|t| path_not_refreshed(t.id, ctx))
        .collect()
}

/// Rule NET from the snapshot: every network check (`network.*` ids) that ended
/// `network.unreachable` — Fail (gateway, all npm registries) or Warn (GitHub) — is folded
/// into one diagnosis whose `target` lists the unreachable hosts (gateway first). Blocking
/// when any of them failed, Warning when only Warn-level checks are affected.
fn network_from_snapshot(snapshot: &EnvSnapshot) -> Option<Diagnosis> {
    fn rank(id: CheckId) -> Option<u8> {
        match id {
            CheckId::NetworkGateway => Some(0),
            CheckId::NetworkNpm => Some(1),
            CheckId::NetworkGithub => Some(2),
            _ => None,
        }
    }
    let mut unreachable: Vec<&crate::models::CheckResult> = snapshot
        .checks
        .iter()
        .filter(|c| {
            rank(c.id).is_some()
                && matches!(c.status, CheckStatus::Fail | CheckStatus::Warn)
                && c.code == "network.unreachable"
        })
        .collect();
    if unreachable.is_empty() {
        return None;
    }
    unreachable.sort_by_key(|c| rank(c.id));
    let mut targets: Vec<&str> = Vec::new();
    for c in &unreachable {
        if let Some(t) = c.params.get("target").map(String::as_str) {
            if !t.trim().is_empty() && !targets.contains(&t) {
                targets.push(t);
            }
        }
    }
    let target = if targets.is_empty() {
        String::from("-")
    } else {
        targets.join(", ")
    };
    let mut diagnosis = network_unreachable(&target);
    if !unreachable.iter().any(|c| c.status == CheckStatus::Fail) {
        diagnosis.severity = Severity::Warning;
    }
    Some(diagnosis)
}

/// Env-var names for rule D: explicit `EnvVarConflict` symptoms plus, when the snapshot's
/// `env_vars` check is Warn/Fail, every *conflicting* finding (`is_conflict_name`: `*_API_KEY`
/// / `*_BASE_URL` / …; proxy variables are informational) that is present or has a persistent
/// source — falling back to the check's `names` param.
fn env_conflict_names(symptoms: &[Symptom], snapshot: Option<&EnvSnapshot>) -> BTreeSet<String> {
    let mut names: BTreeSet<String> = symptoms
        .iter()
        .filter_map(|s| match s {
            Symptom::EnvVarConflict { name } => Some(name.trim().to_owned()),
            _ => None,
        })
        .filter(|n| !n.is_empty())
        .collect();
    let Some(snapshot) = snapshot else {
        return names;
    };
    let Some(check) = snapshot.checks.iter().find(|c| c.id == CheckId::EnvVars) else {
        return names;
    };
    if !matches!(check.status, CheckStatus::Warn | CheckStatus::Fail) {
        return names;
    }
    let from_findings: Vec<String> = snapshot
        .env_vars
        .iter()
        .filter(|f| env_vars::is_present(f) && env_vars::is_conflict_name(&f.name))
        .map(|f| f.name.clone())
        .collect();
    if from_findings.is_empty() {
        let from_params = check
            .params
            .get("names")
            .map(|v| {
                v.split(',')
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        names.extend(from_params);
    } else {
        names.extend(from_findings);
    }
    names
}

// ---------------------------------------------------------------------------
// Diagnosis builders (one per rule)
// ---------------------------------------------------------------------------

fn auth_key_checklist(tool: ToolId, status: u16) -> Diagnosis {
    let mut params = params([("tool", tool_key(tool))]);
    if status != 0 {
        params.insert("status".into(), status.to_string());
    }
    Diagnosis {
        rule_id: RULE_AUTH.into(),
        severity: Severity::Blocking,
        code: "auth.key_checklist".into(),
        params,
        actions: vec![FixAction::GoToStep {
            step: WizardStep::Configure,
        }],
        checklist: keys(&["whitespace", "provider_selected", "key_valid"]),
    }
}

fn url_rule_recheck(tool: ToolId, status: u16, ctx: &Context<'_>) -> Diagnosis {
    let mut params = params([("tool", tool_key(tool))]);
    params.insert("status".into(), status.to_string());
    // The address this tool should carry — Claude Code's is the root, Codex's ends in `/v1`.
    params.insert(
        "expected_base_url".into(),
        crate::config::gateway_defaults(&ctx.config.gateway, tool).base_url,
    );
    Diagnosis {
        rule_id: RULE_URL.into(),
        severity: Severity::Blocking,
        code: "url.rule_recheck".into(),
        params,
        actions: vec![FixAction::GoToStep {
            step: WizardStep::Configure,
        }],
        checklist: keys(&["trailing_slash", "v1_suffix", "hash_literal"]),
    }
}

fn restart_terminal(tool: ToolId) -> Diagnosis {
    Diagnosis {
        rule_id: RULE_SESSION.into(),
        severity: Severity::Warning,
        code: "session.restart_terminal".into(),
        params: params([("tool", tool_key(tool))]),
        actions: vec![FixAction::Rerun],
        checklist: keys(&[
            "close_all_terminals",
            "close_ide_terminals",
            "reopen_and_retry",
        ]),
    }
}

fn env_conflict(names: &BTreeSet<String>, platform: Platform) -> Diagnosis {
    let joined = names.iter().cloned().collect::<Vec<_>>().join(", ");
    let params = params([("names", joined.as_str())]);
    Diagnosis {
        rule_id: RULE_ENV.into(),
        severity: Severity::Warning,
        code: "env.conflict".into(),
        params: params.clone(),
        actions: vec![FixAction::Instructions {
            code: format!(
                "diagnose:env.conflict.instructions.{}",
                platform_key(platform)
            ),
            params,
        }],
        checklist: keys(&["inspect", "remove_manually", "restart_terminal"]),
    }
}

fn path_not_refreshed(tool: ToolId, ctx: &Context<'_>) -> Diagnosis {
    let binary = ctx
        .config
        .tools
        .iter()
        .find(|t| t.id == tool)
        .map(|t| t.binary.clone())
        .unwrap_or_default();
    let dir = npm_bin_dir_for(tool, ctx);
    let mut params = params([("tool", tool_key(tool)), ("binary", binary.as_str())]);
    params.insert("dir".into(), redact_secrets(&dir));
    // One-click PATH repair first (ADR-0008), then the manual routes.
    let mut actions = Vec::new();
    if !dir.trim().is_empty() {
        actions.push(FixAction::RepairPath { dir: dir.clone() });
    }
    actions.extend([
        FixAction::GoToStep {
            step: WizardStep::Install,
        },
        FixAction::Rerun,
    ]);
    Diagnosis {
        rule_id: RULE_PATH.into(),
        severity: Severity::Blocking,
        code: "path.not_refreshed".into(),
        params,
        actions,
        checklist: keys(&["restart_terminal", "check_npm_prefix_on_path", "reinstall"]),
    }
}

fn protocol_mismatch(tool: ToolId, status: Option<u16>, ctx: &Context<'_>) -> Diagnosis {
    // The protocol *this* tool speaks — Claude Code always Anthropic Messages, whatever the
    // company preset says for Codex.
    let protocol = crate::config::tool_protocol(&ctx.config.gateway, tool);
    let mut params = params([
        ("tool", tool_key(tool)),
        ("protocol", protocol_key(protocol)),
    ]);
    if let Some(status) = status {
        params.insert("status".into(), status.to_string());
    }
    Diagnosis {
        rule_id: RULE_PROTOCOL.into(),
        severity: Severity::Blocking,
        code: "protocol.mismatch".into(),
        params,
        actions: vec![FixAction::GoToStep {
            step: WizardStep::Configure,
        }],
        // Claude Code has no protocol setting in CC Switch — for it the address is the lever.
        checklist: if protocol == Protocol::AnthropicMessages {
            keys(&["confirm_gateway_protocol", "check_anthropic_base_url"])
        } else {
            keys(&[
                "confirm_gateway_protocol",
                "use_responses",
                "use_cc_switch_proxy",
            ])
        },
    }
}

fn app_blocked(app: &str, platform: Platform) -> Diagnosis {
    let app = redact_secrets(app);
    let params = params([("app", app.as_str())]);
    let checklist = match platform {
        Platform::Windows => keys(&["smartscreen_more_info", "run_anyway"]),
        _ => keys(&["gatekeeper_open_anyway", "remove_quarantine"]),
    };
    Diagnosis {
        rule_id: RULE_APP_BLOCKED.into(),
        severity: Severity::Warning,
        code: "os.app_blocked".into(),
        params: params.clone(),
        actions: vec![FixAction::Instructions {
            code: format!(
                "diagnose:os.app_blocked.instructions.{}",
                platform_key(platform)
            ),
            params,
        }],
        checklist,
    }
}

fn network_unreachable(target: &str) -> Diagnosis {
    let target = redact_secrets(target);
    Diagnosis {
        rule_id: RULE_NETWORK.into(),
        severity: Severity::Blocking,
        code: "network.unreachable".into(),
        params: params([("target", target.as_str())]),
        actions: vec![FixAction::Rerun],
        checklist: keys(&["check_vpn_proxy", "try_mirror", "retry"]),
    }
}

fn gateway_upstream(status: u16) -> Diagnosis {
    let status = status.to_string();
    Diagnosis {
        rule_id: RULE_UPSTREAM.into(),
        severity: Severity::Warning,
        code: "gateway.upstream".into(),
        params: params([("status", status.as_str())]),
        actions: vec![FixAction::Rerun],
        checklist: keys(&["wait_retry", "contact_support"]),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Directory that should hold the tool's executable: the snapshot's npm global bin (or the
/// parent of the resolved path), else the platform's npm global bin guess, else empty.
fn npm_bin_dir_for(tool: ToolId, ctx: &Context<'_>) -> String {
    let from_snapshot = ctx
        .snapshot
        .and_then(|s| s.tools.iter().find(|t| t.id == tool))
        .and_then(|t| {
            t.npm_global_bin.clone().or_else(|| {
                t.path.as_deref().and_then(|p| {
                    std::path::Path::new(p)
                        .parent()
                        .map(|d| d.to_string_lossy().into_owned())
                })
            })
        })
        .filter(|d| !d.trim().is_empty());
    from_snapshot
        .or_else(|| {
            crate::platform::npm_global_prefix_guess()
                .map(|p| crate::platform::npm_global_bin_dir(&p))
                .map(|p| p.to_string_lossy().into_owned())
        })
        .unwrap_or_default()
}

/// Host of the service gateway (fallback: the raw base URL) for network diagnoses.
fn gateway_host(config: &AppConfig) -> String {
    url::Url::parse(&config.gateway.base_url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_else(|| config.gateway.base_url.clone())
}

/// `windows` or `macos` (Unix-like wording for anything that is not Windows).
fn platform_key(platform: Platform) -> &'static str {
    match platform {
        Platform::Windows => "windows",
        Platform::Macos | Platform::Linux | Platform::Unknown => "macos",
    }
}

fn params<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Params {
    pairs
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

fn keys(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

/// Keeps the first diagnosis per `rule_id`.
fn dedupe_by_rule(diagnoses: &mut Vec<Diagnosis>) {
    let mut seen: HashSet<String> = HashSet::new();
    diagnoses.retain(|d| seen.insert(d.rule_id.clone()));
}

/// Blocking first, then Warning, then Info; stable within a severity.
fn sort_by_severity(diagnoses: &mut [Diagnosis]) {
    diagnoses.sort_by_key(|d| std::cmp::Reverse(d.severity));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::models::{
        CheckResult, EnvVarFinding, EnvVarSource, EnvVarSourceKind, OsInfo, ToolInfo,
    };

    fn cfg() -> AppConfig {
        let mut cfg = config::embedded().expect("embedded config");
        cfg.gateway.base_url = "https://gateway.example.com/v1".into();
        // Pinned so the tests never depend on the shipped production preset.
        cfg.gateway.claude_code = crate::models::ClaudeCodeGateway {
            base_url: "https://gateway.example.com".into(),
            default_model: "claude-sonnet-5".into(),
        };
        cfg
    }

    fn snapshot(platform: Platform) -> EnvSnapshot {
        EnvSnapshot {
            os: OsInfo {
                platform,
                version: "10.0".into(),
                build: Some(19045),
                arch: "x86_64".into(),
                shell: "cmd.exe".into(),
                home_dir: "C:\\Users\\me".into(),
                is_admin: Some(false),
            },
            tools: vec![],
            env_vars: vec![],
            checks: vec![],
            overall: CheckStatus::Pass,
            generated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    fn check(id: CheckId, status: CheckStatus, params: Params) -> CheckResult {
        CheckResult {
            id,
            status,
            code: String::new(),
            params,
            details: vec![],
            fixes: vec![],
            duration_ms: 0,
        }
    }

    fn tool(id: ToolId, installed: bool, on_path: bool) -> ToolInfo {
        ToolInfo {
            id,
            installed,
            version: installed.then(|| "1.0.0".to_owned()),
            path: installed.then(|| "C:\\Users\\me\\AppData\\Roaming\\npm\\codex.cmd".to_owned()),
            on_path,
            npm_global_bin: Some("C:\\Users\\me\\AppData\\Roaming\\npm".into()),
        }
    }

    fn run(symptoms: Vec<Symptom>, snapshot: Option<EnvSnapshot>) -> Vec<Diagnosis> {
        diagnose(&DiagnoseRequest { symptoms, snapshot }, &cfg())
    }

    fn only(symptom: Symptom) -> Diagnosis {
        let mut out = run(vec![symptom], None);
        assert_eq!(out.len(), 1, "{out:?}");
        out.remove(0)
    }

    fn p<'a>(d: &'a Diagnosis, key: &str) -> &'a str {
        d.params.get(key).map(String::as_str).unwrap_or_default()
    }

    // ---- one test per rule -----------------------------------------------------------

    #[test]
    fn rule_table_symptom_to_diagnosis() {
        // (symptom, rule_id, code, severity, checklist)
        let cases: Vec<(Symptom, &str, &str, Severity, Vec<&str>)> = vec![
            (
                Symptom::HttpStatus {
                    status: 401,
                    tool: ToolId::Codex,
                },
                "A",
                "auth.key_checklist",
                Severity::Blocking,
                vec!["whitespace", "provider_selected", "key_valid"],
            ),
            (
                Symptom::HttpStatus {
                    status: 403,
                    tool: ToolId::ClaudeCode,
                },
                "A",
                "auth.key_checklist",
                Severity::Blocking,
                vec!["whitespace", "provider_selected", "key_valid"],
            ),
            (
                Symptom::HttpStatus {
                    status: 404,
                    tool: ToolId::Codex,
                },
                "B",
                "url.rule_recheck",
                Severity::Blocking,
                vec!["trailing_slash", "v1_suffix", "hash_literal"],
            ),
            (
                Symptom::NoEffectAfterConfig {
                    tool: ToolId::Codex,
                },
                "C",
                "session.restart_terminal",
                Severity::Warning,
                vec![
                    "close_all_terminals",
                    "close_ide_terminals",
                    "reopen_and_retry",
                ],
            ),
            (
                Symptom::EnvVarConflict {
                    name: "OPENAI_API_KEY".into(),
                },
                "D",
                "env.conflict",
                Severity::Warning,
                vec!["inspect", "remove_manually", "restart_terminal"],
            ),
            (
                Symptom::CommandNotFound {
                    tool: ToolId::Codex,
                },
                "E",
                "path.not_refreshed",
                Severity::Blocking,
                vec!["restart_terminal", "check_npm_prefix_on_path", "reinstall"],
            ),
            (
                Symptom::ProtocolMismatch {
                    tool: ToolId::Codex,
                },
                "F",
                "protocol.mismatch",
                Severity::Blocking,
                vec![
                    "confirm_gateway_protocol",
                    "use_responses",
                    "use_cc_switch_proxy",
                ],
            ),
            (
                Symptom::HttpStatus {
                    status: 400,
                    tool: ToolId::Codex,
                },
                "F",
                "protocol.mismatch",
                Severity::Blocking,
                vec![
                    "confirm_gateway_protocol",
                    "use_responses",
                    "use_cc_switch_proxy",
                ],
            ),
            (
                Symptom::HttpStatus {
                    status: 422,
                    tool: ToolId::Codex,
                },
                "F",
                "protocol.mismatch",
                Severity::Blocking,
                vec![
                    "confirm_gateway_protocol",
                    "use_responses",
                    "use_cc_switch_proxy",
                ],
            ),
            (
                Symptom::NetworkError {
                    target: "https://gateway.example.com".into(),
                },
                "NET",
                "network.unreachable",
                Severity::Blocking,
                vec!["check_vpn_proxy", "try_mirror", "retry"],
            ),
            (
                Symptom::Timeout {
                    target: "registry".into(),
                },
                "NET",
                "network.unreachable",
                Severity::Blocking,
                vec!["check_vpn_proxy", "try_mirror", "retry"],
            ),
            (
                Symptom::HttpStatus {
                    status: 429,
                    tool: ToolId::Codex,
                },
                "GW",
                "gateway.upstream",
                Severity::Warning,
                vec!["wait_retry", "contact_support"],
            ),
            (
                Symptom::HttpStatus {
                    status: 503,
                    tool: ToolId::Codex,
                },
                "GW",
                "gateway.upstream",
                Severity::Warning,
                vec!["wait_retry", "contact_support"],
            ),
        ];
        for (symptom, rule, code, severity, checklist) in cases {
            let d = only(symptom.clone());
            assert_eq!(d.rule_id, rule, "{symptom:?}");
            assert_eq!(d.code, code, "{symptom:?}");
            assert_eq!(d.severity, severity, "{symptom:?}");
            assert_eq!(d.checklist, checklist, "{symptom:?}");
            assert!(!d.actions.is_empty(), "{symptom:?}");
        }
    }

    #[test]
    fn rule_a_params_and_actions() {
        let d = only(Symptom::HttpStatus {
            status: 401,
            tool: ToolId::ClaudeCode,
        });
        assert_eq!(p(&d, "tool"), "claude-code");
        assert_eq!(p(&d, "status"), "401");
        assert_eq!(
            d.actions,
            vec![FixAction::GoToStep {
                step: WizardStep::Configure
            }]
        );
    }

    #[test]
    fn rule_b_carries_expected_gateway_url() {
        let d = only(Symptom::HttpStatus {
            status: 404,
            tool: ToolId::Codex,
        });
        assert_eq!(p(&d, "expected_base_url"), "https://gateway.example.com/v1");
        assert_eq!(p(&d, "status"), "404");
        assert_eq!(
            d.actions,
            vec![FixAction::GoToStep {
                step: WizardStep::Configure
            }]
        );
    }

    /// Rule B names the address *that tool* should carry: Claude Code's is the root, because
    /// its client appends `/v1/messages` itself.
    #[test]
    fn rule_b_expected_url_is_per_tool() {
        let d = only(Symptom::HttpStatus {
            status: 404,
            tool: ToolId::ClaudeCode,
        });
        assert_eq!(p(&d, "expected_base_url"), "https://gateway.example.com");
    }

    /// Rule F reports the protocol *that tool* speaks, and Claude Code gets the address
    /// checklist instead of "switch the protocol to Responses" (it has no protocol setting).
    #[test]
    fn rule_f_is_per_tool() {
        let codex = only(Symptom::ProtocolMismatch {
            tool: ToolId::Codex,
        });
        assert_eq!(p(&codex, "protocol"), "responses");
        assert!(
            codex.checklist.iter().any(|s| s == "use_responses"),
            "{codex:?}"
        );

        let claude = only(Symptom::ProtocolMismatch {
            tool: ToolId::ClaudeCode,
        });
        assert_eq!(p(&claude, "protocol"), "anthropic_messages");
        assert_eq!(
            claude.checklist,
            vec![
                "confirm_gateway_protocol".to_owned(),
                "check_anthropic_base_url".to_owned()
            ]
        );
    }

    #[test]
    fn rule_c_emitted_without_snapshot() {
        let d = only(Symptom::NoEffectAfterConfig {
            tool: ToolId::Codex,
        });
        assert_eq!(d.rule_id, "C");
        assert_eq!(d.actions, vec![FixAction::Rerun]);
        assert_eq!(p(&d, "tool"), "codex");
    }

    #[test]
    fn rule_d_merges_names_and_uses_platform_instructions() {
        let symptoms = vec![
            Symptom::EnvVarConflict {
                name: "OPENAI_BASE_URL".into(),
            },
            Symptom::EnvVarConflict {
                name: "OPENAI_API_KEY".into(),
            },
            Symptom::EnvVarConflict {
                name: "OPENAI_API_KEY".into(),
            },
        ];
        let out = run(symptoms.clone(), Some(snapshot(Platform::Windows)));
        assert_eq!(out.len(), 1, "{out:?}");
        let d = &out[0];
        assert_eq!(d.rule_id, "D");
        assert_eq!(p(d, "names"), "OPENAI_API_KEY, OPENAI_BASE_URL");
        assert_eq!(
            d.actions,
            vec![FixAction::Instructions {
                code: "diagnose:env.conflict.instructions.windows".into(),
                params: d.params.clone(),
            }]
        );

        let mac = run(symptoms, Some(snapshot(Platform::Macos)));
        assert!(matches!(
            &mac[0].actions[0],
            FixAction::Instructions { code, .. } if code == "diagnose:env.conflict.instructions.macos"
        ));
    }

    #[test]
    fn rule_e_params_and_actions() {
        let d = only(Symptom::CommandNotFound {
            tool: ToolId::Codex,
        });
        assert_eq!(p(&d, "tool"), "codex");
        assert_eq!(p(&d, "binary"), "codex");
        assert!(d.params.contains_key("dir"));
        let dir = p(&d, "dir");
        let mut expected = Vec::new();
        if !dir.trim().is_empty() {
            expected.push(FixAction::RepairPath {
                dir: dir.to_owned(),
            });
        }
        expected.extend([
            FixAction::GoToStep {
                step: WizardStep::Install,
            },
            FixAction::Rerun,
        ]);
        assert_eq!(d.actions, expected);
    }

    #[test]
    fn rule_e_uses_snapshot_npm_bin_dir() {
        let mut snap = snapshot(Platform::Windows);
        snap.tools.push(tool(ToolId::Codex, true, false));
        let out = run(
            vec![Symptom::CommandNotFound {
                tool: ToolId::Codex,
            }],
            Some(snap),
        );
        let e = out.iter().find(|d| d.rule_id == "E").expect("E");
        assert_eq!(p(e, "dir"), "C:\\Users\\me\\AppData\\Roaming\\npm");
        assert_eq!(out.iter().filter(|d| d.rule_id == "E").count(), 1);
    }

    #[test]
    fn rule_f_carries_expected_protocol_and_status() {
        let d = only(Symptom::HttpStatus {
            status: 422,
            tool: ToolId::Codex,
        });
        assert_eq!(p(&d, "protocol"), "responses");
        assert_eq!(p(&d, "status"), "422");
        let d2 = only(Symptom::ProtocolMismatch {
            tool: ToolId::Codex,
        });
        assert!(!d2.params.contains_key("status"));
        assert_eq!(
            d2.actions,
            vec![FixAction::GoToStep {
                step: WizardStep::Configure
            }]
        );
    }

    #[test]
    fn rule_g_is_platform_specific() {
        let win = run(
            vec![Symptom::AppBlockedByOs {
                app: "CC Switch".into(),
            }],
            Some(snapshot(Platform::Windows)),
        );
        assert_eq!(win[0].rule_id, "G");
        assert_eq!(
            win[0].checklist,
            vec!["smartscreen_more_info", "run_anyway"]
        );
        assert_eq!(p(&win[0], "app"), "CC Switch");
        assert_eq!(
            win[0].actions,
            vec![FixAction::Instructions {
                code: "diagnose:os.app_blocked.instructions.windows".into(),
                params: win[0].params.clone(),
            }]
        );

        let mac = run(
            vec![Symptom::AppBlockedByOs {
                app: "CC Switch".into(),
            }],
            Some(snapshot(Platform::Macos)),
        );
        assert_eq!(
            mac[0].checklist,
            vec!["gatekeeper_open_anyway", "remove_quarantine"]
        );
        assert!(matches!(
            &mac[0].actions[0],
            FixAction::Instructions { code, .. } if code == "diagnose:os.app_blocked.instructions.macos"
        ));
    }

    #[test]
    fn network_and_upstream_params() {
        let net = only(Symptom::NetworkError {
            target: "https://user:secret@gateway.example.com/v1".into(),
        });
        assert!(!p(&net, "target").contains("secret"), "{net:?}");
        assert_eq!(net.actions, vec![FixAction::Rerun]);

        let up = only(Symptom::HttpStatus {
            status: 502,
            tool: ToolId::Codex,
        });
        assert_eq!(p(&up, "status"), "502");
        assert_eq!(up.actions, vec![FixAction::Rerun]);
    }

    #[test]
    fn unmapped_status_and_output_yield_nothing() {
        assert!(run(
            vec![Symptom::HttpStatus {
                status: 200,
                tool: ToolId::Codex
            }],
            None
        )
        .is_empty());
        assert!(run(
            vec![Symptom::HttpStatus {
                status: 302,
                tool: ToolId::Codex
            }],
            None
        )
        .is_empty());
        assert!(run(
            vec![Symptom::CommandFailed {
                tool: ToolId::Codex,
                output_tail: "SyntaxError: Unexpected token".into()
            }],
            None
        )
        .is_empty());
    }

    #[test]
    fn command_failed_output_is_classified() {
        let cases: Vec<(&str, &str)> = vec![
            (
                "'codex' is not recognized as an internal or external command",
                "E",
            ),
            ("zsh: command not found: codex", "E"),
            ("Error: getaddrinfo ENOTFOUND gateway.example.com", "NET"),
            ("TypeError: fetch failed", "NET"),
            ("Error: 401 Unauthorized", "A"),
            ("Incorrect API key provided", "A"),
            ("Request failed with status 404", "B"),
        ];
        for (tail, rule) in cases {
            let out = run(
                vec![Symptom::CommandFailed {
                    tool: ToolId::Codex,
                    output_tail: tail.into(),
                }],
                None,
            );
            assert_eq!(out.len(), 1, "{tail}");
            assert_eq!(out[0].rule_id, rule, "{tail}");
        }
    }

    // ---- snapshot-derived, dedupe, ordering ---------------------------------------------

    #[test]
    fn snapshot_env_vars_warn_yields_rule_d_with_finding_names() {
        let mut snap = snapshot(Platform::Macos);
        snap.checks.push(check(
            CheckId::EnvVars,
            CheckStatus::Warn,
            params([("names", "OPENAI_API_KEY")]),
        ));
        snap.env_vars = vec![
            EnvVarFinding {
                name: "OPENAI_API_KEY".into(),
                present_in_session: true,
                value_masked: Some("sk-****abcd".into()),
                sources: vec![],
            },
            EnvVarFinding {
                name: "ANTHROPIC_BASE_URL".into(),
                present_in_session: false,
                value_masked: None,
                sources: vec![EnvVarSource {
                    kind: EnvVarSourceKind::ShellRc,
                    location: "~/.zshrc:12".into(),
                }],
            },
            EnvVarFinding {
                name: "HTTP_PROXY".into(),
                present_in_session: false,
                value_masked: None,
                sources: vec![],
            },
            // present, but a proxy variable is informational — never a "conflict"
            EnvVarFinding {
                name: "HTTPS_PROXY".into(),
                present_in_session: true,
                value_masked: Some("http://proxy.corp:8080".into()),
                sources: vec![EnvVarSource {
                    kind: EnvVarSourceKind::ShellRc,
                    location: "~/.zshrc:3".into(),
                }],
            },
        ];
        let out = run(vec![], Some(snap));
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].rule_id, "D");
        assert_eq!(p(&out[0], "names"), "ANTHROPIC_BASE_URL, OPENAI_API_KEY");
    }

    #[test]
    fn snapshot_env_vars_warn_falls_back_to_check_params() {
        let mut snap = snapshot(Platform::Windows);
        snap.checks.push(check(
            CheckId::EnvVars,
            CheckStatus::Warn,
            params([("names", "OPENAI_API_KEY, OPENAI_BASE_URL")]),
        ));
        let out = run(vec![], Some(snap));
        assert_eq!(out.len(), 1);
        assert_eq!(p(&out[0], "names"), "OPENAI_API_KEY, OPENAI_BASE_URL");
    }

    #[test]
    fn snapshot_env_vars_pass_yields_nothing() {
        let mut snap = snapshot(Platform::Windows);
        snap.checks
            .push(check(CheckId::EnvVars, CheckStatus::Pass, Params::new()));
        snap.env_vars.push(EnvVarFinding {
            name: "HTTP_PROXY".into(),
            present_in_session: true,
            value_masked: Some("****".into()),
            sources: vec![],
        });
        assert!(run(vec![], Some(snap)).is_empty());
    }

    #[test]
    fn snapshot_tool_not_on_path_yields_rule_e() {
        let mut snap = snapshot(Platform::Windows);
        snap.tools.push(tool(ToolId::Codex, true, false));
        snap.tools.push(tool(ToolId::ClaudeCode, true, true));
        let out = run(vec![], Some(snap));
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].rule_id, "E");
        assert_eq!(p(&out[0], "tool"), "codex");
    }

    #[test]
    fn snapshot_tool_missing_entirely_yields_nothing() {
        let mut snap = snapshot(Platform::Windows);
        snap.tools.push(tool(ToolId::Codex, false, false));
        assert!(run(vec![], Some(snap)).is_empty());
    }

    fn network_check(id: CheckId, status: CheckStatus, code: &str, target: &str) -> CheckResult {
        let mut c = check(id, status, params([("target", target)]));
        c.code = code.into();
        c
    }

    #[test]
    fn snapshot_gateway_unreachable_yields_blocking_net() {
        let mut snap = snapshot(Platform::Windows);
        snap.checks.push(network_check(
            CheckId::NetworkNpm,
            CheckStatus::Pass,
            "network.ok",
            "registry.npmjs.org",
        ));
        snap.checks.push(network_check(
            CheckId::NetworkGateway,
            CheckStatus::Fail,
            "network.unreachable",
            "gateway.example.com",
        ));
        let out = run(vec![], Some(snap));
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].rule_id, "NET");
        assert_eq!(out[0].code, "network.unreachable");
        assert_eq!(out[0].severity, Severity::Blocking);
        assert_eq!(p(&out[0], "target"), "gateway.example.com");
        assert_eq!(out[0].actions, vec![FixAction::Rerun]);
    }

    #[test]
    fn snapshot_network_targets_merge_gateway_first() {
        let mut snap = snapshot(Platform::Windows);
        // Snapshot order is npm → gateway → github; the diagnosis lists the gateway first.
        snap.checks.push(network_check(
            CheckId::NetworkNpm,
            CheckStatus::Fail,
            "network.unreachable",
            "registry.npmjs.org",
        ));
        snap.checks.push(network_check(
            CheckId::NetworkGateway,
            CheckStatus::Fail,
            "network.unreachable",
            "gateway.example.com",
        ));
        snap.checks.push(network_check(
            CheckId::NetworkGithub,
            CheckStatus::Warn,
            "network.unreachable",
            "github.com",
        ));
        let out = run(vec![], Some(snap));
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(
            p(&out[0], "target"),
            "gateway.example.com, registry.npmjs.org, github.com"
        );
        assert_eq!(out[0].severity, Severity::Blocking);
    }

    #[test]
    fn snapshot_only_github_warn_yields_warning_net() {
        let mut snap = snapshot(Platform::Windows);
        snap.checks.push(network_check(
            CheckId::NetworkGithub,
            CheckStatus::Warn,
            "network.unreachable",
            "github.com",
        ));
        let out = run(vec![], Some(snap));
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].rule_id, "NET");
        assert_eq!(out[0].severity, Severity::Warning);
        assert_eq!(p(&out[0], "target"), "github.com");
    }

    #[test]
    fn snapshot_network_ok_or_mirror_only_yields_nothing() {
        let mut snap = snapshot(Platform::Windows);
        snap.checks.push(network_check(
            CheckId::NetworkNpm,
            CheckStatus::Warn,
            "network.mirror_only",
            "registry.npmmirror.com",
        ));
        snap.checks.push(network_check(
            CheckId::NetworkGateway,
            CheckStatus::Pass,
            "network.ok",
            "gateway.example.com",
        ));
        assert!(run(vec![], Some(snap)).is_empty());
    }

    #[test]
    fn symptom_network_error_wins_over_snapshot_network_rule() {
        let mut snap = snapshot(Platform::Windows);
        snap.checks.push(network_check(
            CheckId::NetworkGateway,
            CheckStatus::Fail,
            "network.unreachable",
            "gateway.example.com",
        ));
        let out = run(
            vec![Symptom::Timeout {
                target: "https://gateway.example.com/v1/responses".into(),
            }],
            Some(snap),
        );
        assert_eq!(out.iter().filter(|d| d.rule_id == "NET").count(), 1);
        assert_eq!(
            p(&out[0], "target"),
            "https://gateway.example.com/v1/responses"
        );
    }

    #[test]
    fn dedupes_by_rule_id_keeping_first() {
        let out = run(
            vec![
                Symptom::HttpStatus {
                    status: 401,
                    tool: ToolId::Codex,
                },
                Symptom::HttpStatus {
                    status: 403,
                    tool: ToolId::ClaudeCode,
                },
                Symptom::NetworkError { target: "a".into() },
                Symptom::Timeout { target: "b".into() },
            ],
            None,
        );
        assert_eq!(out.len(), 2, "{out:?}");
        let a = out.iter().find(|d| d.rule_id == "A").expect("A");
        assert_eq!(p(a, "status"), "401");
        let net = out.iter().find(|d| d.rule_id == "NET").expect("NET");
        assert_eq!(p(net, "target"), "a");
    }

    #[test]
    fn sorted_blocking_then_warning_and_stable() {
        let out = run(
            vec![
                Symptom::HttpStatus {
                    status: 503,
                    tool: ToolId::Codex,
                },
                Symptom::NoEffectAfterConfig {
                    tool: ToolId::Codex,
                },
                Symptom::AppBlockedByOs { app: "x".into() },
                Symptom::HttpStatus {
                    status: 404,
                    tool: ToolId::Codex,
                },
                Symptom::CommandNotFound {
                    tool: ToolId::Codex,
                },
            ],
            None,
        );
        let ids: Vec<&str> = out.iter().map(|d| d.rule_id.as_str()).collect();
        assert_eq!(ids, ["B", "E", "GW", "C", "G"]);
        assert!(out.windows(2).all(|w| w[0].severity >= w[1].severity));
    }

    #[test]
    fn platform_key_mapping() {
        assert_eq!(platform_key(Platform::Windows), "windows");
        assert_eq!(platform_key(Platform::Macos), "macos");
        assert_eq!(platform_key(Platform::Linux), "macos");
        assert_eq!(platform_key(Platform::Unknown), "macos");
    }

    #[test]
    fn empty_request_yields_nothing() {
        assert!(run(vec![], None).is_empty());
        assert!(run(vec![], Some(snapshot(Platform::Windows))).is_empty());
    }
}
