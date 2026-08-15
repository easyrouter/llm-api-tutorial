import { describe, expect, it } from "vitest";

import type { InstallDoneEvent } from "@/lib/types";
import { ccSwitchRelease, checkResult, downloadResult, installPlan } from "@/test/fixtures/env";

import {
  allHandled,
  countHandled,
  createInstallState,
  hasRunningWork,
  installReducer,
  installTelemetryEvent,
  isBusy,
  itemBadge,
  itemOutcome,
  planOf,
  releaseOf,
  type InstallAction,
  type InstallState,
} from "./install-state";

const err = { code: "network", message: "boom", params: {} };
const plan = installPlan("codex");
const done = (over: Partial<InstallDoneEvent> = {}): InstallDoneEvent => ({
  jobId: "job-1",
  success: true,
  exitCode: 0,
  durationMs: 1000,
  cancelled: false,
  timedOut: false,
  ...over,
});

function reduce(state: InstallState, ...actions: InstallAction[]): InstallState {
  return actions.reduce(installReducer, state);
}

function codex(state: InstallState) {
  const item = state.items.codex;
  if (!item) throw new Error("codex item missing");
  return item;
}

describe("createInstallState", () => {
  it("creates idle items, skipped ones start skipped", () => {
    const s = createInstallState(["node", "codex"], ["node"]);
    expect(s.items.node?.step).toEqual({ phase: "skipped" });
    expect(s.items.codex?.step).toEqual({ phase: "idle" });
    expect(s.items["cc-switch"]).toBeUndefined();
    expect(allHandled(s, ["node", "codex"])).toBe(false);
    expect(countHandled(s, ["node", "codex"])).toBe(1);
  });

  it("ignores actions for unknown targets", () => {
    const s = createInstallState(["codex"]);
    expect(installReducer(s, { type: "plan_start", target: "node" })).toBe(s);
  });
});

describe("npm flow", () => {
  const base = createInstallState(["codex"]);

  it("plan → confirm → starting → running → done, then an automatic re-check is informational", () => {
    let s = reduce(base, { type: "plan_start", target: "codex" });
    expect(codex(s).step.phase).toBe("planning");
    expect(isBusy(codex(s))).toBe(true);

    s = reduce(s, { type: "plan_ok", target: "codex", plan });
    expect(codex(s).step).toEqual({ phase: "confirm", plan });
    expect(itemBadge(codex(s))).toEqual({ status: "pending", labelKey: "confirm" });
    expect(isBusy(codex(s))).toBe(false);

    s = reduce(s, { type: "run_start", target: "codex" });
    expect(codex(s).step.phase).toBe("starting");
    s = reduce(s, { type: "run_ok", target: "codex", job: { jobId: "job-1", target: "codex" } });
    expect(codex(s).step).toEqual({ phase: "running", plan, jobId: "job-1", cancelling: false });

    s = reduce(
      s,
      { type: "job_output", event: { jobId: "job-1", stream: "stdout", line: "added 1 package" } },
      { type: "job_output", event: { jobId: "other", stream: "stderr", line: "ignored" } },
    );
    expect(s.jobs["job-1"]?.lines).toEqual([{ stream: "stdout", line: "added 1 package" }]);
    expect(s.jobs.other?.lines).toHaveLength(1);

    s = reduce(s, { type: "job_done", event: done() });
    expect(codex(s).step).toEqual({ phase: "done", jobId: "job-1" });
    expect(itemOutcome(codex(s))).toBe("done");
    expect(allHandled(s, ["codex"])).toBe(true);

    s = reduce(s, { type: "recheck_start", target: "codex" });
    expect(itemBadge(codex(s))).toEqual({ status: "running", labelKey: "rechecking" });
    s = reduce(s, {
      type: "recheck_ok",
      target: "codex",
      result: checkResult("codex", "warn", { code: "tool.not_on_path" }),
    });
    expect(codex(s).step.phase).toBe("done");
    expect(codex(s).recheck.result?.code).toBe("tool.not_on_path");
  });

  it("a done event that arrives before run_ok is applied when the job id becomes known", () => {
    let s = reduce(
      base,
      { type: "plan_ok", target: "codex", plan },
      { type: "run_start", target: "codex" },
      { type: "job_output", event: { jobId: "job-1", stream: "stdout", line: "fast" } },
      { type: "job_done", event: done({ success: false, exitCode: 1 }) },
    );
    expect(codex(s).step.phase).toBe("starting");
    s = reduce(s, { type: "run_ok", target: "codex", job: { jobId: "job-1", target: "codex" } });
    expect(codex(s).step).toMatchObject({ phase: "failed", stage: "run", jobId: "job-1" });
    expect(planOf(codex(s).step)).toEqual(plan);
  });

  it("failure paths keep the plan and stage: plan / start / run / cancelled", () => {
    const planFailed = reduce(base, { type: "plan_failed", target: "codex", error: err });
    expect(codex(planFailed).step).toMatchObject({ phase: "failed", stage: "plan", error: err });
    expect(itemBadge(codex(planFailed))).toEqual({ status: "fail", labelKey: "failed" });

    const startFailed = reduce(
      base,
      { type: "plan_ok", target: "codex", plan },
      { type: "run_start", target: "codex" },
      { type: "run_failed", target: "codex", error: err },
    );
    expect(codex(startFailed).step).toMatchObject({ phase: "failed", stage: "start", plan });

    const running = reduce(
      base,
      { type: "plan_ok", target: "codex", plan },
      { type: "run_start", target: "codex" },
      { type: "run_ok", target: "codex", job: { jobId: "job-1", target: "codex" } },
    );
    const cancelling = reduce(running, { type: "cancel_requested", target: "codex" });
    expect(codex(cancelling).step).toMatchObject({ phase: "running", cancelling: true });
    expect(itemBadge(codex(cancelling)).labelKey).toBe("cancelling");
    const cancelled = reduce(cancelling, {
      type: "job_done",
      event: done({ success: false, exitCode: null, cancelled: true }),
    });
    expect(codex(cancelled).step).toMatchObject({
      phase: "failed",
      stage: "run",
      done: { cancelled: true },
    });
    // retry re-plans from a failed state
    expect(codex(reduce(cancelled, { type: "plan_start", target: "codex" })).step.phase).toBe(
      "planning",
    );
  });

  it("run_start / run_ok without a plan are ignored", () => {
    const s = reduce(base, { type: "run_start", target: "codex" });
    expect(codex(s).step.phase).toBe("idle");
    const s2 = reduce(base, {
      type: "run_ok",
      target: "codex",
      job: { jobId: "j", target: "codex" },
    });
    expect(codex(s2).step.phase).toBe("idle");
  });
});

