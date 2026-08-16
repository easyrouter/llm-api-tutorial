//! Redacted diagnostic report (Markdown). See parent module docs.
//!
//! Sections, in order: header (generated, app version, platform / arch / OS version / build /
//! shell, config source, gateway preset), `Checks` table (id | status | code | details),
//! `Tools` (tool | installed | version | path | on PATH), `Environment variables`
//! (name | present | masked value | sources), `Verification` (per verified tool: CLI ok /
//! version / path / error class / redacted output tail, gateway HTTP status / latency / error
//! class / redacted server message, running terminal names — the raw error information M5
//! asks for even when no rule matched), `Diagnoses` (rule | severity | code | params),
//! `Config presence (read-only)` and `Notes`.
//!
//! Config presence never reveals values: for every `ToolSpec.config_dir` it reports whether the
//! directory exists; for `~/.codex/config.toml` the top-level key names and, for
//! `[model_providers.*]`, the provider names plus whether `base_url` is set; for
//! `~/.claude/settings.json` the top-level key names; for the CC Switch data dir the file
//! names. Nothing under those directories is ever written (ADR-0003).
//!
//! Every field is passed through [`redact_secrets`] as it is rendered, and the final Markdown
//! once more. Section titles are English (the report is a support artefact pasted into
//! tickets), while all machine facts stay verbatim codes.

use std::fmt::Write as _;
use std::path::Path;

use serde::Serialize;

use crate::models::{
    AppConfig, AppInfo, CheckResult, Diagnosis, DiagnosticReport, EnvSnapshot, EnvVarFinding,
    ToolId, ToolInfo, VerifyResult,
};
use crate::platform::expand_tilde;
use crate::redact::redact_secrets;

/// Maximum number of file names listed for the CC Switch data directory.
const MAX_LISTED_FILES: usize = 40;

pub struct ReportInput<'a> {
    pub app: &'a AppInfo,
    pub config: &'a AppConfig,
    pub snapshot: Option<&'a EnvSnapshot>,
    pub diagnoses: &'a [Diagnosis],
    /// Latest verification result per tool (already redacted by `verify`); may be empty.
    pub verify: &'a [VerifyResult],
}

impl std::fmt::Debug for ReportInput<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReportInput")
            .field("app", &self.app.version)
            .finish_non_exhaustive()
    }
}

/// Renders the fully redacted Markdown report.
pub fn build(input: &ReportInput<'_>) -> DiagnosticReport {
    let generated_at = chrono::Utc::now().to_rfc3339();
    let mut md = String::new();
    header(&mut md, input, &generated_at);
    checks_section(&mut md, input.snapshot);
    tools_section(&mut md, input.snapshot);
    env_vars_section(&mut md, input.snapshot);
    verification_section(&mut md, input.verify);
    diagnoses_section(&mut md, input.diagnoses);
    config_presence_section(&mut md, input.config);
    notes_section(&mut md);
    DiagnosticReport {
        generated_at,
        app_version: redact_secrets(&input.app.version),
        markdown: redact_secrets(&md),
    }
}

// ---------------------------------------------------------------------------
// Sections
// ---------------------------------------------------------------------------

fn header(md: &mut String, input: &ReportInput<'_>, generated_at: &str) {
    md.push_str("# SeedRouter Onboarding diagnostic report\n\n");
    push_kv(md, "generated", generated_at);
    push_kv(
        md,
        "app",
        &format!("{} {}", input.app.name, input.app.version),
    );
    push_kv(md, "config source", &wire(&input.app.config_source));
    push_kv(md, "company", &input.config.company.name);
    push_kv(md, "gateway preset", &input.config.gateway.base_url);
    if let Some(snap) = input.snapshot {
        push_kv(md, "platform", &wire(&snap.os.platform));
        push_kv(md, "arch", &snap.os.arch);
        push_kv(md, "os version", &snap.os.version);
        push_kv(md, "build", &opt(snap.os.build.map(|b| b.to_string())));
        push_kv(md, "shell", &snap.os.shell);
        push_kv(md, "admin", &opt(snap.os.is_admin.map(|b| b.to_string())));
        push_kv(md, "snapshot", &snap.generated_at);
        push_kv(md, "overall", &wire(&snap.overall));
    } else {
        push_kv(md, "platform", &wire(&input.app.platform));
        push_kv(md, "arch", &input.app.arch);
        push_kv(md, "snapshot", "none");
    }
    md.push('\n');
}

