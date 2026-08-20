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
    /// Where the Codex desktop client comes from (Store id, offline MSIX, macOS DMG).
    #[serde(default)]
    pub codex_app: CodexAppSpec,
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

/// Wire protocol of a gateway / provider. `Responses` and `ChatCompletions` are the OpenAI
/// shapes (Codex); `AnthropicMessages` is what Claude Code speaks (`/v1/messages`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Responses,
    ChatCompletions,
    AnthropicMessages,
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

/// Distribution points of the Codex desktop client (`app-config.json` → `codexApp`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexAppSpec {
    /// Microsoft Store product id (Windows).
    #[serde(default)]
    pub store_product_id: String,
    /// Offline Store-signed MSIX for x64 (Windows).
    #[serde(default)]
    pub windows_msix_x64: String,
    /// Offline Store-signed MSIX for arm64 (Windows).
    #[serde(default)]
    pub windows_msix_arm64: String,
    /// Official DMG (macOS, universal).
    #[serde(default)]
    pub macos_dmg: String,
    /// Public download page (fallback link).
    #[serde(default)]
    pub download_page: String,
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
    /// Recommendation only (Windows: is Windows Terminal installed?); `Skipped` elsewhere.
    WindowsTerminal,
    Node,
    Npm,
    Codex,
    ClaudeCode,
    CcSwitch,
    /// Recommendation only: the Codex desktop client (ChatGPT / Codex app) is installed.
    CodexApp,
    EnvVars,
    NetworkNpm,
    NetworkGateway,
    NetworkGithub,
}

impl CheckId {
    pub const ALL: [CheckId; 12] = [
        CheckId::Os,
        CheckId::WindowsTerminal,
        CheckId::Node,
        CheckId::Npm,
        CheckId::Codex,
        CheckId::ClaudeCode,
        CheckId::CcSwitch,
        CheckId::CodexApp,
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

/// A remediation the UI can offer. Never performs anything silently: the one-click variants
/// (`RepairPath`, `CleanEnvVars`) fetch a plan first, show exactly what will change and run
/// only after the user confirms (ADR-0008).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum FixAction {
    /// Open a URL in the system browser (download pages, docs).
    OpenUrl { url: String, label_code: String },
    /// Jump to a wizard step.
    GoToStep { step: WizardStep },
    /// Offer to run an install plan (still requires user confirmation).
    Install { tool: InstallTarget },
    /// Show static instructions (i18n code) — the manual alternative to a one-click fix.
    Instructions { code: String, params: Params },
    /// One-click: add `dir` to the user's persistent `PATH` (`plan_path_repair` →
    /// `apply_path_repair`).
    RepairPath { dir: String },
    /// One-click: remove the listed environment variables from their persistent sources
    /// (`plan_env_cleanup` → `apply_env_cleanup`; the plan shows the full values first).
    CleanEnvVars { names: Vec<String> },
    /// Open a well-known OS URI (Microsoft Store page, Windows region settings).
    OpenSystemUri { uri: SystemUri },
    /// Re-run this check.
    Rerun,
}

/// OS URIs the app may open (`open_system_uri`); the mapping to the actual URI lives in Rust so
/// the webview never passes arbitrary schemes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemUri {
    /// `ms-windows-store://pdp/?productid=<codexApp.windows.storeProductId>`.
    MsStoreCodexApp,
    /// `ms-settings:regionformatting` (Windows "Region" settings page).
    WindowsRegionSettings,
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
    /// Codex desktop client (Microsoft Store / MSIX on Windows, DMG on macOS).
    CodexApp,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Page / file the user downloads manually when the plan has no command (Node.js download
    /// page of the chosen mirror). `None` for command plans and for installer targets (the UI
    /// fetches the release through `fetch_installer_release`).
    #[serde(default)]
    pub download_url: Option<String>,
    /// For `plan_installer_run` plans: the downloaded installer the command operates on
    /// (inside the app downloads dir; re-validated by `start_install`).
    #[serde(default)]
    pub installer_path: Option<String>,
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
    /// The command was killed because it exceeded the install timeout (`exit_code` is `None`).
    pub timed_out: bool,
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

/// An installer resolved for this machine (`fetch_installer_release`): CC Switch (GitHub /
/// intranet), Node.js LTS (chosen dist mirror) or the Codex desktop client (static URLs).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallerRelease {
    pub target: InstallTarget,
    pub version: String,
    pub asset_name: String,
    pub download_url: String,
    pub sha256: Option<String>,
    /// Where it came from: `github`, `intranet`, a Node mirror id (`official` / `npmmirror`)
    /// or `static`.
    pub source: String,
    /// Whether running the downloaded installer will ask for administrator rights.
    #[serde(default)]
    pub requires_admin: bool,
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
    /// Codex only: the step belongs to one of the two account paths (`None` = shown on both).
    #[serde(default)]
    pub branch: Option<GuideBranch>,
}

/// How the Codex user authenticates in CC Switch: with a ChatGPT account (CC Switch's
/// "OpenAI Official" preset + `codex` login) or with the gateway API key (custom provider).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuideBranch {
    ChatgptLogin,
    ApiKey,
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

/// Combined result of the in-place connectivity test on the configure screen: URL rules and
/// key format always run; the two live requests only when both allow sending the key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectivityReport {
    pub url: UrlPreview,
    pub key: KeyValidation,
    /// `GET {base}/models` — the cheap half of the test: it answers in about a second and proves
    /// address + key on its own, even when the protocol probe is slow or the model name is wrong.
    /// `None` when nothing was sent (invalid URL or a key with blocking format issues).
    pub models: Option<ModelList>,
    /// The protocol probe (`POST` per [`Protocol`]) — the half that also proves the model works.
    /// `None` under the same conditions as `models`.
    pub gateway: Option<GatewayCheck>,
}

