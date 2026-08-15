/**
 * Pure state machine for the Install screen (M2). One reducer owns every install item plus the
 * streamed job output and download progress, keyed by job id, so events that arrive before
 * the `start_install` / `download_file` promise resolves are never lost.
 *
 * Per-target flows (see `installKind`):
 * - npm (Codex, Claude Code): idle → planning → confirm → starting → running → done | failed
 *   (done triggers an automatic re-check whose result is shown as information)
 * - node: idle → planning → confirm (open download page, then "I installed it — re-check";
 *   a passing re-check moves the item to done)
 * - cc-switch: idle → fetching_release → release → downloading → downloaded (open installer,
 *   then "I installed it — re-check"; a passing re-check moves the item to done)
 * Every phase can be skipped; failures carry the stage so Retry knows where to resume.
 */
import type { BadgeStatus, LogLine } from "@/components/ui";
import type {
  CcSwitchRelease,
  CheckResult,
  DownloadProgressEvent,
  DownloadResult,
  InstallDoneEvent,
  InstallJob,
  InstallOutputEvent,
  InstallPlan,
  InstallTarget,
  TelemetryEvent,
  WireError,
} from "@/lib/types";

export type FailStage = "plan" | "start" | "run" | "release" | "download";

export type ItemPhase =
  | { phase: "idle" }
  | { phase: "skipped" }
  | { phase: "planning" }
  | { phase: "confirm"; plan: InstallPlan }
  | { phase: "starting"; plan: InstallPlan }
  | { phase: "running"; plan: InstallPlan; jobId: string; cancelling: boolean }
  | { phase: "fetching_release" }
  | { phase: "release"; release: CcSwitchRelease }
  | { phase: "downloading"; release: CcSwitchRelease; jobId: string }
  | { phase: "downloaded"; release: CcSwitchRelease; download: DownloadResult }
  | { phase: "done"; jobId: string | null }
  | {
      phase: "failed";
      stage: FailStage;
      error: WireError | null;
      /** Set when an install job ended unsuccessfully (exit code / cancelled). */
      done: InstallDoneEvent | null;
      plan: InstallPlan | null;
      release: CcSwitchRelease | null;
      jobId: string | null;
    };

export type RecheckStatus = "idle" | "running" | "done" | "failed";

export interface RecheckState {
  status: RecheckStatus;
  result: CheckResult | null;
  error: WireError | null;
}

export interface ItemState {
  target: InstallTarget;
  step: ItemPhase;
  recheck: RecheckState;
}

export interface JobLog {
  lines: LogLine[];
  done: InstallDoneEvent | null;
}

export interface DownloadProgress {
  downloaded: number;
  total: number | null;
}

export interface InstallState {
  items: Partial<Record<InstallTarget, ItemState>>;
  jobs: Record<string, JobLog>;
  downloads: Record<string, DownloadProgress>;
}

type Targeted<T extends string, P = object> = { type: T; target: InstallTarget } & P;

export type InstallAction =
  | Targeted<"plan_start">
  | Targeted<"plan_ok", { plan: InstallPlan }>
  | Targeted<"plan_failed", { error: WireError }>
  | Targeted<"run_start">
  | Targeted<"run_ok", { job: InstallJob }>
  | Targeted<"run_failed", { error: WireError }>
  | Targeted<"cancel_requested">
  | { type: "job_output"; event: InstallOutputEvent }
  | { type: "job_done"; event: InstallDoneEvent }
  | Targeted<"release_start">
  | Targeted<"release_ok", { release: CcSwitchRelease }>
  | Targeted<"release_failed", { error: WireError }>
  | Targeted<"download_start", { jobId: string }>
  | { type: "download_progress"; event: DownloadProgressEvent }
  | Targeted<"download_ok", { download: DownloadResult }>
  | Targeted<"download_failed", { error: WireError }>
  | Targeted<"recheck_start">
  | Targeted<"recheck_ok", { result: CheckResult }>
  | Targeted<"recheck_failed", { error: WireError }>
  | Targeted<"skip">
  | Targeted<"unskip">;

const idleRecheck: RecheckState = { status: "idle", result: null, error: null };

/** Initial state for `targets`; `skipped` targets start in the skipped phase. */
export function createInstallState(
  targets: readonly InstallTarget[],
  skipped: readonly InstallTarget[] = [],
): InstallState {
  const items: Partial<Record<InstallTarget, ItemState>> = {};
  for (const target of targets) {
    items[target] = {
      target,
      step: { phase: skipped.includes(target) ? "skipped" : "idle" },
      recheck: idleRecheck,
    };
  }
  return { items, jobs: {}, downloads: {} };
}