fn checks_section(md: &mut String, snapshot: Option<&EnvSnapshot>) {
    md.push_str("## Checks\n\n");
    let Some(snap) = snapshot.filter(|s| !s.checks.is_empty()) else {
        md.push_str("_no check results_\n\n");
        return;
    };
    table_header(md, &["id", "status", "code", "details"]);
    for c in &snap.checks {
        table_row(
            md,
            &[
                wire(&c.id),
                wire(&c.status),
                c.code.clone(),
                check_details(c),
            ],
        );
    }
    md.push('\n');
}

fn check_details(c: &CheckResult) -> String {
    let mut parts: Vec<String> = c.params.iter().map(|(k, v)| format!("{k}={v}")).collect();
    parts.extend(c.details.iter().cloned());
    parts.join("; ")
}

fn tools_section(md: &mut String, snapshot: Option<&EnvSnapshot>) {
    md.push_str("## Tools\n\n");
    let Some(snap) = snapshot.filter(|s| !s.tools.is_empty()) else {
        md.push_str("_no tool information_\n\n");
        return;
    };
    table_header(md, &["tool", "installed", "version", "path", "on PATH"]);
    for t in &snap.tools {
        table_row(md, &tool_row(t));
    }
    md.push('\n');
}

fn tool_row(t: &ToolInfo) -> [String; 5] {
    [
        wire(&t.id),
        t.installed.to_string(),
        opt(t.version.clone()),
        opt(t.path.clone()),
        t.on_path.to_string(),
    ]
}

fn env_vars_section(md: &mut String, snapshot: Option<&EnvSnapshot>) {
    md.push_str("## Environment variables\n\n");
    let Some(snap) = snapshot.filter(|s| !s.env_vars.is_empty()) else {
        md.push_str("_no environment variable findings_\n\n");
        return;
    };
    table_header(md, &["name", "present", "value (masked)", "sources"]);
    for f in &snap.env_vars {
        table_row(md, &env_var_row(f));
    }
    md.push('\n');
}

fn env_var_row(f: &EnvVarFinding) -> [String; 4] {
    let sources = f
        .sources
        .iter()
        .map(|s| format!("{}: {}", wire(&s.kind), s.location))
        .collect::<Vec<_>>()
        .join("; ");
    [
        f.name.clone(),
        f.present_in_session.to_string(),
        opt(f.value_masked.clone()),
        sources,
    ]
}

fn verification_section(md: &mut String, results: &[VerifyResult]) {
    md.push_str("## Verification\n\n");
    if results.is_empty() {
        md.push_str("_not run_\n\n");
        return;
    }
    for r in results {
        let _ = writeln!(md, "### {}\n", wire(&r.tool));
        push_kv(md, "ok", &r.ok.to_string());
        push_kv(md, "cli ok", &r.cli.ok.to_string());
        push_kv(md, "cli version", &opt(r.cli.version.clone()));
        push_kv(md, "cli path", &opt(r.cli.path.clone()));
        push_kv(
            md,
            "cli error class",
            &opt(r.cli.error_class.map(|c| wire(&c))),
        );
        push_kv(md, "cli output (redacted)", &opt(r.cli.output_tail.clone()));
        match &r.gateway {
            Some(g) => {
                push_kv(md, "gateway ok", &g.ok.to_string());
                push_kv(
                    md,
                    "gateway http status",
                    &opt(g.http_status.map(|s| s.to_string())),
                );
                push_kv(
                    md,
                    "gateway latency ms",
                    &opt(g.latency_ms.map(|l| l.to_string())),
                );
                push_kv(
                    md,
                    "gateway error class",
                    &opt(g.error_class.map(|c| wire(&c))),
                );
                push_kv(md, "gateway message (redacted)", &opt(g.message.clone()));
            }
            None => push_kv(md, "gateway", "not tested"),
        }
        let terminals: Vec<String> = r.running_terminals.iter().map(|t| t.name.clone()).collect();
        push_kv(md, "running terminals", &list_or_none(&terminals));
        md.push('\n');
    }
}

