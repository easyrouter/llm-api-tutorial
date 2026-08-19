/**
 * IPC data types — a 1:1 mirror of `src-tauri/src/models.rs`.
 * Rust serialises with camelCase fields / snake_case (or kebab-case where noted) enum values.
 * KEEP IN SYNC with the Rust side; this file is the contract the UI codes against.
 */

export type Params = Record<string, string>;

// ---------------------------------------------------------------------------
// App / config
// ---------------------------------------------------------------------------

export type Platform = "windows" | "macos" | "linux" | "unknown";
export type ConfigSource = "bundled" | "bundled_with_override" | "fallback";
/** OpenAI shapes (Codex) plus the Anthropic Messages shape Claude Code speaks. */
export type Protocol = "responses" | "chat_completions" | "anthropic_messages";
/** kebab-case on the wire */
export type ToolId = "codex" | "claude-code";

export interface AppInfo {
  name: string;
  version: string;
  platform: Platform;
  arch: string;
  localeHint: string;
  configSource: ConfigSource;
  logDir: string;
}

export interface AppConfig {
  schemaVersion: number;
  company: { name: string; supportContact: string };
  gateway: GatewayPreset;
  tools: ToolSpec[];
  ccSwitch: CcSwitchSpec;
  codexApp: CodexAppSpec;
  requirements: Requirements;
  mirrors: Mirrors;
  envVarsToInspect: string[];
  docs: DocsConfig;
  telemetry: TelemetryConfig;
}

export interface GatewayPreset {
  baseUrl: string;
  protocol: Protocol;
  presetProviderName: string;
  defaultModel: string;
  defaultReasoningEffort: string;
}

export interface ToolSpec {
  id: ToolId;
  displayName: string;
  npmPackage: string;
  binary: string;
  versionArgs: string[];
  configDir: string;
}

export interface CcSwitchSpec {
  githubRepo: string;
  releasesApi: string;
  downloadPage: string;
  intranetMirror: string;
  dataDir: string;
}

/** Distribution points of the Codex desktop client (`app-config.json` → `codexApp`). */
export interface CodexAppSpec {
  storeProductId: string;
  windowsMsixX64: string;
  windowsMsixArm64: string;
  macosDmg: string;
  downloadPage: string;
}

export interface Requirements {
  nodeMinVersion: string;
  nodeRecommendedLts: string;
  os: {
    windows: { minBuild: number; label: string };
    macos: { minVersion: string; label: string };
  };
}

export interface MirrorEntry {
  id: string;
  url: string;
  downloadPage: string;
}

export interface Mirrors {
  npmRegistries: MirrorEntry[];
  nodeDist: MirrorEntry[];
  probeTimeoutMs: number;
}

export interface DocsConfig {
  baseUrl: string;
  indexPath: string;
  cacheTtlSeconds: number;
}

export interface TelemetryConfig {
  enabled: boolean;
  endpoint: string;
  flushIntervalSeconds: number;
}

// ---------------------------------------------------------------------------
// M1 — checks
// ---------------------------------------------------------------------------

export type CheckId =
  | "os"
  | "windows_terminal"
  | "node"
  | "npm"
  | "codex"
  | "claude_code"
  | "cc_switch"
  | "codex_app"
  | "env_vars"
  | "network_npm"
  | "network_gateway"
  | "network_github";

export const CHECK_IDS: readonly CheckId[] = [
  "os",
  "windows_terminal",
  "node",
  "npm",
  "codex",
  "claude_code",
  "cc_switch",
  "codex_app",
  "env_vars",
  "network_npm",
  "network_gateway",
  "network_github",
] as const;

export type CheckStatus = "pass" | "warn" | "fail" | "skipped";

export type WizardStep =
  "welcome" | "env_check" | "install" | "configure" | "verify" | "diagnose" | "done";

/** kebab-case on the wire */
export type InstallTarget = "node" | "codex" | "claude-code" | "cc-switch" | "codex-app";

/** Well-known OS URIs the app may open (`openSystemUri`); mapped to the real URI in Rust. */
export type SystemUri = "ms_store_codex_app" | "windows_region_settings";

export type FixAction =
  | { kind: "open_url"; url: string; labelCode: string }
  | { kind: "go_to_step"; step: WizardStep }
  | { kind: "install"; tool: InstallTarget }
  | { kind: "instructions"; code: string; params: Params }
  /** One-click: add `dir` to the user's persistent PATH (plan → confirm → apply). */
  | { kind: "repair_path"; dir: string }
  /** One-click: remove the listed env vars from their persistent sources (plan shows full values). */
  | { kind: "clean_env_vars"; names: string[] }
  | { kind: "open_system_uri"; uri: SystemUri }
  | { kind: "rerun" };

