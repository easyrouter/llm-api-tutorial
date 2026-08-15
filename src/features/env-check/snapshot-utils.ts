/**
 * Helpers around `EnvSnapshot` / `CheckResult` shared by the Environment and Install screens.
 */
import type { Translate } from "@/lib/errors";
import type { CheckId, CheckResult, EnvSnapshot, Params } from "@/lib/types";

import { overallOf } from "./env-check-state";

/**
 * Returns a copy of `snapshot` with `result` replacing the row of the same check id (or
 * appended when absent) and `overall` recomputed. `null` snapshots stay `null` — a single
 * re-run cannot invent the OS/tool facts a full run provides.
 */
export function applyResultToSnapshot(
  snapshot: EnvSnapshot | null,
  result: CheckResult,
): EnvSnapshot | null {
  if (!snapshot) return null;
  const found = snapshot.checks.some((c) => c.id === result.id);
  const checks = found
    ? snapshot.checks.map((c) => (c.id === result.id ? result : c))
    : [...snapshot.checks, result];
  return { ...snapshot, checks, overall: overallOf(checks) };
}

/** Tool-check rows → the tool id used by `common:tools.<id>`. */
const TOOL_OF_CHECK: Partial<Record<CheckId, string>> = {
  codex: "codex",
  claude_code: "claude-code",
};

/**
 * Localised one-line message for a check result: `checks:<code>` with the Rust params. The
 * `tool` param is mapped through `common:tools.<id>` when it is a known id (and defaulted from
 * the check id for `tool.*` codes), and an unknown code falls back to a neutral sentence that
 * still shows the code (never the raw key).
 */
export function checkMessage(
  t: Translate,
  result: Pick<CheckResult, "id" | "code" | "params">,
): string {
  const params: Params = { ...result.params };
  const tool = params.tool ?? TOOL_OF_CHECK[result.id];
  if (tool) params.tool = t(`common:tools.${tool}`, { defaultValue: tool });
  return t(`checks:${result.code}`, {
    ...params,
    defaultValue: t("checks:screen.unknownCode", { code: result.code }),
  });
}