fn diagnoses_section(md: &mut String, diagnoses: &[Diagnosis]) {
    md.push_str("## Diagnoses\n\n");
    if diagnoses.is_empty() {
        md.push_str("_none_\n\n");
        return;
    }
    table_header(md, &["rule", "severity", "code", "params"]);
    for d in diagnoses {
        let params = d
            .params
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; ");
        table_row(
            md,
            &[d.rule_id.clone(), wire(&d.severity), d.code.clone(), params],
        );
    }
    md.push('\n');
}

fn config_presence_section(md: &mut String, config: &AppConfig) {
    md.push_str("## Config presence (read-only)\n\n");
    for tool in &config.tools {
        let dir = expand_tilde(&tool.config_dir);
        for line in inspect_tool_dir(tool.id, &tool.config_dir, &dir) {
            md.push_str(&redact_secrets(&line));
            md.push('\n');
        }
    }
    let cc_dir = expand_tilde(&config.cc_switch.data_dir);
    for line in inspect_cc_switch_dir(&config.cc_switch.data_dir, &cc_dir) {
        md.push_str(&redact_secrets(&line));
        md.push('\n');
    }
    md.push('\n');
}

fn notes_section(md: &mut String) {
    md.push_str("## Notes\n\n");
    md.push_str("- Generated locally by SeedRouter Onboarding; nothing was sent anywhere.\n");
    md.push_str("- API keys and other secrets are never included; values are masked or omitted.\n");
    md.push_str("- Config files were only read; the app never writes to ~/.codex, ~/.claude or ~/.cc-switch.\n");
}

// ---------------------------------------------------------------------------
// Read-only config inspection (pure over a path; testable with a temp dir)
// ---------------------------------------------------------------------------

/// Summary of a Codex `config.toml`: only names, never values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TomlSummary {
    pub top_level_keys: Vec<String>,
    pub providers: Vec<ProviderSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSummary {
    pub name: String,
    pub has_base_url: bool,
}

/// Parses Codex `config.toml` content and lists top-level key names plus
/// `[model_providers.<name>]` entries with a `base_url` presence flag. `None` when unparsable.
pub fn summarize_codex_config(content: &str) -> Option<TomlSummary> {
    let table: toml::Table = toml::from_str(content).ok()?;
    let mut top_level_keys: Vec<String> = table.keys().cloned().collect();
    top_level_keys.sort();
    let mut providers: Vec<ProviderSummary> = table
        .get("model_providers")
        .and_then(toml::Value::as_table)
        .map(|providers| {
            providers
                .iter()
                .map(|(name, value)| ProviderSummary {
                    name: name.clone(),
                    has_base_url: value.as_table().is_some_and(|t| t.contains_key("base_url")),
                })
                .collect()
        })
        .unwrap_or_default();
    providers.sort_by(|a, b| a.name.cmp(&b.name));
    Some(TomlSummary {
        top_level_keys,
        providers,
    })
}

/// Top-level key names of a JSON object document (e.g. `~/.claude/settings.json`); `None`
/// when the content is not a JSON object.
pub fn json_top_level_keys(content: &str) -> Option<Vec<String>> {
    let value: serde_json::Value = serde_json::from_str(content).ok()?;
    let mut keys: Vec<String> = value.as_object()?.keys().cloned().collect();
    keys.sort();
    Some(keys)
}

/// Markdown lines describing a tool's config directory (`label` is the unexpanded path shown
/// to the user, `dir` the resolved path that is inspected).
pub fn inspect_tool_dir(tool: ToolId, label: &str, dir: &Path) -> Vec<String> {
    let mut lines = vec![format!("- {label}: {}", presence(dir.is_dir()))];
    if !dir.is_dir() {
        return lines;
    }
    match tool {
        ToolId::Codex => {
            lines.extend(describe_codex_config(&dir.join("config.toml")));
            lines.push(format!(
                "  - auth.json: {}",
                presence(dir.join("auth.json").is_file())
            ));
        }
        ToolId::ClaudeCode => {
            lines.extend(describe_claude_settings(&dir.join("settings.json")));
        }
    }
    lines
}