function failed(
  stage: FailStage,
  error: WireError | null,
  extra: Partial<Extract<ItemPhase, { phase: "failed" }>> = {},
): ItemPhase {
  return {
    phase: "failed",
    stage,
    error,
    done: null,
    plan: null,
    release: null,
    jobId: null,
    ...extra,
  };
}

/** Phase after an install job finished. */
function phaseAfterJob(event: InstallDoneEvent, plan: InstallPlan): ItemPhase {
  if (event.success && !event.cancelled) return { phase: "done", jobId: event.jobId };
  return failed("run", null, { done: event, plan, jobId: event.jobId });
}

function jobLog(state: InstallState, jobId: string): JobLog {
  return state.jobs[jobId] ?? { lines: [], done: null };
}

function withItem(
  state: InstallState,
  target: InstallTarget,
  update: (item: ItemState) => ItemState,
): InstallState {
  const item = state.items[target];
  if (!item) return state;
  return { ...state, items: { ...state.items, [target]: update(item) } };
}

function setStep(state: InstallState, target: InstallTarget, step: ItemPhase): InstallState {
  return withItem(state, target, (item) => ({ ...item, step }));
}

function reduceItem(state: InstallState, action: InstallAction): InstallState {
  switch (action.type) {
    case "plan_start":
      return setStep(state, action.target, { phase: "planning" });
    case "plan_ok":
      return setStep(state, action.target, { phase: "confirm", plan: action.plan });
    case "plan_failed":
      return setStep(state, action.target, failed("plan", action.error));
    case "run_start":
      return withItem(state, action.target, (item) => {
        const plan = planOf(item.step);
        return plan ? { ...item, step: { phase: "starting", plan } } : item;
      });
    case "run_ok":
      return withItem(state, action.target, (item) => {
        const plan = planOf(item.step);
        if (!plan) return item;
        const early = state.jobs[action.job.jobId]?.done;
        const step: ItemPhase = early
          ? phaseAfterJob(early, plan)
          : { phase: "running", plan, jobId: action.job.jobId, cancelling: false };
        return { ...item, step };
      });
    case "run_failed":
      return withItem(state, action.target, (item) => ({
        ...item,
        step: failed("start", action.error, { plan: planOf(item.step) }),
      }));
    case "cancel_requested":
      return withItem(state, action.target, (item) =>
        item.step.phase === "running"
          ? { ...item, step: { ...item.step, cancelling: true } }
          : item,
      );
    case "release_start":
      return setStep(state, action.target, { phase: "fetching_release" });
    case "release_ok":
      return setStep(state, action.target, { phase: "release", release: action.release });
    case "release_failed":
      return setStep(state, action.target, failed("release", action.error));
    case "download_start":
      return withItem(state, action.target, (item) => {
        const release = releaseOf(item.step);
        return release
          ? { ...item, step: { phase: "downloading", release, jobId: action.jobId } }
          : item;
      });
    case "download_ok":
      return withItem(state, action.target, (item) => {
        const release = releaseOf(item.step);
        return release
          ? { ...item, step: { phase: "downloaded", release, download: action.download } }
          : item;
      });
    case "download_failed":
      return withItem(state, action.target, (item) => ({
        ...item,
        step: failed("download", action.error, { release: releaseOf(item.step) }),
      }));
    case "recheck_start":
      return withItem(state, action.target, (item) => ({
        ...item,
        recheck: { status: "running", result: null, error: null },
      }));
    case "recheck_ok":
      return withItem(state, action.target, (item) => {
        const passed = action.result.status === "pass";
        const closable = item.step.phase !== "skipped" && item.step.phase !== "done";
        return {
          ...item,
          recheck: { status: "done", result: action.result, error: null },
          step: passed && closable ? { phase: "done", jobId: null } : item.step,
        };
      });
    case "recheck_failed":
      return withItem(state, action.target, (item) => ({
        ...item,
        recheck: { status: "failed", result: null, error: action.error },
      }));
    case "skip":
      return setStep(state, action.target, { phase: "skipped" });
    case "unskip":
      return withItem(state, action.target, (item) => ({
        ...item,
        step: { phase: "idle" },
        recheck: idleRecheck,
      }));
    default:
      return state;
  }
}

export function installReducer(state: InstallState, action: InstallAction): InstallState {
  switch (action.type) {
    case "job_output": {
      const { jobId, stream, line } = action.event;
      const log = jobLog(state, jobId);
      return {
        ...state,
        jobs: { ...state.jobs, [jobId]: { ...log, lines: [...log.lines, { stream, line }] } },
      };
    }
    case "job_done": {
      const { event } = action;
      const jobs = { ...state.jobs, [event.jobId]: { ...jobLog(state, event.jobId), done: event } };
      const next = { ...state, jobs };
      const running = findItemByJob(state, event.jobId);
      if (!running || running.step.phase !== "running") return next;
      return setStep(next, running.target, phaseAfterJob(event, running.step.plan));
    }
    case "download_progress": {
      const { jobId, downloaded, total } = action.event;
      return { ...state, downloads: { ...state.downloads, [jobId]: { downloaded, total } } };
    }
    default:
      return reduceItem(state, action);
  }
}