/// Model ids offered by the gateway (`GET {base}/models`), plus the probe outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelList {
    pub gateway: GatewayCheck,
    pub models: Vec<String>,
}

/// One-click provider hand-off to CC Switch via its `ccswitch://v1/import` deep link
/// (ADR-0006). CC Switch shows its own confirmation dialog and writes its own data — this app
/// still never touches `~/.cc-switch` (ADR-0003).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CcSwitchImportRequest {
    pub tool: ToolId,
    pub provider_name: String,
    pub base_url: String,
    /// Required by CC Switch for a provider import. In memory only; embedded in the deep link
    /// only after the user confirmed the (masked) preview.
    pub api_key: String,
    #[serde(default)]
    pub model: String,
}

/// What the UI shows before the deep link is opened ("show before run", hard rule 4).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CcSwitchImportPreview {
    /// The deep link with the API key masked — safe to display.
    pub display_url: String,
    /// CC Switch app type the provider lands in (`codex` / `claude`).
    pub app: String,
}

/// Scope of Codex's `model_auto_compact_token_limit`: what the auto-compact threshold counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoCompactScope {
    /// Only the conversation body after the cached prefix (recommended default).
    BodyAfterPrefix,
    /// The whole request, prefix included — compaction triggers earlier.
    Total,
}

/// Inputs of the Codex `config.toml` template (`guide::codex_config_template`). Deliberately
/// carries **no API key**: the rendered template contains a placeholder the UI substitutes at
/// copy time, so the key never crosses IPC for template generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexConfigRequest {
    pub provider_name: String,
    pub base_url: String,
    pub model: String,
    pub reasoning_effort: String,
    pub auto_compact_scope: AutoCompactScope,
}

// ---------------------------------------------------------------------------
// Optional Codex Fast UI toolkit — Windows only (ADR-0007)
// ---------------------------------------------------------------------------

/// What the optional toolkit should do to the independent patched copy of the Codex client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FastUiAction {
    /// Build (or rebuild) the patched copy from the officially installed Codex client.
    Install,
    /// Re-check that an existing copy still carries exactly the expected patch.
    Verify,
    /// Put the official `app.asar` back into the copy.
    Restore,
}

/// Availability of the optional toolkit on this machine (`fast_ui::status`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FastUiStatus {
    /// Windows only; `false` everywhere else, and then nothing below matters.
    pub supported: bool,
    pub toolkit_version: String,
    /// Codex build the toolkit was tested against; shown as a caveat.
    pub tested_codex_build: String,
    /// The bundled archive is present and matches its pinned SHA-256.
    pub toolkit_available: bool,
    pub codex_app_found: bool,
    /// Install location of the official client (empty when not found).
    pub codex_app_path: String,
    pub installed: bool,
    pub install_root: String,
    pub shortcut_path: String,
    /// i18n code explaining why installing cannot run now; `None` when it can.
    pub blocked_code: Option<String>,
}

/// Show-before-run plan for one toolkit action (hard rule 4).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FastUiPlan {
    pub action: FastUiAction,
    /// The exact command; `fast_ui::start` refuses anything that differs from it.
    pub display_command: String,
    pub install_root: String,
    pub shortcut_path: String,
    pub toolkit_version: String,
    pub toolkit_sha256: String,
    /// `true` when an existing copy is replaced (the toolkit backs the previous one up).
    pub reinstall: bool,
}