export interface CheckResult {
  id: CheckId;
  status: CheckStatus;
  /** i18n code → `checks.<code>` */
  code: string;
  params: Params;
  details: string[];
  fixes: FixAction[];
  durationMs: number;
}

export interface OsInfo {
  platform: Platform;
  version: string;
  build: number | null;
  arch: string;
  shell: string;
  homeDir: string;
  isAdmin: boolean | null;
}

export interface ToolInfo {
  id: ToolId;
  installed: boolean;
  version: string | null;
  path: string | null;
  onPath: boolean;
  npmGlobalBin: string | null;
}

export type EnvVarSourceKind =
  "process" | "user_registry" | "machine_registry" | "shell_rc" | "launchctl";

export interface EnvVarSource {
  kind: EnvVarSourceKind;
  location: string;
}

export interface EnvVarFinding {
  name: string;
  presentInSession: boolean;
  valueMasked: string | null;
  sources: EnvVarSource[];
}

export interface ProbeResult {
  id: string;
  url: string;
  reachable: boolean;
  latencyMs: number | null;
  httpStatus: number | null;
  error: string | null;
}

export interface MirrorChoice {
  npmRegistry: MirrorEntry;
  nodeDist: MirrorEntry;
  probes: ProbeResult[];
}

export interface EnvSnapshot {
  os: OsInfo;
  tools: ToolInfo[];
  envVars: EnvVarFinding[];
  checks: CheckResult[];
  overall: CheckStatus;
  generatedAt: string;
}

// ---------------------------------------------------------------------------
// M2 — install
// ---------------------------------------------------------------------------

export interface InstallPlan {
  target: InstallTarget;
  program: string;
  args: string[];
  env: Record<string, string>;
  displayCommand: string;
  registry: MirrorEntry | null;
  requiresAdmin: boolean;
  /** i18n code → `install.plan.<code>` */
  explanationCode: string;
  /**
   * Page / file the user downloads manually when the plan has no command (Node.js download
   * page of the chosen mirror). `null` for command plans and for installer targets (fetch the
   * release with `fetchInstallerRelease` instead).
   */
  downloadUrl: string | null;
  /** `planInstallerRun` plans: the downloaded installer the command runs (re-validated by Rust). */
  installerPath: string | null;
}

export interface InstallJob {
  jobId: string;
  target: InstallTarget;
}

export type OutputStream = "stdout" | "stderr" | "system";

export interface InstallOutputEvent {
  jobId: string;
  stream: OutputStream;
  line: string;
}

export interface InstallDoneEvent {
  jobId: string;
  success: boolean;
  exitCode: number | null;
  durationMs: number;
  cancelled: boolean;
  /** The command was killed because it exceeded the install timeout (`exitCode` is null). */
  timedOut: boolean;
}

export interface DownloadProgressEvent {
  jobId: string;
  downloaded: number;
  total: number | null;
}

export interface DownloadRequest {
  jobId: string;
  url: string;
  fileName: string;
  expectedSha256: string | null;
}

export interface DownloadResult {
  path: string;
  bytes: number;
  sha256: string;
  verified: boolean | null;
}

/**
 * An installer resolved for this machine: CC Switch (GitHub / intranet), Node.js LTS (chosen
 * dist mirror — `source` is the mirror id) or the Codex desktop client (`source: "static"`).
 */
export interface InstallerRelease {
  target: InstallTarget;
  version: string;
  assetName: string;
  downloadUrl: string;
  sha256: string | null;
  source: string;
  /** Running the downloaded installer will ask for administrator rights (UAC / password). */
  requiresAdmin: boolean;
}

// ---------------------------------------------------------------------------
// One-click remediation (ADR-0008)
// ---------------------------------------------------------------------------

export interface PathRepairPlan {
  dir: string;
  platform: Platform;
  /** `HKCU\Environment\Path` or the shell rc file (`~/.zshrc`). */
  location: string;
  displayCommand: string;
  alreadyPresent: boolean;
  createsBackup: boolean;
}

export interface PathRepairResult {
  changed: boolean;
  location: string;
  backupPath: string | null;
}

export type EnvCleanupAction =
  | "delete_user_registry"
  | "delete_machine_registry"
  | "comment_out_rc_line"
  | "launchctl_unsetenv"
  | "none";

export interface EnvCleanupItem {
  name: string;
  source: EnvVarSource;
  /** Full current value (user-requested preview; never logged). `null` for rc-file hits. */
  value: string | null;
  /** The rc-file line that will be commented out. */
  line: string | null;
  action: EnvCleanupAction;
  displayCommand: string;
}

