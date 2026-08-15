/**
 * Pure helpers for the Install screen: which targets need installing (derived from the
 * environment snapshot), how each target is installed, and where its download page lives.
 */
import type {
  AppConfig,
  CheckId,
  CheckResult,
  EnvSnapshot,
  InstallPlan,
  InstallTarget,
  ToolId,
} from "@/lib/types";

/** Display / processing order on the Install screen (Node first — the others need it). */
export const INSTALL_TARGET_ORDER: readonly InstallTarget[] = [
  "node",
  "codex",
  "claude-code",
  "cc-switch",
];

/** How a target gets installed — decides which item body / state machine is used. */
export type InstallKind = "npm" | "node" | "cc-switch";

export function installKind(target: InstallTarget): InstallKind {
  switch (target) {
    case "codex":
    case "claude-code":
      return "npm";
    case "node":
      return "node";
    case "cc-switch":
      return "cc-switch";
  }
}

/** The check that is re-run after a target was installed. */
export const RECHECK_ID: Readonly<Record<InstallTarget, CheckId>> = {
  node: "node",
  codex: "codex",
  "claude-code": "claude_code",
  "cc-switch": "cc_switch",
};

const TARGET_OF_TOOL: Readonly<Record<ToolId, InstallTarget>> = {
  codex: "codex",
  "claude-code": "claude-code",
};

/** Codes (per check id) that mean "this target is missing / unusable" — see checks/mod.rs. */
function targetsFromCode(result: CheckResult): InstallTarget[] {
  switch (result.id) {
    case "node":
      return result.code === "node.missing" || result.code === "node.too_old" ? ["node"] : [];
    case "npm":
      return result.code === "npm.missing" ? ["node"] : [];
    case "codex":
      return result.code === "tool.missing" ? ["codex"] : [];
    case "claude_code":
      return result.code === "tool.missing" ? ["claude-code"] : [];
    case "cc_switch":
      return result.code === "cc_switch.missing" || result.code === "cc_switch.data_only"
        ? ["cc-switch"]
        : [];
    default:
      return [];
  }
}

/** `install` fixes attached by Rust to a non-passing check. */
function targetsFromFixes(result: CheckResult): InstallTarget[] {
  if (result.status === "pass" || result.status === "skipped") return [];
  return result.fixes.flatMap((f) => (f.kind === "install" ? [f.tool] : []));
}

/**
 * Targets the Install screen should offer, in `INSTALL_TARGET_ORDER`:
 * - Node when the node/npm check says missing or too old,
 * - Codex / Claude Code when their check says `tool.missing` — only for selected tools,
 * - CC Switch when it is missing,
 * - whatever Rust attached as an `install` fix to a non-passing check (same tool filter),
 * - plus everything the user explicitly requested from the Environment screen.
 */
export function deriveInstallTargets(
  snapshot: EnvSnapshot | null,
  selectedTools: readonly ToolId[],
  requested: readonly InstallTarget[],
): InstallTarget[] {
  const wanted = new Set<InstallTarget>(requested);
  const selectedTargets = new Set(selectedTools.map((t) => TARGET_OF_TOOL[t]));
  const allowed = (target: InstallTarget) =>
    installKind(target) !== "npm" || selectedTargets.has(target);

  for (const check of snapshot?.checks ?? []) {
    for (const target of [...targetsFromCode(check), ...targetsFromFixes(check)]) {
      if (allowed(target)) wanted.add(target);
    }
  }
  return INSTALL_TARGET_ORDER.filter((t) => wanted.has(t));
}

/**
 * Download page for a Node.js plan: the URL Rust put on the plan (`plan.downloadUrl`, the
 * chosen Node dist mirror's page), otherwise the plan's registry page, otherwise the first
 * configured Node distribution mirror, otherwise nodejs.org.
 */
export function nodeDownloadPage(plan: InstallPlan | null, config: AppConfig | null): string {
  const fromPlan = plan?.downloadUrl || plan?.registry?.downloadPage;
  if (fromPlan) return fromPlan;
  const fromConfig = config?.mirrors.nodeDist.find((m) => m.downloadPage)?.downloadPage;
  return fromConfig ?? "https://nodejs.org/en/download";
}

/**
 * Registry id to leave out when re-planning after `plan` failed on it (PRD M2 "switch mirror
 * and retry"): the plan's registry when the preset configures at least one other npm registry,
 * `null` when there is nothing to switch to (plain retry).
 */
export function registryToAvoidOnRetry(
  plan: InstallPlan | null,
  config: AppConfig | null,
): string | null {
  const failed = plan?.registry?.id;
  if (!failed) return null;
  const hasAlternate = (config?.mirrors.npmRegistries ?? []).some((r) => r.id !== failed);
  return hasAlternate ? failed : null;
}

/** Release page for CC Switch (manual fallback when the release API is unreachable). */
export function ccSwitchDownloadPage(config: AppConfig | null): string {
  return config?.ccSwitch.downloadPage || "https://github.com/farion1231/cc-switch/releases/latest";
}

/** npm documentation on fixing global-install permission errors (never done for the user). */
export const NPM_PERMISSIONS_DOCS_URL =
  "https://docs.npmjs.com/resolving-eacces-permissions-errors-when-installing-packages-globally";
