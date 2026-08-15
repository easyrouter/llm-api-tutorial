//! Data transfer objects shared between the Rust core and the web frontend.
//!
//! Every type here is serialised with `camelCase` field names and `snake_case`
//! enum variants so that `src/lib/types.ts` can mirror it 1:1. **Keep the two
//! files in sync** — the TypeScript side is the contract the UI codes against.
//!
//! Design rules
//! - Rust never returns human-readable prose. It returns *codes* (`code`,
//!   `rule_id`, …) plus interpolation `params`; the frontend resolves them via
//!   i18n so every message exists in both zh-CN and en.
//! - Nothing in this module may carry an API key or any other secret. Values
//!   that could be secrets are always pre-masked (`value_masked`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// String map used for i18n interpolation parameters.
pub type Params = BTreeMap<String, String>;

// ---------------------------------------------------------------------------
// App / config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub platform: Platform,
    pub arch: String,
    pub locale_hint: String,
    pub config_source: ConfigSource,
    pub log_dir: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSource {
    Bundled,
    BundledWithOverride,
    Fallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Windows,
    Macos,
    Linux,
    Unknown,
}

/// Company preset shipped as `resources/app-config.json` (+ optional user override).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub schema_version: u32,
    pub company: CompanyInfo,
    pub gateway: GatewayPreset,
    pub tools: Vec<ToolSpec>,
    pub cc_switch: CcSwitchSpec,
    pub requirements: Requirements,
    pub mirrors: Mirrors,
    pub env_vars_to_inspect: Vec<String>,
    pub docs: DocsConfig,
    pub telemetry: TelemetryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanyInfo {
    pub name: String,
    #[serde(default)]
    pub support_contact: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayPreset {
    pub base_url: String,
    pub protocol: Protocol,
    pub preset_provider_name: String,
    #[serde(default)]
    pub default_model: String,
    #[serde(default)]
    pub default_reasoning_effort: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Responses,
    ChatCompletions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolId {
    Codex,
    ClaudeCode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSpec {
    pub id: ToolId,
    pub display_name: String,
    pub npm_package: String,
    pub binary: String,
    pub version_args: Vec<String>,
    pub config_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CcSwitchSpec {
    pub github_repo: String,
    pub releases_api: String,
    pub download_page: String,
    #[serde(default)]
    pub intranet_mirror: String,
    pub data_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Requirements {
    pub node_min_version: String,
    pub node_recommended_lts: String,
    pub os: OsRequirements,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OsRequirements {
    pub windows: WindowsRequirement,
    pub macos: MacosRequirement,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowsRequirement {
    pub min_build: u32,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacosRequirement {
    pub min_version: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mirrors {
    pub npm_registries: Vec<MirrorEntry>,
    pub node_dist: Vec<MirrorEntry>,
    pub probe_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorEntry {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub download_page: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocsConfig {
    pub base_url: String,
    pub index_path: String,
    pub cache_ttl_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryConfig {
    pub enabled: bool,
    #[serde(default)]
    pub endpoint: String,
    pub flush_interval_seconds: u64,
}

// ---------------------------------------------------------------------------
// M1 — environment checks
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckId {
    Os,
    Node,
    Npm,
    Codex,
    ClaudeCode,
    CcSwitch,
    EnvVars,
    NetworkNpm,
    NetworkGateway,
    NetworkGithub,
}

impl CheckId {
    pub const ALL: [CheckId; 10] = [
        CheckId::Os,
        CheckId::Node,
        CheckId::Npm,
        CheckId::Codex,
        CheckId::ClaudeCode,
        CheckId::CcSwitch,
        CheckId::EnvVars,
        CheckId::NetworkNpm,
        CheckId::NetworkGateway,
        CheckId::NetworkGithub,
    ];
}

/// Three-state outcome required by the PRD (通过 / 警告 / 阻断) plus bookkeeping states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub id: CheckId,
    pub status: CheckStatus,
    /// i18n code, e.g. `node.too_old`; resolved as `checks.<code>` on the frontend.
    pub code: String,
    pub params: Params,
    /// Raw, non-secret facts worth showing verbatim (paths, versions, URLs).
    pub details: Vec<String>,
    pub fixes: Vec<FixAction>,
    pub duration_ms: u64,
}

/// A remediation the UI can offer. Never performs anything silently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FixAction {
    /// Open a URL in the system browser (download pages, docs).
    OpenUrl { url: String, label_code: String },
    /// Jump to a wizard step.
    GoToStep { step: WizardStep },
    /// Offer to run an install plan (still requires user confirmation).
    Install { tool: InstallTarget },
    /// Show static instructions (i18n code) — used for things the tool must not do itself
    /// (e.g. editing environment variables, PRD #17).
    Instructions { code: String, params: Params },
    /// Re-run this check.
    Rerun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WizardStep {
    Welcome,
    EnvCheck,
    Install,
    Configure,
    Verify,
    Diagnose,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallTarget {
    Node,
    Codex,
    ClaudeCode,
    CcSwitch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OsInfo {
    pub platform: Platform,
    pub version: String,
    pub build: Option<u64>,
    pub arch: String,
    pub shell: String,
    pub home_dir: String,
    pub is_admin: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub id: ToolId,
    pub installed: bool,
    pub version: Option<String>,
    /// Resolved executable path, if found.
    pub path: Option<String>,
    /// `true` when found through the current `PATH`; `false` when only found in the npm
    /// global bin directory (=> PATH not refreshed, guide fault E).
    pub on_path: bool,
    pub npm_global_bin: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvVarFinding {
    pub name: String,
    pub present_in_session: bool,
    /// Masked value such as `sk-****abcd`; never the raw value.
    pub value_masked: Option<String>,
    pub sources: Vec<EnvVarSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvVarSource {
    pub kind: EnvVarSourceKind,
    /// Human-readable location, e.g. `HKCU\Environment` or `~/.zshrc:12`.
    pub location: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvVarSourceKind {
    Process,
    UserRegistry,
    MachineRegistry,
    ShellRc,
    Launchctl,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub id: String,
    pub url: String,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub http_status: Option<u16>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorChoice {
    pub npm_registry: MirrorEntry,
    pub node_dist: MirrorEntry,
    pub probes: Vec<ProbeResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvSnapshot {
    pub os: OsInfo,
    pub tools: Vec<ToolInfo>,
    pub env_vars: Vec<EnvVarFinding>,
    pub checks: Vec<CheckResult>,
    pub overall: CheckStatus,
    pub generated_at: String,
}

// ---------------------------------------------------------------------------
// M2 — installation
// ---------------------------------------------------------------------------

/// Fully-resolved command the app *proposes* to run. The UI must display
/// `display_command` and obtain explicit confirmation before `start_install`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPlan {
    pub target: InstallTarget,
    pub program: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub display_command: String,
    pub registry: Option<MirrorEntry>,
    pub requires_admin: bool,
    /// i18n code explaining what will happen (e.g. `install.plan.npm_global`).
    pub explanation_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallJob {
    pub job_id: String,
    pub target: InstallTarget,
}

/// Emitted on the `install://output` event channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallOutputEvent {
    pub job_id: String,
    pub stream: OutputStream,
    pub line: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputStream {
    Stdout,
    Stderr,
    System,
}

/// Emitted on the `install://done` event channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallDoneEvent {
    pub job_id: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub cancelled: bool,
}

/// Emitted on the `download://progress` event channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgressEvent {
    pub job_id: String,
    pub downloaded: u64,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    pub job_id: String,
    pub url: String,
    pub file_name: String,
    pub expected_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResult {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    /// `Some(true|false)` when an expected hash was supplied, `None` otherwise.
    pub verified: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CcSwitchRelease {
    pub version: String,
    pub asset_name: String,
    pub download_url: String,
    pub sha256: Option<String>,
    pub source: String,
}

// ---------------------------------------------------------------------------
// M3 — configuration guide (positioning A: the user configures inside CC Switch)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigGuide {
    pub tool: ToolId,
    pub preset: ProviderPreset,
    pub steps: Vec<GuideStep>,
}

/// Values the user copies into CC Switch. Base URL is company-wide; the rest are hints.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPreset {
    pub provider_name: String,
    pub base_url: String,
    pub protocol: Protocol,
    pub model_hint: String,
    pub reasoning_effort_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideStep {
    pub id: String,
    /// i18n code prefix; frontend reads `guide.<code>.title` / `.body`.
    pub code: String,
    pub params: Params,
    /// Copyable value shown next to the step (e.g. base URL), if any.
    pub copy_value: Option<String>,
    /// Check that auto-verifies completion of this step, if any.
    pub verify_check: Option<CheckId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyValidation {
    pub valid: bool,
    pub issues: Vec<KeyIssue>,
    /// Trimmed length only — never the key itself.
    pub length: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyIssue {
    Empty,
    LeadingOrTrailingWhitespace,
    ContainsWhitespace,
    ContainsNewline,
    NonAscii,
    UnexpectedPrefix,
    TooShort,
    LooksLikePlaceholder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlPreview {
    pub input: String,
    pub effective_url: String,
    pub rule: UrlRule,
    pub warnings: Vec<UrlWarning>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UrlRule {
    /// Path already ends in `/v1` — used as-is.
    AlreadyVersioned,
    /// Bare origin — `/v1` appended.
    AppendedV1,
    /// Ends with `#` — used literally, no suffix appended.
    LiteralHash,
    /// Custom path kept as-is (may be intentional).
    CustomPath,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UrlWarning {
    NotHttps,
    TrailingSlashRemoved,
    ContainsWhitespace,
    ContainsCredentials,
    LooksLikeChatCompletionsEndpoint,
    DiffersFromCompanyGateway,
}

// ---------------------------------------------------------------------------
// M4 — verification
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyRequest {
    pub tool: ToolId,
    /// Optional live gateway probe. Held in memory for the duration of the request only.
    pub gateway: Option<GatewayProbeRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayProbeRequest {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub protocol: Protocol,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResult {
    pub tool: ToolId,
    pub cli: CliCheck,
    pub gateway: Option<GatewayCheck>,
    pub running_terminals: Vec<TerminalProcess>,
    pub ok: bool,
    pub diagnoses: Vec<Diagnosis>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliCheck {
    pub ok: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    /// Redacted stderr/stdout tail when not ok.
    pub output_tail: Option<String>,
    pub error_class: Option<ErrorClass>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayCheck {
    pub ok: bool,
    pub http_status: Option<u16>,
    pub latency_ms: Option<u64>,
    pub error_class: Option<ErrorClass>,
    /// Redacted server message (keys, tokens and URLs with credentials stripped).
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorClass {
    Auth,
    NotFound,
    ProtocolMismatch,
    RateLimited,
    ServerError,
    Network,
    Timeout,
    Tls,
    CommandNotFound,
    CommandFailed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalProcess {
    pub pid: u32,
    pub name: String,
}

// ---------------------------------------------------------------------------
// M5 — diagnosis (rule engine covering guide faults A–G)
// ---------------------------------------------------------------------------

/// Observed facts fed into the rule engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Symptom {
    HttpStatus { status: u16, tool: ToolId },
    CommandNotFound { tool: ToolId },
    CommandFailed { tool: ToolId, output_tail: String },
    NoEffectAfterConfig { tool: ToolId },
    NetworkError { target: String },
    Timeout { target: String },
    AppBlockedByOs { app: String },
    EnvVarConflict { name: String },
    ProtocolMismatch { tool: ToolId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Blocking,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnosis {
    /// Guide fault id: `A`…`G`, optionally with a sub-rule suffix (`A1`).
    pub rule_id: String,
    pub severity: Severity,
    /// i18n code prefix; frontend reads `diagnose.<code>.title` / `.explanation`.
    pub code: String,
    pub params: Params,
    pub actions: Vec<FixAction>,
    /// Ordered checklist codes for the user (`diagnose.<code>.steps.<n>`).
    pub checklist: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnoseRequest {
    pub symptoms: Vec<Symptom>,
    pub snapshot: Option<EnvSnapshot>,
}

/// Fully redacted report suitable for pasting into a ticket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticReport {
    pub generated_at: String,
    pub app_version: String,
    pub markdown: String,
}

// ---------------------------------------------------------------------------
// M6 — help documentation (fetched from the designated docs site)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocsIndex {
    pub sections: Vec<DocSection>,
    pub fetched_at: String,
    pub source: DocsSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocSection {
    pub id: String,
    pub title: String,
    pub path: String,
    pub lang: String,
    pub wizard_step: Option<WizardStep>,
    #[serde(default)]
    pub children: Vec<DocSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocPage {
    pub id: String,
    pub title: String,
    pub lang: String,
    pub markdown: String,
    pub source: DocsSource,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocsSource {
    Remote,
    Cache,
    Bundled,
}

// ---------------------------------------------------------------------------
// Telemetry (PRD #16) — aggregate, non-identifying, never contains secrets
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryEvent {
    pub name: String,
    pub step: Option<WizardStep>,
    pub status: Option<String>,
    pub duration_ms: Option<u64>,
    pub error_class: Option<ErrorClass>,
    pub rule_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryStatus {
    pub enabled: bool,
    pub configured: bool,
    pub queued: usize,
    pub session_id: String,
}
