import { describe, expect, it } from "vitest";

import type { Diagnosis, Severity } from "@/lib/types";

import {
  diagnosisKey,
  mergeDiagnoses,
  primaryRuleId,
  severityBadge,
  sortDiagnoses,
} from "./diagnoses";

const d = (ruleId: string, severity: Severity, code = `code.${ruleId}`): Diagnosis => ({
  ruleId,
  severity,
  code,
  params: {},
  actions: [],
  checklist: [],
});

describe("diagnoses helpers", () => {
  it("keys a diagnosis by rule + code", () => {
    expect(diagnosisKey(d("A", "blocking", "auth.key_checklist"))).toBe("A:auth.key_checklist");
  });

  it("sorts blocking → warning → info, keeping engine order within a severity", () => {
    const sorted = sortDiagnoses([
      d("C", "warning"),
      d("X", "info"),
      d("A", "blocking"),
      d("D", "warning"),
      d("E", "blocking"),
    ]);
    expect(sorted.map((x) => x.ruleId)).toEqual(["A", "E", "C", "D", "X"]);
  });

  it("does not mutate the input", () => {
    const input = [d("C", "warning"), d("A", "blocking")];
    sortDiagnoses(input);
    expect(input.map((x) => x.ruleId)).toEqual(["C", "A"]);
  });

  it("merges without duplicates (first wins) and sorts the result", () => {
    const base = [d("C", "warning"), d("A", "blocking")];
    const extra = [d("A", "blocking"), d("D", "warning"), d("C", "info", "other.code")];
    const merged = mergeDiagnoses(base, extra);
    expect(merged.map(diagnosisKey)).toEqual(["A:code.A", "C:code.C", "D:code.D", "C:other.code"]);
  });

  it.each<[Severity, string]>([
    ["blocking", "fail"],
    ["warning", "warn"],
    ["info", "skipped"],
  ])("maps %s to badge %s", (severity, badge) => {
    expect(severityBadge(severity)).toBe(badge);
  });

  it("reports the most severe rule id (or null)", () => {
    expect(primaryRuleId([])).toBeNull();
    expect(primaryRuleId([d("C", "warning"), d("B", "blocking")])).toBe("B");
  });
});
