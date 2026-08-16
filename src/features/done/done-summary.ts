/**
 * Pure helpers for the Done screen: turns the wizard's selection + verify results into the
 * per-tool rows the screen renders. Unit-tested in `done-summary.test.ts`.
 */
import type { AppConfig, ToolId, VerifyResult } from "@/lib/types";

/** Help section opened by the Done screen's "Open help" button. */
export const DONE_HELP_SECTION = "faq";

/** Commands used when the preset does not name a binary for a tool. */
const FALLBACK_COMMANDS: Record<ToolId, string> = { codex: "codex", "claude-code": "claude" };

export type ToolOutcome = "ok" | "failed" | "not_verified";

export interface ToolSummary {
  tool: ToolId;
  outcome: ToolOutcome;
  /** CLI version reported by verification (only meaningful when `outcome === "ok"`). */
  version: string | null;
  /** What the user types in a terminal to start the tool. */
  command: string;
}

/** The command the user types to start `tool` — the binary from the preset, else a default. */
export function commandForTool(tool: ToolId, config: AppConfig | null): string {
  const binary = config?.tools.find((spec) => spec.id === tool)?.binary.trim();
  return binary || FALLBACK_COMMANDS[tool];
}

/** Classifies one verify result: `ok`, `failed` (ran but not ok) or `not_verified` (never ran). */
export function outcomeOf(result: VerifyResult | undefined): ToolOutcome {
  if (!result) return "not_verified";
  return result.ok ? "ok" : "failed";
}

/** One row per selected tool, in selection order. */
export function summarizeTools(
  selected: readonly ToolId[],
  results: Partial<Record<ToolId, VerifyResult>>,
  config: AppConfig | null,
): ToolSummary[] {
  return selected.map((tool) => {
    const result = results[tool];
    return {
      tool,
      outcome: outcomeOf(result),
      version: result?.cli.version ?? null,
      command: commandForTool(tool, config),
    };
  });
}

/** True when at least one tool was selected and every selected tool verified ok. */
export function allVerified(summaries: readonly ToolSummary[]): boolean {
  return summaries.length > 0 && summaries.every((s) => s.outcome === "ok");
}

/** Official page of the Codex IDE extension (the Codex client shares `~/.codex` with the CLI). */
export const CODEX_IDE_EXTENSION_URL = "https://developers.openai.com/codex/ide";

/**
 * Where the Done screen sends the user next. The Codex provider configured in CC Switch covers
 * both the Codex client (IDE extension / desktop) and Codex CLI — they read the same
 * `~/.codex` configuration — so selecting `codex` yields both launch options.
 */
export type LaunchOptionId = "codex-client" | "codex-cli" | "claude-code";

/** Launch options for the selected tools, in display order. */
export function launchOptions(selected: readonly ToolId[]): LaunchOptionId[] {
  const out: LaunchOptionId[] = [];
  if (selected.includes("codex")) out.push("codex-client", "codex-cli");
  if (selected.includes("claude-code")) out.push("claude-code");
  return out;
}