fn describe_codex_config(path: &Path) -> Vec<String> {
    let Some(content) = read_if_file(path) else {
        return vec!["  - config.toml: absent".to_owned()];
    };
    let Some(summary) = summarize_codex_config(&content) else {
        return vec!["  - config.toml: present (unparsable)".to_owned()];
    };
    let mut lines = vec![format!(
        "  - config.toml: present; keys: {}",
        list_or_none(&summary.top_level_keys)
    )];
    if !summary.providers.is_empty() {
        let providers: Vec<String> = summary
            .providers
            .iter()
            .map(|p| {
                format!(
                    "{} (base_url {})",
                    p.name,
                    if p.has_base_url { "set" } else { "missing" }
                )
            })
            .collect();
        lines.push(format!("  - model_providers: {}", providers.join(", ")));
    }
    lines
}

fn describe_claude_settings(path: &Path) -> Vec<String> {
    let Some(content) = read_if_file(path) else {
        return vec!["  - settings.json: absent".to_owned()];
    };
    match json_top_level_keys(&content) {
        Some(keys) => vec![format!(
            "  - settings.json: present; keys: {}",
            list_or_none(&keys)
        )],
        None => vec!["  - settings.json: present (unparsable)".to_owned()],
    }
}

/// Markdown lines for the CC Switch data directory: presence and file names only.
pub fn inspect_cc_switch_dir(label: &str, dir: &Path) -> Vec<String> {
    let mut lines = vec![format!("- {label}: {}", presence(dir.is_dir()))];
    if !dir.is_dir() {
        return lines;
    }
    let mut names = file_names(dir);
    let total = names.len();
    names.truncate(MAX_LISTED_FILES);
    let mut listed = list_or_none(&names);
    if total > MAX_LISTED_FILES {
        let _ = write!(listed, ", … ({total} entries)");
    }
    lines.push(format!("  - entries: {listed}"));
    lines
}

/// Sorted entry names (files and sub-directories, one level) of `dir`.
fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn read_if_file(path: &Path) -> Option<String> {
    path.is_file()
        .then(|| std::fs::read_to_string(path).ok())
        .flatten()
}

fn presence(present: bool) -> &'static str {
    if present {
        "present"
    } else {
        "absent"
    }
}

fn list_or_none(items: &[String]) -> String {
    if items.is_empty() {
        "(none)".to_owned()
    } else {
        items.join(", ")
    }
}

// ---------------------------------------------------------------------------
// Markdown helpers
// ---------------------------------------------------------------------------

/// Wire (serde) form of an enum, e.g. `Platform::Windows` → `windows`.
fn wire<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn opt(value: Option<String>) -> String {
    value.unwrap_or_else(|| "-".to_owned())
}

fn push_kv(md: &mut String, key: &str, value: &str) {
    let _ = writeln!(md, "- {key}: {}", cell(value));
}

fn table_header(md: &mut String, columns: &[&str]) {
    let _ = writeln!(md, "| {} |", columns.join(" | "));
    let separators: String = columns.iter().map(|_| " --- |").collect();
    let _ = writeln!(md, "|{separators}");
}

fn table_row(md: &mut String, cells: &[String]) {
    let rendered: Vec<String> = cells.iter().map(|c| cell(c)).collect();
    let _ = writeln!(md, "| {} |", rendered.join(" | "));
}

