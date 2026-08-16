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
  CcSwitchRelease,
  CheckId,
  CheckResult,
  CodexConfigRequest,
  ConfigGuide,
  ConnectivityReport,
  DiagnoseRequest,
  Diagnosis,
  DiagnosticReport,
  DocPage,
  DocsIndex,
  DownloadRequest,
  DownloadResult,
  EnvSnapshot,
  GatewayProbeRequest,
  InstallJob,
  InstallPlan,
  InstallTarget,
  KeyValidation,
  MirrorChoice,
  ModelList,
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
export const fetchCcSwitchRelease = () => invoke<CcSwitchRelease>("fetch_cc_switch_release");
export const downloadFile = (request: DownloadRequest) =>
  invoke<DownloadResult>("download_file", { request });
export const openDownloadedFile = (path: string) => invoke<void>("open_downloaded_file", { path });

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