/// Handle of a running toolkit job; output arrives on `fastui://output`, the outcome on
/// `fastui://done` (both carry the `InstallOutputEvent` / `InstallDoneEvent` shapes).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FastUiJob {
    pub job_id: String,
    pub action: FastUiAction,
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
    /// The gateway base URL is not `https` — the probe refused to send the key in clear text.
    NotHttps,
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
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
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

// ---------------------------------------------------------------------------
// One-click remediation (ADR-0008): PATH repair, env-var cleanup, Codex config.toml apply
// ---------------------------------------------------------------------------

/// What `apply_path_repair` will do. Shown to the user before anything runs; `apply` re-derives
/// the plan for the same `dir` and refuses to run when `display_command` differs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathRepairPlan {
    pub dir: String,
    pub platform: Platform,
    /// Where the persistent PATH lives: `HKCU\Environment\Path` or a shell rc file (`~/.zshrc`).
    pub location: String,
    /// Human-readable rendering of the change (registry edit or the exact line appended).
    pub display_command: String,
    /// `dir` is already part of the persistent PATH (apply is a no-op).
    pub already_present: bool,
    /// A backup copy is written before editing a file (rc files only).
    pub creates_backup: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathRepairResult {
    pub changed: bool,
    pub location: String,
    pub backup_path: Option<String>,
}

/// How one env-var source will be removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvCleanupAction {
    /// Delete the value under `HKCU\Environment` (no elevation).
    DeleteUserRegistry,
    /// Delete the value under `HKLM\…\Environment` through an elevated `reg delete` (UAC).
    DeleteMachineRegistry,
    /// Comment the defining line out in the shell start-up file (backup first).
    CommentOutRcLine,
    /// `launchctl unsetenv NAME`.
    LaunchctlUnsetenv,
    /// Only set in this process (inherited from the launcher) — nothing persistent to remove.
    None,
}

/// One variable at one source, with its **full** value (user-requested preview; never logged).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvCleanupItem {
    pub name: String,
    pub source: EnvVarSource,
    /// Full current value when the source exposes one (`None` for rc-file hits — the line is
    /// shown instead — and for sources without a readable value).
    pub value: Option<String>,
    /// The rc-file line that will be commented out (rc sources only).
    pub line: Option<String>,
    pub action: EnvCleanupAction,
    pub display_command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvCleanupPlan {
    pub platform: Platform,
    pub items: Vec<EnvCleanupItem>,
    /// At least one item needs elevation (machine registry).
    pub requires_admin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvCleanupResult {
    /// `name ← location` of every source actually removed.
    pub removed: Vec<String>,
    /// `name ← location` of sources that could not be removed (with the reason appended).
    pub failed: Vec<String>,
    pub backups: Vec<String>,
}

/// Text the user confirmed for `~/.codex/config.toml` (may contain the real key; memory only).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexConfigApplyRequest {
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexConfigStatus {
    pub path: String,
    pub exists: bool,
    /// Current file content with secrets redacted (for the preview), `None` when absent.
    pub current_redacted: Option<String>,
    /// Backups written by this app (`config.toml.seedrouter-<timestamp>.bak`), newest first.
    pub backups: Vec<String>,
    /// `Some(true)` when the live file equals the supplied template (ignoring trailing
    /// whitespace), `Some(false)` when it differs, `None` when no template was supplied or the
    /// file is absent.
    pub matches_template: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexConfigApplyResult {
    pub path: String,
    pub backup_path: Option<String>,
    pub bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `#[serde(rename_all = …)]` on an enum renames the **variants**, not the fields inside
    /// them, so every internally tagged DTO needs `rename_all_fields` too. Without it
    /// `label_code` reached the webview under its Rust name while `src/lib/types.ts` reads
    /// `labelCode`, and every `open_url` fix button rendered the raw key
    /// `fixes.labels.undefined`; in the other direction `diagnose` rejected the whole request
    /// as soon as the UI reported a `command_failed` symptom.
    #[test]
    fn tagged_enum_fields_are_camel_case_like_the_typescript_mirror() {
        let fix = serde_json::to_value(FixAction::OpenUrl {
            url: "https://example.test/download".into(),
            label_code: "node_download".into(),
        })
        .expect("serialize");
        assert_eq!(fix["kind"], "open_url");
        assert_eq!(fix["labelCode"], "node_download");
        assert!(fix.get("label_code").is_none(), "{fix}");

        let sent_by_the_webview = serde_json::json!({
            "kind": "command_failed",
            "tool": "codex",
            "outputTail": "boom",
        });
        match serde_json::from_value::<Symptom>(sent_by_the_webview).expect("deserialize") {
            Symptom::CommandFailed { tool, output_tail } => {
                assert_eq!(tool, ToolId::Codex);
                assert_eq!(output_tail, "boom");
            }
            other => panic!("unexpected symptom: {other:?}"),
        }
    }
}
