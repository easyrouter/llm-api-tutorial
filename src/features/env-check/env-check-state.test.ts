import { describe, expect, it } from "vitest";

import type { Translate } from "@/lib/errors";
import type { CheckId, CheckResult, CheckStatus } from "@/lib/types";
import { CHECK_IDS } from "@/lib/types";
import { checkResult, envSnapshot } from "@/test/fixtures/env";

import {
  envCheckReducer,
  failuresAreInstallFixable,
  initialEnvCheckState,
  isInstallFixable,
  nextGate,
  overallOf,
  rowStatus,
  summarize,
  visibleCheckIds,
  type EnvCheckState,
} from "./env-check-state";
import { applyResultToSnapshot, checkMessage } from "./snapshot-utils";

const err = { code: "network", message: "boom", params: {} };

function doneState(results: CheckResult[]): EnvCheckState {
  return envCheckReducer(envCheckReducer(initialEnvCheckState, { type: "start_all" }), {
    type: "all_done",
    snapshot: envSnapshot(Object.fromEntries(results.map((r) => [r.id, r]))),
  });
}

describe("visibleCheckIds", () => {
  it("hides the rows of tools that were not selected", () => {
    expect(visibleCheckIds(["codex", "claude-code"])).toEqual(CHECK_IDS);
    expect(visibleCheckIds(["codex"])).not.toContain("claude_code");
    expect(visibleCheckIds(["codex"])).toContain("codex");
    expect(visibleCheckIds([])).toEqual(
      CHECK_IDS.filter((id) => id !== "codex" && id !== "claude_code"),
    );
  });

  it("hides the Windows Terminal row on non-Windows platforms only", () => {
    expect(visibleCheckIds(["codex"], "windows")).toContain("windows_terminal");
    expect(visibleCheckIds(["codex"], "macos")).not.toContain("windows_terminal");
    expect(visibleCheckIds(["codex"], "linux")).not.toContain("windows_terminal");
    // platform unknown (app info still loading): keep the row; Rust reports it as skipped
    expect(visibleCheckIds(["codex"], null)).toContain("windows_terminal");
  });
});

describe("overallOf", () => {
  it.each<[CheckStatus[], CheckStatus]>([
    [[], "skipped"],
    [["skipped"], "skipped"],
    [["pass", "skipped"], "pass"],
    [["pass", "warn"], "warn"],
    [["warn", "fail", "pass"], "fail"],
  ])("%j → %s", (statuses, expected) => {
    const results = statuses.map((s, i) => checkResult(CHECK_IDS[i] as CheckId, s));
    expect(overallOf(results)).toBe(expected);
  });
});

describe("envCheckReducer", () => {
  it("start_all clears previous results and marks every row running", () => {
    const seeded = doneState([checkResult("os", "fail")]);
    const s = envCheckReducer(seeded, { type: "start_all" });
    expect(s.phase).toBe("running");
    expect(s.results).toEqual({});
    expect(rowStatus(s, "os")).toBe("running");
    expect(rowStatus(initialEnvCheckState, "os")).toBe("pending");
  });

  it("streams results row by row and all_done makes the snapshot authoritative", () => {
    let s = envCheckReducer(initialEnvCheckState, { type: "start_all" });
    s = envCheckReducer(s, { type: "result", result: checkResult("node", "warn") });
    expect(rowStatus(s, "node")).toBe("warn");
    expect(rowStatus(s, "npm")).toBe("running");

    const final = envSnapshot({ node: checkResult("node", "pass") });
    s = envCheckReducer(s, { type: "all_done", snapshot: final });
    expect(s.phase).toBe("done");
    expect(rowStatus(s, "node")).toBe("pass");
    expect(rowStatus(s, "npm")).toBe("pass");
  });

  it("all_failed keeps streamed rows and records the error", () => {
    let s = envCheckReducer(initialEnvCheckState, { type: "start_all" });
    s = envCheckReducer(s, { type: "result", result: checkResult("os") });
    s = envCheckReducer(s, { type: "all_failed", error: err });
    expect(s.phase).toBe("error");
    expect(s.error).toEqual(err);
    expect(rowStatus(s, "os")).toBe("pass");
  });

  it("re-running one row: running → replaced result, or error kept per row", () => {
    let s = doneState([checkResult("codex", "fail", { code: "tool.missing" })]);
    s = envCheckReducer(s, { type: "rerun_start", id: "codex" });
    expect(rowStatus(s, "codex")).toBe("running");
    expect(nextGate(s, CHECK_IDS).canProceed).toBe(false);

    const ok = envCheckReducer(s, { type: "rerun_done", result: checkResult("codex", "pass") });
    expect(rowStatus(ok, "codex")).toBe("pass");
    expect(ok.rerunning).toEqual([]);

    const failed = envCheckReducer(s, { type: "rerun_failed", id: "codex", error: err });
    expect(rowStatus(failed, "codex")).toBe("fail");
    expect(failed.rowErrors.codex).toEqual(err);
    // the next rerun_start clears the row error
    expect(envCheckReducer(failed, { type: "rerun_start", id: "codex" }).rowErrors.codex).toBe(
      undefined,
    );
  });

  it("acknowledge toggles and start_all resets it", () => {
    let s = envCheckReducer(initialEnvCheckState, { type: "acknowledge", value: true });
    expect(s.acknowledged).toBe(true);
    s = envCheckReducer(s, { type: "start_all" });
    expect(s.acknowledged).toBe(false);
  });
});