export interface EnvCleanupPlan {
  platform: Platform;
  items: EnvCleanupItem[];
  /** At least one item needs elevation (machine registry → UAC prompt). */
  requiresAdmin: boolean;
}

export interface EnvCleanupResult {
  removed: string[];
  failed: string[];
  backups: string[];
}

export interface CodexConfigApplyRequest {
  /** Full config.toml text to write (may contain the real key — memory only). */
  content: string;
}

export interface CodexConfigStatus {
  path: string;
  exists: boolean;
  currentRedacted: string | null;
  /** Backups written by this app, newest first. */
  backups: string[];
  matchesTemplate: boolean | null;
}

export interface CodexConfigApplyResult {
  path: string;
  backupPath: string | null;
  bytes: number;
}

// ---------------------------------------------------------------------------
// M3 — guide
// ---------------------------------------------------------------------------

export interface ProviderPreset {
  providerName: string;
  baseUrl: string;
  protocol: Protocol;
  modelHint: string;
  reasoningEffortHint: string;
}

/** Codex account paths: CC Switch "OpenAI Official" preset + ChatGPT login vs. custom provider with the gateway key. */
export type GuideBranch = "chatgpt_login" | "api_key";

export interface GuideStep {
  id: string;
  /** i18n code → `guide.<code>.title` / `.body` */
  code: string;
  params: Params;
  copyValue: string | null;
  verifyCheck: CheckId | null;
  /** Codex only: shown for this account path only (`null` = both). */
  branch: GuideBranch | null;
}

export interface ConfigGuide {
  tool: ToolId;
  preset: ProviderPreset;
  steps: GuideStep[];
}

export type KeyIssue =
  | "empty"
  | "leading_or_trailing_whitespace"
  | "contains_whitespace"
  | "contains_newline"
  | "non_ascii"
  | "unexpected_prefix"
  | "too_short"
  | "looks_like_placeholder";

export interface KeyValidation {
  valid: boolean;
  issues: KeyIssue[];
  length: number;
}

export type UrlRule =
  "already_versioned" | "appended_v1" | "literal_hash" | "custom_path" | "invalid";

export type UrlWarning =
  | "not_https"
  | "trailing_slash_removed"
  | "contains_whitespace"
  | "contains_credentials"
  | "looks_like_chat_completions_endpoint"
  | "differs_from_company_gateway";

export interface UrlPreview {
  input: string;
  effectiveUrl: string;
  rule: UrlRule;
  warnings: UrlWarning[];
}

/**
 * Combined result of the in-place connectivity test (configure screen): URL rules and key
 * format always run; `gateway` is `null` when nothing was sent (invalid URL / blocked key).
 */
export interface ConnectivityReport {
  url: UrlPreview;
  key: KeyValidation;
  gateway: GatewayCheck | null;
}

/** Model ids offered by the gateway (`GET {base}/models`), plus the probe outcome. */
export interface ModelList {
  gateway: GatewayCheck;
  models: string[];
}

/** One-click provider hand-off to CC Switch via its `ccswitch://v1/import` deep link. */
export interface CcSwitchImportRequest {
  tool: ToolId;
  providerName: string;
  baseUrl: string;
  apiKey: string;
  model: string;
}

/** Shown before the deep link is opened; `displayUrl` has the API key masked. */
export interface CcSwitchImportPreview {
  displayUrl: string;
  app: string;
}

/** Scope of Codex's `model_auto_compact_token_limit` (what the threshold counts). */
export type AutoCompactScope = "body_after_prefix" | "total";

/**
 * Inputs of the Codex `config.toml` template. Deliberately carries no API key: the rendered
 * template contains a placeholder the UI substitutes at copy time.
 */
export interface CodexConfigRequest {
  providerName: string;
  baseUrl: string;
  model: string;
  reasoningEffort: string;
  autoCompactScope: AutoCompactScope;
}

// ---------------------------------------------------------------------------
// Optional Codex Fast UI toolkit — Windows only (ADR-0007)
// ---------------------------------------------------------------------------

/** What the optional toolkit does to the independent patched copy of the Codex client. */
export type FastUiAction = "install" | "verify" | "restore";

/** Availability of the optional toolkit on this machine. */
export interface FastUiStatus {
  /** Windows only; when false nothing below matters. */
  supported: boolean;
  toolkitVersion: string;
  /** Codex build the toolkit was tested against; shown as a caveat. */
  testedCodexBuild: string;
  /** The bundled archive is present and matches its pinned SHA-256. */
  toolkitAvailable: boolean;
  codexAppFound: boolean;
  codexAppPath: string;
  installed: boolean;
  installRoot: string;
  shortcutPath: string;
  /** Fully qualified i18n code (`guide:fastui.blocked.*`), null when installing can run. */
  blockedCode: string | null;
}

