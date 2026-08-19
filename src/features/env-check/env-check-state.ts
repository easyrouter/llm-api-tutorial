/**
 * Pure state + helpers for the Environment Check screen (M1). No React, no IPC — everything
 * here is unit-tested in `env-check-state.test.ts`.
 *
 * Flow: `start_all` marks every row as running → each `result` (streamed on
 * `checks://progress`, or returned by a single re-run) fills its row → `all_done` carries the
 * final `EnvSnapshot` (authoritative; it overwrites streamed rows). Rows for tools the user
 * did not select are hidden (`visibleCheckIds`) and excluded from the summary and the Next gate.
 */
import type { BadgeStatus } from "@/components/ui";
import type {
  CheckId,
  CheckResult,
  CheckStatus,
  EnvSnapshot,
  Platform,
  ToolId,
  WireError,
} from "@/lib/types";
import { CHECK_IDS } from "@/lib/types";

/** Which check row belongs to which selectable tool. */
export const TOOL_CHECK_IDS: Readonly<Record<ToolId, CheckId>> = {
  codex: "codex",
  "claude-code": "claude_code",
};

/**
 * All check ids, minus the rows of tools the user did not select on the Welcome screen and
 * minus platform-specific rows that do not apply (`windows_terminal` off Windows). A `null`
 * platform (app info not loaded yet) keeps platform rows visible — Rust reports them as
 * `Skipped`, which the summary ignores.
 */
export function visibleCheckIds(
  selectedTools: readonly ToolId[],
  platform: Platform | null = null,
): CheckId[] {
  const hidden = new Set<CheckId>();
  for (const [tool, id] of Object.entries(TOOL_CHECK_IDS) as [ToolId, CheckId][]) {
    if (!selectedTools.includes(tool)) hidden.add(id);
  }
  if (platform !== null && platform !== "windows") hidden.add("windows_terminal");
  return CHECK_IDS.filter((id) => !hidden.has(id));
}

export type EnvCheckPhase = "idle" | "running" | "done" | "error";

export interface EnvCheckState {
  phase: EnvCheckPhase;
  results: Partial<Record<CheckId, CheckResult>>;
  /** Rows currently being re-run individually. */
  rerunning: readonly CheckId[];
  /** Per-row error of the last individual re-run. */
  rowErrors: Partial<Record<CheckId, WireError>>;
  /** Error of the last full run. */
  error: WireError | null;
  /** "Continue anyway" checkbox. */
  acknowledged: boolean;
}

export type EnvCheckAction =
  | { type: "start_all" }
  | { type: "result"; result: CheckResult }
  | { type: "all_done"; snapshot: EnvSnapshot }
  | { type: "all_failed"; error: WireError }
  | { type: "rerun_start"; id: CheckId }
  | { type: "rerun_done"; result: CheckResult }
  | { type: "rerun_failed"; id: CheckId; error: WireError }
  | { type: "acknowledge"; value: boolean };

export const initialEnvCheckState: EnvCheckState = {
  phase: "idle",
  results: {},
  rerunning: [],
  rowErrors: {},
  error: null,
  acknowledged: false,
};

function resultsById(checks: readonly CheckResult[]): Partial<Record<CheckId, CheckResult>> {
  const out: Partial<Record<CheckId, CheckResult>> = {};
  for (const c of checks) out[c.id] = c;
  return out;
}

function without<T>(list: readonly T[], item: T): T[] {
  return list.filter((x) => x !== item);
}

function withoutKey<K extends string, V>(
  record: Partial<Record<K, V>>,
  key: K,
): Partial<Record<K, V>> {
  if (!(key in record)) return record;
  const next = { ...record };
  delete next[key];
  return next;
}

export function envCheckReducer(state: EnvCheckState, action: EnvCheckAction): EnvCheckState {
  switch (action.type) {
    case "start_all":
      return {
        ...state,
        phase: "running",
        results: {},
        rerunning: [],
        rowErrors: {},
        error: null,
        acknowledged: false,
      };
    case "result":
      return { ...state, results: { ...state.results, [action.result.id]: action.result } };
    case "all_done":
      return {
        ...state,
        phase: "done",
        results: resultsById(action.snapshot.checks),
        error: null,
      };
    case "all_failed":
      return { ...state, phase: "error", error: action.error };
    case "rerun_start":
      return {
        ...state,
        rerunning: state.rerunning.includes(action.id)
          ? state.rerunning
          : [...state.rerunning, action.id],
        rowErrors: withoutKey(state.rowErrors, action.id),
      };
    case "rerun_done":
      return {
        ...state,
        rerunning: without(state.rerunning, action.result.id),
        results: { ...state.results, [action.result.id]: action.result },
      };
    case "rerun_failed":
      return {
        ...state,
        rerunning: without(state.rerunning, action.id),
        rowErrors: { ...state.rowErrors, [action.id]: action.error },
      };
    case "acknowledge":
      return { ...state, acknowledged: action.value };
  }
}

