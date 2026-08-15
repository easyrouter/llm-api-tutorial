/**
 * Helpers around `EnvSnapshot` / `CheckResult` shared by the Environment and Install screens.
 */
import type { Translate } from "@/lib/errors";
import type { CheckResult, EnvSnapshot, Params } from "@/lib/types";

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

/** Params whose value is a tool id are shown with the tool's display name. */
const TOOL_NAME_PARAMS = new Set(["tool"]);

/**
 * Localised one-line message for a check result: `checks:<code>` with the Rust params. The
 * `tool` param is mapped through `common:tools.<id>` when it is a known id, and an unknown code
 * falls back to a neutral sentence that still shows the code (never the raw key).
 */
export function checkMessage(t: Translate, result: Pick<CheckResult, "code" | "params">): string {
  const params: Params = { ...result.params };
  for (const key of TOOL_NAME_PARAMS) {
    const value = params[key];
    if (value) params[key] = t(`common:tools.${value}`, { defaultValue: value });
  }
  return t(`checks:${result.code}`, {
    ...params,
    defaultValue: t("checks:screen.unknownCode", { code: result.code }),
  });
}
