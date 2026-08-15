/**
 * Pure helpers for working with `Diagnosis` lists (M5) on the UI side: ordering, de-duplication
 * and the mapping from severity to the shared status-badge vocabulary. No I/O, no i18n.
 */
import type { BadgeStatus } from "@/components/ui";
import type { Diagnosis, Severity } from "@/lib/types";

/** Higher = more severe. Used to sort blocking findings first. */
const SEVERITY_RANK: Record<Severity, number> = { blocking: 2, warning: 1, info: 0 };

/** Stable identity of a diagnosis: the same rule with the same code is the same finding. */
export function diagnosisKey(d: Diagnosis): string {
  return `${d.ruleId}:${d.code}`;
}

/**
 * Sorts by severity (blocking → warning → info). The sort is stable, so findings of equal
 * severity keep the order the rule engine produced (which already reflects rule priority).
 */
export function sortDiagnoses(diagnoses: readonly Diagnosis[]): Diagnosis[] {
  return [...diagnoses].sort((a, b) => SEVERITY_RANK[b.severity] - SEVERITY_RANK[a.severity]);
}

/**
 * Merges two diagnosis lists (e.g. the ones attached to a `VerifyResult` and the ones from a
 * later `diagnose` call with the full snapshot), dropping duplicates by `diagnosisKey` — the
 * first occurrence wins — and returning the result sorted by severity.
 */
export function mergeDiagnoses(
  base: readonly Diagnosis[],
  extra: readonly Diagnosis[],
): Diagnosis[] {
  const seen = new Set<string>();
  const merged: Diagnosis[] = [];
  for (const d of [...base, ...extra]) {
    const key = diagnosisKey(d);
    if (seen.has(key)) continue;
    seen.add(key);
    merged.push(d);
  }
  return sortDiagnoses(merged);
}

/** Maps a diagnosis severity onto the badge colours: blocking=fail, warning=warn, info=neutral. */
export function severityBadge(severity: Severity): BadgeStatus {
  switch (severity) {
    case "blocking":
      return "fail";
    case "warning":
      return "warn";
    case "info":
      return "skipped";
  }
}

/** The rule id of the most severe finding, or `null` when there is none (telemetry `ruleId`). */
export function primaryRuleId(diagnoses: readonly Diagnosis[]): string | null {
  const first = sortDiagnoses(diagnoses)[0];
  return first ? first.ruleId : null;
}
