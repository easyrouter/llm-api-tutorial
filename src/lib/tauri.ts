/**
 * Typed wrappers around Tauri `invoke`. One function per Rust command in
 * `src-tauri/src/commands.rs`. UI code must call these — never `invoke` directly —
 * so the IPC surface stays greppable and typed.
 */
import { invoke } from "@tauri-apps/api/core";

import type {
  AppConfig,
  AppInfo,
  CcSwitchImportPreview,
  CcSwitchImportRequest,
  CheckId,
  CheckResult,
  CodexConfigApplyRequest,
  CodexConfigApplyResult,
  CodexConfigRequest,
  CodexConfigStatus,
  ConfigGuide,
  ConnectivityReport,
  DiagnoseRequest,
  Diagnosis,
  DiagnosticReport,
  DocPage,
  DocsIndex,
  DownloadRequest,
  DownloadResult,
  EnvCleanupPlan,
  EnvCleanupResult,
  EnvSnapshot,
  FastUiAction,
  FastUiJob,
  FastUiPlan,
  FastUiStatus,
  GatewayProbeRequest,
  InstallJob,
  InstallPlan,
  InstallTarget,
  InstallerRelease,
  KeyValidation,
  MirrorChoice,
  ModelList,
  PathRepairPlan,
  PathRepairResult,
  SystemUri,
  TelemetryEvent,
  TelemetryStatus,
  TerminalProcess,
  ToolId,
  UrlPreview,
  VerifyRequest,
  VerifyResult,
} from "./types";

// app / config
export const getAppInfo = () => invoke<AppInfo>("get_app_info");
export const getAppConfig = () => invoke<AppConfig>("get_app_config");
export const openExternal = (url: string) => invoke<void>("open_external", { url });
/** Opens a well-known OS URI (Microsoft Store page of the Codex app, Windows region settings). */
export const openSystemUri = (uri: SystemUri) => invoke<void>("open_system_uri", { uri });

// M1
export const runEnvChecks = () => invoke<EnvSnapshot>("run_env_checks");
export const runEnvCheck = (id: CheckId) => invoke<CheckResult>("run_env_check", { id });
export const probeMirrors = () => invoke<MirrorChoice>("probe_mirrors");

// M2
/** `excludeRegistry`: id of the npm registry a previous attempt failed on (re-plan elsewhere). */
export const planInstall = (target: InstallTarget, excludeRegistry: string | null = null) =>
  invoke<InstallPlan>("plan_install", { target, excludeRegistry });
export const startInstall = (plan: InstallPlan) => invoke<InstallJob>("start_install", { plan });
export const cancelInstall = (jobId: string) => invoke<void>("cancel_install", { jobId });
/** The installer file for `target` (cc-switch, node, codex-app); npm targets are rejected. */
export const fetchInstallerRelease = (target: InstallTarget) =>
  invoke<InstallerRelease>("fetch_installer_release", { target });
/** The command that runs a downloaded installer — show it, then pass it to `startInstall`. */
export const planInstallerRun = (target: InstallTarget, path: string) =>
  invoke<InstallPlan>("plan_installer_run", { target, path });
export const downloadFile = (request: DownloadRequest) =>
  invoke<DownloadResult>("download_file", { request });
export const openDownloadedFile = (path: string) => invoke<void>("open_downloaded_file", { path });

// One-click remediation (ADR-0008) — every apply re-derives and compares the plan in Rust.
export const planPathRepair = (dir: string) => invoke<PathRepairPlan>("plan_path_repair", { dir });
export const applyPathRepair = (plan: PathRepairPlan) =>
  invoke<PathRepairResult>("apply_path_repair", { plan });
/** Full values are returned once for the confirmation dialog; never log or persist them. */
export const planEnvCleanup = (names: string[]) =>
  invoke<EnvCleanupPlan>("plan_env_cleanup", { names });
export const applyEnvCleanup = (plan: EnvCleanupPlan) =>
  invoke<EnvCleanupResult>("apply_env_cleanup", { plan });
export const codexConfigStatus = (template: string | null = null) =>
  invoke<CodexConfigStatus>("codex_config_status", { template });
export const applyCodexConfig = (request: CodexConfigApplyRequest) =>
  invoke<CodexConfigApplyResult>("apply_codex_config", { request });
export const restoreCodexConfig = () => invoke<CodexConfigApplyResult>("restore_codex_config");

// M3
export const getConfigGuide = (tool: ToolId) => invoke<ConfigGuide>("get_config_guide", { tool });
export const validateApiKey = (key: string) => invoke<KeyValidation>("validate_api_key", { key });
export const previewEffectiveUrl = (url: string) =>
  invoke<UrlPreview>("preview_effective_url", { url });
/** URL rules + key format + (when both allow it) one live gateway probe. */
export const testConnectivity = (request: GatewayProbeRequest) =>
  invoke<ConnectivityReport>("test_connectivity", { request });
/** `GET {base}/models` with the user's key; `request.model` is ignored. */
export const listGatewayModels = (request: GatewayProbeRequest) =>
  invoke<ModelList>("list_gateway_models", { request });
/** Recommended Codex config.toml (editable template; key placeholder substituted on copy). */
export const getCodexConfigTemplate = (request: CodexConfigRequest) =>
  invoke<string>("get_codex_config_template", { request });
/** Masked `ccswitch://` import link for the confirmation dialog. */
export const previewCcSwitchImport = (request: CcSwitchImportRequest) =>
  invoke<CcSwitchImportPreview>("preview_cc_switch_import", { request });
/** Opens the real `ccswitch://` import link (call only after the user confirmed the preview). */
export const openCcSwitchImport = (request: CcSwitchImportRequest) =>
  invoke<void>("open_cc_switch_import", { request });

// Optional Codex Fast UI toolkit (Windows only — ADR-0007)
export const codexFastUiStatus = () => invoke<FastUiStatus>("codex_fast_ui_status");
/** The command to show the user; `startCodexFastUi` refuses anything that differs from it. */
export const planCodexFastUi = (action: FastUiAction) =>
  invoke<FastUiPlan>("plan_codex_fast_ui", { action });
export const startCodexFastUi = (plan: FastUiPlan) =>
  invoke<FastUiJob>("start_codex_fast_ui", { plan });

// M4
export const verifySetup = (request: VerifyRequest) =>
  invoke<VerifyResult>("verify_setup", { request });
export const listRunningTerminals = () => invoke<TerminalProcess[]>("list_running_terminals");

// M5
export const diagnose = (request: DiagnoseRequest) => invoke<Diagnosis[]>("diagnose", { request });
export const buildDiagnosticReport = (
  snapshot: EnvSnapshot | null,
  diagnoses: Diagnosis[],
  verify: VerifyResult[] = [],
) => invoke<DiagnosticReport>("build_diagnostic_report", { snapshot, diagnoses, verify });
export const saveDiagnosticReport = (path: string, markdown: string) =>
  invoke<void>("save_diagnostic_report", { path, markdown });

// M6
export const fetchDocsIndex = (lang: string) => invoke<DocsIndex>("fetch_docs_index", { lang });
export const fetchDocPage = (id: string, lang: string) =>
  invoke<DocPage>("fetch_doc_page", { id, lang });

// telemetry
export const trackEvent = (event: TelemetryEvent) => invoke<void>("track_event", { event });
export const setTelemetryEnabled = (enabled: boolean) =>
  invoke<TelemetryStatus>("set_telemetry_enabled", { enabled });
export const getTelemetryStatus = () => invoke<TelemetryStatus>("get_telemetry_status");