describe("node flow", () => {
  it("stays in confirm until a re-check passes", () => {
    const nodePlan = installPlan("node", {
      displayCommand: "",
      explanationCode: "node.download_page",
    });
    let s = reduce(
      createInstallState(["node"]),
      { type: "plan_start", target: "node" },
      { type: "plan_ok", target: "node", plan: nodePlan },
      { type: "recheck_start", target: "node" },
      {
        type: "recheck_ok",
        target: "node",
        result: checkResult("node", "fail", { code: "node.missing" }),
      },
    );
    expect(s.items.node?.step.phase).toBe("confirm");
    expect(s.items.node?.recheck).toMatchObject({
      status: "done",
      result: { code: "node.missing" },
    });

    s = reduce(s, { type: "recheck_failed", target: "node", error: err });
    expect(s.items.node?.recheck).toEqual({ status: "failed", result: null, error: err });

    s = reduce(s, { type: "recheck_ok", target: "node", result: checkResult("node", "pass") });
    expect(s.items.node?.step).toEqual({ phase: "done", jobId: null });
  });
});

describe("cc-switch flow", () => {
  const release = ccSwitchRelease();
  const base = createInstallState(["cc-switch"]);
  const cc = (s: InstallState) => {
    const item = s.items["cc-switch"];
    if (!item) throw new Error("cc-switch item missing");
    return item;
  };

  it("fetch release → download (progress by job id) → downloaded → done after re-check", () => {
    let s = reduce(base, { type: "release_start", target: "cc-switch" });
    expect(cc(s).step.phase).toBe("fetching_release");
    s = reduce(s, { type: "release_ok", target: "cc-switch", release });
    expect(cc(s).step).toEqual({ phase: "release", release });
    expect(releaseOf(cc(s).step)).toEqual(release);

    s = reduce(s, { type: "download_start", target: "cc-switch", jobId: "dl-1" });
    expect(cc(s).step).toEqual({ phase: "downloading", release, jobId: "dl-1" });
    s = reduce(s, {
      type: "download_progress",
      event: { jobId: "dl-1", downloaded: 500, total: 1000 },
    });
    expect(s.downloads["dl-1"]).toEqual({ downloaded: 500, total: 1000 });

    const result = downloadResult();
    s = reduce(s, { type: "download_ok", target: "cc-switch", download: result });
    expect(cc(s).step).toEqual({ phase: "downloaded", release, download: result });
    expect(itemBadge(cc(s))).toEqual({ status: "pending", labelKey: "downloaded" });

    s = reduce(s, {
      type: "recheck_ok",
      target: "cc-switch",
      result: checkResult("cc_switch", "pass"),
    });
    expect(cc(s).step).toEqual({ phase: "done", jobId: null });
  });

  it("failures carry the stage and the release for a download retry", () => {
    const releaseFailed = reduce(base, {
      type: "release_failed",
      target: "cc-switch",
      error: err,
    });
    expect(cc(releaseFailed).step).toMatchObject({ phase: "failed", stage: "release" });

    const downloadFailed = reduce(
      base,
      { type: "release_ok", target: "cc-switch", release },
      { type: "download_start", target: "cc-switch", jobId: "dl-1" },
      { type: "download_failed", target: "cc-switch", error: err },
    );
    expect(cc(downloadFailed).step).toMatchObject({ phase: "failed", stage: "download", release });
  });

  it("download_start without a release is ignored", () => {
    const s = reduce(base, { type: "download_start", target: "cc-switch", jobId: "dl-1" });
    expect(cc(s).step.phase).toBe("idle");
  });
});