/// Redacts and makes a value safe inside a Markdown table cell / list item.
fn cell(value: &str) -> String {
    let redacted = redact_secrets(value);
    let flat = redacted.replace(['\r', '\n'], " ").replace('|', "\\|");
    if flat.trim().is_empty() {
        "-".to_owned()
    } else {
        flat.trim().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::models::{
        CheckId, CheckStatus, ConfigSource, EnvVarSource, EnvVarSourceKind, FixAction, OsInfo,
        Params, Platform, Severity,
    };

    fn app() -> AppInfo {
        AppInfo {
            name: "seedrouter-onboarding".into(),
            version: "0.1.0".into(),
            platform: Platform::Windows,
            arch: "x86_64".into(),
            locale_hint: "en".into(),
            config_source: ConfigSource::Bundled,
            log_dir: "C:\\logs".into(),
        }
    }

    fn snapshot() -> EnvSnapshot {
        EnvSnapshot {
            os: OsInfo {
                platform: Platform::Windows,
                version: "10.0.19045".into(),
                build: Some(19045),
                arch: "x86_64".into(),
                shell: "cmd.exe".into(),
                home_dir: "C:\\Users\\me".into(),
                is_admin: Some(false),
            },
            tools: vec![ToolInfo {
                id: ToolId::Codex,
                installed: true,
                version: Some("0.50.0".into()),
                path: Some("C:\\Users\\me\\AppData\\Roaming\\npm\\codex.cmd".into()),
                on_path: false,
                npm_global_bin: Some("C:\\Users\\me\\AppData\\Roaming\\npm".into()),
            }],
            env_vars: vec![EnvVarFinding {
                name: "OPENAI_API_KEY".into(),
                present_in_session: true,
                value_masked: Some("sk-****abcd".into()),
                sources: vec![EnvVarSource {
                    kind: EnvVarSourceKind::UserRegistry,
                    location: "HKCU\\Environment".into(),
                }],
            }],
            checks: vec![CheckResult {
                id: CheckId::EnvVars,
                status: CheckStatus::Warn,
                code: "env_vars.conflicts".into(),
                params: Params::from([("names".to_owned(), "OPENAI_API_KEY".to_owned())]),
                // Simulated leak in a free-text detail: must be redacted by the report.
                details: vec![
                    "OPENAI_API_KEY=sk-abcdefghijklmnop1234 | pipe".into(),
                    "Authorization: Bearer abcdefghijklmnopqrstuvwxyz".into(),
                ],
                fixes: vec![FixAction::Rerun],
                duration_ms: 12,
            }],
            overall: CheckStatus::Warn,
            generated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    fn diagnosis() -> Diagnosis {
        Diagnosis {
            rule_id: "D".into(),
            severity: Severity::Warning,
            code: "env.conflict".into(),
            params: Params::from([("names".to_owned(), "OPENAI_API_KEY".to_owned())]),
            actions: vec![],
            checklist: vec!["inspect".into()],
        }
    }

    #[test]
    fn report_has_all_sections_and_no_raw_secrets() {
        let cfg = config::embedded().expect("config");
        let snap = snapshot();
        let diagnoses = vec![diagnosis()];
        let report = build(&ReportInput {
            app: &app(),
            config: &cfg,
            snapshot: Some(&snap),
            diagnoses: &diagnoses,
            verify: &[],
        });
        let md = &report.markdown;
        for section in [
            "# SeedRouter Onboarding diagnostic report",
            "## Checks",
            "## Tools",
            "## Environment variables",
            "## Verification",
            "## Diagnoses",
            "## Config presence (read-only)",
            "## Notes",
        ] {
            assert!(md.contains(section), "missing {section}:\n{md}");
        }
        assert!(md.contains("- platform: windows"), "{md}");
        assert!(md.contains("- build: 19045"), "{md}");
        assert!(md.contains("- shell: cmd.exe"), "{md}");
        assert!(
            md.contains("| env_vars | warn | env_vars.conflicts |"),
            "{md}"
        );
        assert!(md.contains("| codex | true | 0.50.0 |"), "{md}");
        assert!(
            md.contains(
                "| OPENAI_API_KEY | true | sk-****abcd | user_registry: HKCU\\Environment |"
            ),
            "{md}"
        );
        assert!(
            md.contains("| D | warning | env.conflict | names=OPENAI_API_KEY |"),
            "{md}"
        );

        // Secrets: masked value survives, raw ones do not.
        assert!(!md.contains("sk-abcdefghijklmnop1234"), "{md}");
        assert!(!md.contains("abcdefghijklmnopqrstuvwxyz"), "{md}");
        let raw_sk = md
            .match_indices("sk-")
            .filter(|(i, _)| !md[*i..].starts_with("sk-****"))
            .count();
        assert_eq!(raw_sk, 0, "unmasked sk- occurrences:\n{md}");
        // Pipes inside cells are escaped so the table stays intact.
        assert!(md.contains("\\| pipe"), "{md}");
        assert_eq!(report.app_version, "0.1.0");
        assert!(!report.generated_at.is_empty());
    }

    #[test]
    fn report_without_snapshot_still_renders() {
        let cfg = config::embedded().expect("config");
        let report = build(&ReportInput {
            app: &app(),
            config: &cfg,
            snapshot: None,
            diagnoses: &[],
            verify: &[],
        });
        let md = &report.markdown;
        assert!(md.contains("- snapshot: none"), "{md}");
        assert!(md.contains("_no check results_"), "{md}");
        assert!(md.contains("_no tool information_"), "{md}");
        assert!(md.contains("_no environment variable findings_"), "{md}");
        assert!(md.contains("## Verification\n\n_not run_"), "{md}");
        assert!(md.contains("## Diagnoses\n\n_none_"), "{md}");
    }

    #[test]
    fn report_renders_verification_details_redacted() {
        use crate::models::{CliCheck, ErrorClass, GatewayCheck, TerminalProcess};
        let cfg = config::embedded().expect("config");
        let key = "sk-abcdefghijklmnopqrstuvwxyz0123456789";
        let result = VerifyResult {
            tool: ToolId::ClaudeCode,
            cli: CliCheck {
                ok: false,
                version: None,
                path: Some("C:\\npm\\claude.cmd".into()),
                output_tail: Some(format!("Error: bad key {key}")),
                error_class: Some(ErrorClass::CommandFailed),
            },
            gateway: Some(GatewayCheck {
                ok: false,
                http_status: Some(401),
                latency_ms: Some(120),
                error_class: Some(ErrorClass::Auth),
                message: Some("Incorrect API key provided: [REDACTED]".into()),
            }),
            running_terminals: vec![TerminalProcess {
                pid: 1,
                name: "WindowsTerminal.exe".into(),
            }],
            ok: false,
            diagnoses: Vec::new(),
        };
        let report = build(&ReportInput {
            app: &app(),
            config: &cfg,
            snapshot: None,
            diagnoses: &[],
            verify: &[result],
        });
        let md = &report.markdown;
        assert!(md.contains("### claude-code"), "{md}");
        assert!(md.contains("- cli error class: command_failed"), "{md}");
        assert!(md.contains("- gateway http status: 401"), "{md}");
        assert!(md.contains("- gateway error class: auth"), "{md}");
        assert!(md.contains("- gateway latency ms: 120"), "{md}");
        assert!(
            md.contains("- running terminals: WindowsTerminal.exe"),
            "{md}"
        );
        assert!(!md.contains(key), "key must be redacted: {md}");
    }

    const SAMPLE_TOML: &str = r#"
model = "gpt-5"
model_provider = "company"
model_reasoning_effort = "medium"

[model_providers.company]
name = "Service Gateway"
base_url = "https://gateway.internal.example.com/v1"
wire_api = "responses"
env_key = "OPENAI_API_KEY"

[model_providers.other]
name = "Other"
"#;

    #[test]
    fn codex_summary_lists_names_not_values() {
        let summary = summarize_codex_config(SAMPLE_TOML).expect("parse");
        assert_eq!(
            summary.top_level_keys,
            vec![
                "model",
                "model_provider",
                "model_providers",
                "model_reasoning_effort"
            ]
        );
        assert_eq!(
            summary.providers,
            vec![
                ProviderSummary {
                    name: "company".into(),
                    has_base_url: true
                },
                ProviderSummary {
                    name: "other".into(),
                    has_base_url: false
                },
            ]
        );
        assert!(summarize_codex_config("not = = toml").is_none());
        let empty = summarize_codex_config("").expect("empty toml parses");
        assert!(empty.top_level_keys.is_empty());
        assert!(empty.providers.is_empty());
    }

    #[test]
    fn json_keys_only_for_objects() {
        assert_eq!(
            json_top_level_keys(r#"{"env": {"ANTHROPIC_API_KEY": "sk-abc"}, "apiKeyHelper": "x"}"#),
            Some(vec!["apiKeyHelper".to_owned(), "env".to_owned()])
        );
        assert_eq!(json_top_level_keys("[1,2]"), None);
        assert_eq!(json_top_level_keys("{"), None);
    }

    #[test]
    fn inspect_codex_dir_on_temp_dir_lists_names_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("config.toml"), SAMPLE_TOML).expect("write");
        std::fs::write(
            dir.path().join("auth.json"),
            r#"{"OPENAI_API_KEY":"sk-abcdefghijklmnop"}"#,
        )
        .expect("write");
        let lines = inspect_tool_dir(ToolId::Codex, "~/.codex", dir.path());
        let joined = lines.join("\n");
        assert!(joined.starts_with("- ~/.codex: present"), "{joined}");
        assert!(joined.contains("config.toml: present; keys: model, model_provider, model_providers, model_reasoning_effort"), "{joined}");
        assert!(
            joined.contains("model_providers: company (base_url set), other (base_url missing)"),
            "{joined}"
        );
        assert!(joined.contains("auth.json: present"), "{joined}");
        for secret in [
            "gpt-5",
            "gateway.internal",
            "sk-abcdefghijklmnop",
            "Service Gateway",
            "responses",
        ] {
            assert!(!joined.contains(secret), "leaked {secret}:\n{joined}");
        }
    }

    #[test]
    fn inspect_claude_dir_on_temp_dir_lists_keys_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("settings.json"),
            r#"{"env": {"ANTHROPIC_BASE_URL": "https://x/v1", "ANTHROPIC_AUTH_TOKEN": "sk-ant-abcdefghijklmnop"}, "permissions": {}}"#,
        )
        .expect("write");
        let lines = inspect_tool_dir(ToolId::ClaudeCode, "~/.claude", dir.path());
        let joined = lines.join("\n");
        assert!(
            joined.contains("settings.json: present; keys: env, permissions"),
            "{joined}"
        );
        assert!(!joined.contains("ANTHROPIC"), "{joined}");
        assert!(!joined.contains("sk-ant"), "{joined}");
    }

    #[test]
    fn inspect_absent_and_unparsable_files() {
        let missing = Path::new("definitely/not/a/real/dir/for/seedrouter-onboarding");
        assert_eq!(
            inspect_tool_dir(ToolId::Codex, "~/.codex", missing),
            vec!["- ~/.codex: absent".to_owned()]
        );
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            inspect_tool_dir(ToolId::Codex, "~/.codex", dir.path()),
            vec![
                "- ~/.codex: present".to_owned(),
                "  - config.toml: absent".to_owned(),
                "  - auth.json: absent".to_owned(),
            ]
        );
        std::fs::write(dir.path().join("config.toml"), "this is = not [ toml").expect("write");
        let lines = inspect_tool_dir(ToolId::Codex, "~/.codex", dir.path());
        assert!(
            lines
                .iter()
                .any(|l| l.contains("config.toml: present (unparsable)")),
            "{lines:?}"
        );
    }

    #[test]
    fn inspect_cc_switch_dir_lists_file_names_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("config.json"),
            r#"{"key":"sk-abcdefghijklmnop"}"#,
        )
        .expect("write");
        std::fs::create_dir(dir.path().join("backups")).expect("mkdir");
        let lines = inspect_cc_switch_dir("~/.cc-switch", dir.path());
        assert_eq!(
            lines,
            vec![
                "- ~/.cc-switch: present".to_owned(),
                "  - entries: backups, config.json".to_owned(),
            ]
        );
        assert_eq!(
            inspect_cc_switch_dir("~/.cc-switch", Path::new("no/such/dir/cc-switch")),
            vec!["- ~/.cc-switch: absent".to_owned()]
        );
    }

    #[test]
    fn cell_escapes_and_redacts() {
        assert_eq!(cell("a|b\nc"), "a\\|b c");
        assert_eq!(cell("   "), "-");
        assert_eq!(cell("key sk-abcdefghijklmnop"), "key [REDACTED]");
        assert_eq!(wire(&Platform::Macos), "macos");
        assert_eq!(wire(&CheckStatus::Pass), "pass");
    }
}