describe("summarize", () => {
  it("counts only the visible rows", () => {
    const s = doneState([
      checkResult("codex", "fail"),
      checkResult("claude_code", "fail"),
      checkResult("node", "warn"),
    ]);
    const all = summarize(s, CHECK_IDS);
    expect(all).toMatchObject({ total: 11, done: 11, fail: 2, warn: 1, pass: 8, overall: "fail" });
    const codexOnly = summarize(s, visibleCheckIds(["codex"]));
    expect(codexOnly).toMatchObject({ total: 10, fail: 1, overall: "fail" });
    const none = summarize(s, visibleCheckIds([]));
    expect(none).toMatchObject({ total: 9, fail: 0, warn: 1, overall: "warn" });
  });

  it("reports progress while running", () => {
    let s = envCheckReducer(initialEnvCheckState, { type: "start_all" });
    s = envCheckReducer(s, { type: "result", result: checkResult("os") });
    expect(summarize(s, CHECK_IDS)).toMatchObject({ total: 11, done: 1, pass: 1 });
  });
});

describe("install-fixable failures and the Next gate", () => {
  const missingCodex = checkResult("codex", "fail", {
    code: "tool.missing",
    fixes: [{ kind: "install", tool: "codex" }],
  });
  const missingNode = checkResult("node", "fail", {
    code: "node.missing",
    fixes: [{ kind: "open_url", url: "https://nodejs.org", labelCode: "node_download" }],
  });
  const oldOs = checkResult("os", "fail", { code: "os.too_old" });

  it("isInstallFixable needs an install or open_url fix", () => {
    expect(isInstallFixable(missingCodex)).toBe(true);
    expect(isInstallFixable(missingNode)).toBe(true);
    expect(isInstallFixable(oldOs)).toBe(false);
    expect(failuresAreInstallFixable([missingCodex, missingNode])).toBe(true);
    expect(failuresAreInstallFixable([missingCodex, oldOs])).toBe(false);
    expect(failuresAreInstallFixable([checkResult("os")])).toBe(false);
  });

  it("is closed while running and open when nothing fails", () => {
    expect(nextGate(initialEnvCheckState, CHECK_IDS)).toEqual({
      canProceed: false,
      offerContinueAnyway: false,
    });
    expect(nextGate(doneState([checkResult("node", "warn")]), CHECK_IDS)).toEqual({
      canProceed: true,
      offerContinueAnyway: false,
    });
  });

  it("offers continue-anyway only for install-fixable failures and requires the checkbox", () => {
    const fixable = doneState([missingCodex, missingNode]);
    expect(nextGate(fixable, CHECK_IDS)).toEqual({ canProceed: false, offerContinueAnyway: true });
    const acked = envCheckReducer(fixable, { type: "acknowledge", value: true });
    expect(nextGate(acked, CHECK_IDS).canProceed).toBe(true);

    const hard = doneState([missingCodex, oldOs]);
    expect(nextGate(hard, CHECK_IDS)).toEqual({ canProceed: false, offerContinueAnyway: false });
    const hardAcked = envCheckReducer(hard, { type: "acknowledge", value: true });
    expect(nextGate(hardAcked, CHECK_IDS).canProceed).toBe(false);
  });

  it("ignores failures of hidden tool rows", () => {
    const s = doneState([checkResult("claude_code", "fail", { code: "tool.missing" })]);
    expect(nextGate(s, visibleCheckIds(["codex"])).canProceed).toBe(true);
    expect(nextGate(s, CHECK_IDS).canProceed).toBe(false);
  });
});

describe("checkMessage", () => {
  const t: Translate = (key, options) => {
    const params = options ?? {};
    switch (key) {
      case "common:tools.codex":
        return "Codex CLI";
      case "checks:tool.ok":
        return `${String(params.tool)} ${String(params.version)} is installed`;
      case "checks:screen.unknownCode":
        return `unknown (${String(params.code)})`;
      default:
        return typeof params.defaultValue === "string" ? params.defaultValue : key;
    }
  };

  it("maps the tool param through common:tools and defaults it from the check id", () => {
    expect(checkMessage(t, { id: "codex", code: "tool.ok", params: { version: "1.0" } })).toBe(
      "Codex CLI 1.0 is installed",
    );
    expect(
      checkMessage(t, { id: "codex", code: "tool.ok", params: { tool: "codex", version: "2" } }),
    ).toBe("Codex CLI 2 is installed");
    // unknown tool ids are shown verbatim
    expect(
      checkMessage(t, { id: "os", code: "tool.ok", params: { tool: "gemini", version: "3" } }),
    ).toBe("gemini 3 is installed");
  });

  it("falls back to a neutral sentence for unknown codes", () => {
    expect(checkMessage(t, { id: "os", code: "os.brand_new", params: {} })).toBe(
      "unknown (os.brand_new)",
    );
  });
});

describe("applyResultToSnapshot", () => {
  it("replaces the row and recomputes overall; null stays null", () => {
    const snap = envSnapshot({ codex: checkResult("codex", "fail") });
    expect(snap.overall).toBe("fail");
    const updated = applyResultToSnapshot(snap, checkResult("codex", "pass"));
    expect(updated?.overall).toBe("pass");
    expect(updated?.checks.filter((c) => c.id === "codex")).toHaveLength(1);
    expect(applyResultToSnapshot(null, checkResult("codex"))).toBeNull();
  });

  it("appends a row that was not in the snapshot", () => {
    const snap = envSnapshot();
    const shorter = { ...snap, checks: snap.checks.filter((c) => c.id !== "npm") };
    const updated = applyResultToSnapshot(shorter, checkResult("npm", "warn"));
    expect(updated?.checks.map((c) => c.id)).toContain("npm");
    expect(updated?.overall).toBe("warn");
  });
});