describe("skip / unskip", () => {
  it("skip counts as handled; unskip returns to idle and clears the re-check", () => {
    let s = reduce(
      createInstallState(["codex"]),
      { type: "plan_ok", target: "codex", plan },
      { type: "recheck_ok", target: "codex", result: checkResult("codex", "fail") },
      { type: "skip", target: "codex" },
    );
    expect(itemOutcome(codex(s))).toBe("skipped");
    expect(allHandled(s, ["codex"])).toBe(true);
    expect(itemBadge(codex(s))).toEqual({ status: "skipped", labelKey: "skipped" });

    s = reduce(s, { type: "unskip", target: "codex" });
    expect(codex(s).step).toEqual({ phase: "idle" });
    expect(codex(s).recheck.status).toBe("idle");
    expect(itemBadge(codex(s))).toEqual({ status: "pending", labelKey: "idle" });
  });

  it("a passing re-check never reopens a skipped item", () => {
    const s = reduce(
      createInstallState(["codex"]),
      { type: "skip", target: "codex" },
      { type: "recheck_ok", target: "codex", result: checkResult("codex", "pass") },
    );
    expect(codex(s).step.phase).toBe("skipped");
  });
});

describe("hasRunningWork / installTelemetryEvent", () => {
  it("is true only while a job runs or a download is in flight", () => {
    const targets = ["codex", "cc-switch"] as const;
    let state = createInstallState(targets);
    expect(hasRunningWork(state, targets)).toBe(false);

    state = reduce(state, { type: "plan_start", target: "codex" });
    expect(hasRunningWork(state, targets)).toBe(false);
    state = reduce(
      state,
      { type: "plan_ok", target: "codex", plan },
      { type: "run_start", target: "codex" },
    );
    expect(hasRunningWork(state, targets)).toBe(true);
    state = reduce(state, {
      type: "run_ok",
      target: "codex",
      job: { jobId: "job-1", target: "codex" },
    });
    expect(hasRunningWork(state, targets)).toBe(true);
    state = reduce(state, { type: "job_done", event: done() });
    expect(hasRunningWork(state, targets)).toBe(false);

    state = reduce(
      state,
      { type: "release_ok", target: "cc-switch", release: ccSwitchRelease() },
      { type: "download_start", target: "cc-switch", jobId: "dl-1" },
    );
    expect(hasRunningWork(state, targets)).toBe(true);
    state = reduce(state, { type: "download_ok", target: "cc-switch", download: downloadResult() });
    expect(hasRunningWork(state, targets)).toBe(false);
    expect(hasRunningWork(state, ["node"])).toBe(false);
  });

  it("builds a step_result event with enumerated fields only", () => {
    expect(installTelemetryEvent("pass", 1234.6)).toEqual({
      name: "step_result",
      step: "install",
      status: "pass",
      durationMs: 1235,
      errorClass: null,
      ruleId: null,
    });
    expect(installTelemetryEvent("fail", null).durationMs).toBeNull();
    expect(installTelemetryEvent("fail", -5).durationMs).toBe(0);
  });
});