/** Show-before-run plan for one toolkit action. */
export interface FastUiPlan {
  action: FastUiAction;
  displayCommand: string;
  installRoot: string;
  shortcutPath: string;
  toolkitVersion: string;
  toolkitSha256: string;
  /** An existing copy will be replaced (the toolkit backs the previous one up). */
  reinstall: boolean;
}

/** Handle of a running toolkit job; output arrives on `fastui://output`. */
export interface FastUiJob {
  jobId: string;
  action: FastUiAction;
}

// ---------------------------------------------------------------------------
// M4 — verify
// ---------------------------------------------------------------------------

export interface GatewayProbeRequest {
  baseUrl: string;
  apiKey: string;
  model: string;
  protocol: Protocol;
}

export interface VerifyRequest {
  tool: ToolId;
  gateway: GatewayProbeRequest | null;
}

export type ErrorClass =
  | "auth"
  | "not_found"
  | "protocol_mismatch"
  | "rate_limited"
  | "server_error"
  | "network"
  | "timeout"
  | "tls"
  | "not_https"
  | "command_not_found"
  | "command_failed"
  | "unknown";

export interface CliCheck {
  ok: boolean;
  version: string | null;
  path: string | null;
  outputTail: string | null;
  errorClass: ErrorClass | null;
}

export interface GatewayCheck {
  ok: boolean;
  httpStatus: number | null;
  latencyMs: number | null;
  errorClass: ErrorClass | null;
  message: string | null;
}

export interface TerminalProcess {
  pid: number;
  name: string;
}

export interface VerifyResult {
  tool: ToolId;
  cli: CliCheck;
  gateway: GatewayCheck | null;
  runningTerminals: TerminalProcess[];
  ok: boolean;
  diagnoses: Diagnosis[];
}

// ---------------------------------------------------------------------------
// M5 — diagnose
// ---------------------------------------------------------------------------

export type Symptom =
  | { kind: "http_status"; status: number; tool: ToolId }
  | { kind: "command_not_found"; tool: ToolId }
  | { kind: "command_failed"; tool: ToolId; outputTail: string }
  | { kind: "no_effect_after_config"; tool: ToolId }
  | { kind: "network_error"; target: string }
  | { kind: "timeout"; target: string }
  | { kind: "app_blocked_by_os"; app: string }
  | { kind: "env_var_conflict"; name: string }
  | { kind: "protocol_mismatch"; tool: ToolId };

export type Severity = "info" | "warning" | "blocking";

export interface Diagnosis {
  ruleId: string;
  severity: Severity;
  /** i18n code → `diagnose.<code>.title` / `.explanation` / `.steps.<n>` */
  code: string;
  params: Params;
  actions: FixAction[];
  checklist: string[];
}

export interface DiagnoseRequest {
  symptoms: Symptom[];
  snapshot: EnvSnapshot | null;
}

export interface DiagnosticReport {
  generatedAt: string;
  appVersion: string;
  markdown: string;
}

// ---------------------------------------------------------------------------
// M6 — docs
// ---------------------------------------------------------------------------

export type DocsSource = "remote" | "cache" | "bundled";

export interface DocSection {
  id: string;
  title: string;
  path: string;
  lang: string;
  wizardStep: WizardStep | null;
  children: DocSection[];
}

export interface DocsIndex {
  sections: DocSection[];
  fetchedAt: string;
  source: DocsSource;
}

export interface DocPage {
  id: string;
  title: string;
  lang: string;
  markdown: string;
  source: DocsSource;
  fetchedAt: string;
}

// ---------------------------------------------------------------------------
// telemetry
// ---------------------------------------------------------------------------

export interface TelemetryEvent {
  name: string;
  step: WizardStep | null;
  status: string | null;
  durationMs: number | null;
  errorClass: ErrorClass | null;
  ruleId: string | null;
}

export interface TelemetryStatus {
  enabled: boolean;
  configured: boolean;
  queued: number;
  sessionId: string;
}

// ---------------------------------------------------------------------------
// errors (see src-tauri/src/error.rs)
// ---------------------------------------------------------------------------

export interface WireError {
  code: string;
  message: string;
  params: Params;
}

export function isWireError(e: unknown): e is WireError {
  return (
    typeof e === "object" &&
    e !== null &&
    typeof (e as WireError).code === "string" &&
    typeof (e as WireError).message === "string"
  );
}