/** Badge status of one row: running while (re)checking, pending before the first run. */
export function rowStatus(state: EnvCheckState, id: CheckId): BadgeStatus {
  if (state.rerunning.includes(id)) return "running";
  const result = state.results[id];
  if (result) return result.status;
  return state.phase === "running" ? "running" : "pending";
}

/** Results of the visible rows only, in display order. */
export function visibleResults(state: EnvCheckState, ids: readonly CheckId[]): CheckResult[] {
  const out: CheckResult[] = [];
  for (const id of ids) {
    const r = state.results[id];
    if (r) out.push(r);
  }
  return out;
}

/** Worst status wins (fail > warn > pass); `skipped` is ignored — mirrors Rust `overall_status`. */
export function overallOf(results: readonly CheckResult[]): CheckStatus {
  const rank: Record<CheckStatus, number> = { skipped: 0, pass: 1, warn: 2, fail: 3 };
  let worst: CheckStatus = "skipped";
  for (const r of results) {
    if (r.status !== "skipped" && rank[r.status] > rank[worst]) worst = r.status;
  }
  return worst;
}

export interface CheckSummary {
  total: number;
  done: number;
  pass: number;
  warn: number;
  fail: number;
  skipped: number;
  overall: CheckStatus;
}

/** Counts per status over the visible rows. `total` counts rows, `done` rows with a result. */
export function summarize(state: EnvCheckState, ids: readonly CheckId[]): CheckSummary {
  const results = visibleResults(state, ids);
  const summary: CheckSummary = {
    total: ids.length,
    done: results.length,
    pass: 0,
    warn: 0,
    fail: 0,
    skipped: 0,
    overall: overallOf(results),
  };
  for (const r of results) summary[r.status] += 1;
  return summary;
}

/** A failure the Install step can resolve (an install or a download-page fix is attached). */
export function isInstallFixable(result: CheckResult): boolean {
  return result.fixes.some((f) => f.kind === "install" || f.kind === "open_url");
}

/**
 * A failure with a one-click fix right in the row (`repair_path`, `clean_env_vars`): the user
 * can resolve it here without leaving the screen (ADR-0008).
 */
export function isFixableInPlace(result: CheckResult): boolean {
  return result.fixes.some((f) => f.kind === "repair_path" || f.kind === "clean_env_vars");
}

/** Install-fixable or fixable in place — either way the wizard knows how to resolve it. */
export function isResolvable(result: CheckResult): boolean {
  return isInstallFixable(result) || isFixableInPlace(result);
}

/**
 * True when there is at least one failing row and every failing row is resolvable (by the
 * Install step or by a one-click fix on this screen).
 */
export function failuresAreInstallFixable(results: readonly CheckResult[]): boolean {
  const failing = results.filter((r) => r.status === "fail");
  return failing.length > 0 && failing.every(isResolvable);
}

export interface NextGate {
  /** Next button enabled. */
  canProceed: boolean;
  /** Show the "continue anyway" checkbox. */
  offerContinueAnyway: boolean;
}

/**
 * Next is enabled once the run finished and either nothing is blocked or the user explicitly
 * acknowledged blockers that the Install step or a one-click fix can resolve. Warn-level rows
 * (e.g. `codex_app.missing`, `env_vars.conflicts`) never block.
 */
export function nextGate(state: EnvCheckState, ids: readonly CheckId[]): NextGate {
  if (state.phase !== "done" || state.rerunning.length > 0) {
    return { canProceed: false, offerContinueAnyway: false };
  }
  const results = visibleResults(state, ids);
  const overall = overallOf(results);
  if (overall !== "fail") return { canProceed: true, offerContinueAnyway: false };
  const fixable = failuresAreInstallFixable(results);
  return { canProceed: fixable && state.acknowledged, offerContinueAnyway: fixable };
}
