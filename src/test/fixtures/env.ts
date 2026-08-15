/**
 * Test fixtures for M1/M2 screens: builders for `CheckResult`, `EnvSnapshot`, `InstallPlan`
 * and friends. Every builder takes a partial override so tests only spell out what matters.
 */
import type {
  CcSwitchRelease,
  CheckId,
  CheckResult,
  CheckStatus,
  DownloadResult,
  EnvSnapshot,
  EnvVarFinding,
  InstallPlan,
  InstallTarget,
  OsInfo,
} from "@/lib/types";
import { CHECK_IDS } from "@/lib/types";

export function checkResult(
  id: CheckId,
  status: CheckStatus = "pass",
  overrides: Partial<CheckResult> = {},
): CheckResult {
  return {
    id,
    status,
    code: `${id}.ok`,
    params: {},
    details: [],
    fixes: [],
    durationMs: 12,
    ...overrides,
  };
}

export const osInfo: OsInfo = {
  platform: "windows",
  version: "10.0.19045",
  build: 19045,
  arch: "x86_64",
  shell: "powershell",
  homeDir: "C:\\Users\\alice",
  isAdmin: false,
};

/** A snapshot where every check passes, unless overridden per id. */
export function envSnapshot(
  overrides: Partial<Record<CheckId, CheckResult>> = {},
  extra: Partial<EnvSnapshot> = {},
): EnvSnapshot {
  const checks = CHECK_IDS.map((id) => overrides[id] ?? checkResult(id));
  const overall: CheckStatus = checks.some((c) => c.status === "fail")
    ? "fail"
    : checks.some((c) => c.status === "warn")
      ? "warn"
      : "pass";
  return {
    os: osInfo,
    tools: [],
    envVars: [],
    checks,
    overall,
    generatedAt: "2026-01-01T00:00:00Z",
    ...extra,
  };
}

export function envVarFinding(overrides: Partial<EnvVarFinding> = {}): EnvVarFinding {
  return {
    name: "OPENAI_API_KEY",
    presentInSession: true,
    valueMasked: "sk-****abcd",
    sources: [{ kind: "user_registry", location: "HKCU\\Environment" }],
    ...overrides,
  };
}

export function installPlan(
  target: InstallTarget,
  overrides: Partial<InstallPlan> = {},
): InstallPlan {
  const pkg = target === "codex" ? "@openai/codex" : "@anthropic-ai/claude-code";
  return {
    target,
    program: "npm",
    args: ["install", "-g", pkg, "--registry", "https://registry.npmmirror.com/"],
    env: {},
    displayCommand: `npm install -g ${pkg} --registry https://registry.npmmirror.com/`,
    registry: {
      id: "npmmirror",
      url: "https://registry.npmmirror.com/",
      downloadPage: "https://npmmirror.com/mirrors/node/",
    },
    requiresAdmin: false,
    explanationCode: "npm_global",
    ...overrides,
  };
}

export function ccSwitchRelease(overrides: Partial<CcSwitchRelease> = {}): CcSwitchRelease {
  return {
    version: "3.2.0",
    assetName: "CC.Switch_3.2.0_x64-setup.exe",
    downloadUrl:
      "https://github.com/farion1231/cc-switch/releases/download/v3.2.0/CC.Switch_3.2.0_x64-setup.exe",
    sha256: "ab".repeat(32),
    source: "github",
    ...overrides,
  };
}

export function downloadResult(overrides: Partial<DownloadResult> = {}): DownloadResult {
  return {
    path: "C:\\Users\\alice\\AppData\\Local\\codex-onboarding\\cache\\CC.Switch_3.2.0_x64-setup.exe",
    bytes: 12_345_678,
    sha256: "ab".repeat(32),
    verified: true,
    ...overrides,
  };
}