// ---------------------------------------------------------------------------
// selectors
// ---------------------------------------------------------------------------

/** The plan carried by a phase, if any (confirm/starting/running/failed-after-plan). */
export function planOf(step: ItemPhase): InstallPlan | null {
  switch (step.phase) {
    case "confirm":
    case "starting":
    case "running":
      return step.plan;
    case "failed":
      return step.plan;
    default:
      return null;
  }
}

/** The CC Switch release carried by a phase, if any. */
export function releaseOf(step: ItemPhase): CcSwitchRelease | null {
  switch (step.phase) {
    case "release":
    case "downloading":
    case "downloaded":
      return step.release;
    case "failed":
      return step.release;
    default:
      return null;
  }
}

/** Job id of the currently running / finished install job for an item, if any. */
export function jobIdOf(step: ItemPhase): string | null {
  switch (step.phase) {
    case "running":
    case "done":
    case "failed":
      return step.jobId;
    default:
      return null;
  }
}

export function findItemByJob(state: InstallState, jobId: string): ItemState | null {
  for (const item of Object.values(state.items)) {
    if (item && jobIdOf(item.step) === jobId) return item;
  }
  return null;
}

export type ItemOutcome = "pending" | "done" | "skipped";

export function itemOutcome(item: ItemState): ItemOutcome {
  if (item.step.phase === "done") return "done";
  if (item.step.phase === "skipped") return "skipped";
  return "pending";
}

/** True when every listed target is done or skipped (Next may be enabled). */
export function allHandled(state: InstallState, targets: readonly InstallTarget[]): boolean {
  return targets.every((t) => {
    const item = state.items[t];
    return item ? itemOutcome(item) !== "pending" : true;
  });
}

export function countHandled(state: InstallState, targets: readonly InstallTarget[]): number {
  return targets.filter((t) => {
    const item = state.items[t];
    return item ? itemOutcome(item) !== "pending" : false;
  }).length;
}

/** True while an IPC call or job for this item is in flight (buttons should be disabled). */
export function isBusy(item: ItemState): boolean {
  if (item.recheck.status === "running") return true;
  switch (item.step.phase) {
    case "planning":
    case "starting":
    case "running":
    case "fetching_release":
    case "downloading":
      return true;
    default:
      return false;
  }
}

/**
 * Badge for the card header: colour from `BadgeStatus`, label key under `install:state.*`
 * (`rechecking` / `cancelling` win over the phase while they are in flight).
 */
export function itemBadge(item: ItemState): { status: BadgeStatus; labelKey: string } {
  if (item.recheck.status === "running") return { status: "running", labelKey: "rechecking" };
  const { step } = item;
  switch (step.phase) {
    case "idle":
      return { status: "pending", labelKey: "idle" };
    case "skipped":
      return { status: "skipped", labelKey: "skipped" };
    case "done":
      return { status: "pass", labelKey: "done" };
    case "failed":
      return { status: "fail", labelKey: "failed" };
    case "confirm":
    case "release":
    case "downloaded":
      return { status: "pending", labelKey: step.phase };
    case "running":
      return { status: "running", labelKey: step.cancelling ? "cancelling" : "running" };
    case "planning":
    case "starting":
    case "fetching_release":
    case "downloading":
      return { status: "running", labelKey: step.phase };
  }
}

// ---------------------------------------------------------------------------
// telemetry
// ---------------------------------------------------------------------------

/**
 * `step_result` event for the Install step (PRD #16 "install success rate / failing stage"):
 * `pass` for a successful npm job or verified download, `fail` for a failed / cancelled job or a
 * failed download. Carries duration only — no package names, URLs, paths or output.
 */
export function installTelemetryEvent(
  status: "pass" | "fail",
  durationMs: number | null,
): TelemetryEvent {
  return {
    name: "step_result",
    step: "install",
    status,
    durationMs: durationMs === null ? null : Math.max(0, Math.round(durationMs)),
    errorClass: null,
    ruleId: null,
  };
}

/**
 * True while a job or download that would be orphaned by leaving the screen is in flight
 * (`starting` / `running` npm job, `downloading` installer). Short IPC calls (planning,
 * fetching a release, re-checking) do not count.
 */
export function hasRunningWork(state: InstallState, targets: readonly InstallTarget[]): boolean {
  return targets.some((t) => {
    const item = state.items[t];
    if (!item) return false;
    switch (item.step.phase) {
      case "starting":
      case "running":
      case "downloading":
        return true;
      default:
        return false;
    }
  });
}
