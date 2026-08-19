/**
 * Wires the Install screen's pure reducer (`install-state.ts`) to the Rust core: typed
 * `invoke` wrappers for plan / start / cancel / release / download / run-plan / re-check, and
 * the three event channels (`install://output`, `install://done`, `download://progress`) which are
 * subscribed once for the lifetime of the screen and released on unmount.
 *
 * Side effects that follow a state transition (auto re-check after a successful install job,
 * planning the installer command right after a verified download for targets this tool runs
 * itself, forgetting an explicit request once the target is installed, one `step_result`
 * telemetry event per finished job / download) live here, not in components.
 */
import { useCallback, useEffect, useMemo, useReducer, useRef } from "react";

import { onDownloadProgress, onInstallDone, onInstallOutput } from "@/lib/events";
import { toWireError } from "@/lib/errors";
import {
  cancelInstall,
  downloadFile,
  fetchInstallerRelease,
  planInstall,
  planInstallerRun,
  runEnvCheck,
  startInstall,
  trackEvent,
} from "@/lib/tauri";
import type { InstallDoneEvent, InstallPlan, InstallTarget, InstallerRelease } from "@/lib/types";
import { applyResultToSnapshot } from "@/features/env-check/snapshot-utils";
import { useInstallStore } from "@/stores/install";
import { useWizardStore } from "@/stores/wizard";

import {
  createInstallState,
  installReducer,
  installTelemetryEvent,
  type InstallState,
} from "./install-state";
import { RECHECK_ID, runsInstaller } from "./install-targets";

type Unlisten = () => void;

export interface PlanOptions {
  /** npm registry id a previous attempt failed on; the new plan uses another one when possible. */
  excludeRegistry?: string | null;
}

export interface InstallActions {
  /** idle/failed → planning → confirm. */
  plan: (target: InstallTarget, options?: PlanOptions) => Promise<void>;
  /** confirm → starting → running (the user confirmed the displayed command). */
  run: (target: InstallTarget, plan: InstallPlan) => Promise<void>;
  cancel: (target: InstallTarget, jobId: string) => Promise<void>;
  /** idle/failed → fetching_release → release (`fetchInstallerRelease`). */
  fetchRelease: (target: InstallTarget) => Promise<void>;
  /**
   * release → downloading → downloaded. For targets this tool runs itself (`runsInstaller`)
   * a verified download continues straight into `planRun`.
   */
  download: (target: InstallTarget, release: InstallerRelease) => Promise<void>;
  /**
   * downloaded/failed → run_planning → downloaded (with `runPlan`): asks Rust for the command
   * that runs the downloaded installer (`planInstallerRun`) so it can be shown before `run`.
   */
  planRun: (target: InstallTarget, path: string) => Promise<void>;
  /** Re-runs the target's check; a passing result marks the item done. */
  recheck: (target: InstallTarget) => Promise<void>;
  skip: (target: InstallTarget) => void;
  unskip: (target: InstallTarget) => void;
}

export interface InstallController {
  state: InstallState;
  actions: InstallActions;
}

/** Fire-and-forget telemetry (opt-in and enumerated fields only; never blocks the flow). */
function track(event: ReturnType<typeof installTelemetryEvent>): void {
  trackEvent(event).catch(() => undefined);
}

function newJobId(): string {
  const c = globalThis.crypto;
  if (c && typeof c.randomUUID === "function") return c.randomUUID();
  return `job-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

/** Subscribes to every channel; returns a disposer that also handles late registrations. */
function subscribeAll(subscribers: (() => Promise<Unlisten>)[]): Unlisten {
  let disposed = false;
  const active: Unlisten[] = [];
  for (const subscribe of subscribers) {
    subscribe()
      .then((unlisten) => {
        if (disposed) unlisten();
        else active.push(unlisten);
      })
      .catch(() => undefined);
  }
  return () => {
    disposed = true;
    for (const unlisten of active.splice(0)) unlisten();
  };
}

export function useInstallController(
  targets: readonly InstallTarget[],
  skipped: readonly InstallTarget[],
): InstallController {
  const [state, dispatch] = useReducer(
    installReducer,
    { targets, skipped },
    ({ targets: t, skipped: s }) => createInstallState(t, s),
  );
  const setSnapshot = useWizardStore((s) => s.setSnapshot);
  const storeSkip = useInstallStore((s) => s.skip);
  const storeUnskip = useInstallStore((s) => s.unskip);
  const unrequest = useInstallStore((s) => s.unrequest);

  /** job id → target, so `install://done` can trigger follow-up work for the right item. */
  const jobTargets = useRef(new Map<string, InstallTarget>());
  /** `install://done` events that arrived before `start_install` resolved. */
  const earlyDone = useRef(new Map<string, InstallDoneEvent>());

  const recheck = useCallback(
    async (target: InstallTarget) => {
      dispatch({ type: "recheck_start", target });
      try {
        const result = await runEnvCheck(RECHECK_ID[target]);
        dispatch({ type: "recheck_ok", target, result });
        setSnapshot(applyResultToSnapshot(useWizardStore.getState().snapshot, result));
        if (result.status === "pass") unrequest(target);
      } catch (e) {
        dispatch({ type: "recheck_failed", target, error: toWireError(e) });
      }
    },
    [setSnapshot, unrequest],
  );

  const afterJobDone = useCallback(
    (target: InstallTarget, event: InstallDoneEvent) => {
      jobTargets.current.delete(event.jobId);
      const passed = event.success && !event.cancelled;
      track(installTelemetryEvent(passed ? "pass" : "fail", event.durationMs));
      if (passed) {
        unrequest(target);
        void recheck(target);
      }
    },
    [recheck, unrequest],
  );

  useEffect(
    () =>
      subscribeAll([
        () => onInstallOutput((event) => dispatch({ type: "job_output", event })),
        () =>
          onInstallDone((event) => {
            dispatch({ type: "job_done", event });
            const target = jobTargets.current.get(event.jobId);
            if (target) afterJobDone(target, event);
            else earlyDone.current.set(event.jobId, event);
          }),
        () => onDownloadProgress((event) => dispatch({ type: "download_progress", event })),
      ]),
    [afterJobDone],
  );

  const plan = useCallback(async (target: InstallTarget, options: PlanOptions = {}) => {
    dispatch({ type: "plan_start", target });
    try {
      const result = await planInstall(target, options.excludeRegistry ?? null);
      dispatch({ type: "plan_ok", target, plan: result });
    } catch (e) {
      dispatch({ type: "plan_failed", target, error: toWireError(e) });
    }
  }, []);

  const run = useCallback(
    async (target: InstallTarget, installPlan: InstallPlan) => {
      dispatch({ type: "run_start", target });
      try {
        const job = await startInstall(installPlan);
        jobTargets.current.set(job.jobId, target);
        dispatch({ type: "run_ok", target, job });
        const early = earlyDone.current.get(job.jobId);
        if (early) {
          earlyDone.current.delete(job.jobId);
          afterJobDone(target, early);
        }
      } catch (e) {
        dispatch({ type: "run_failed", target, error: toWireError(e) });
      }
    },
    [afterJobDone],
  );

  const cancel = useCallback(async (target: InstallTarget, jobId: string) => {
    dispatch({ type: "cancel_requested", target });
    // A rejection means the job already finished; `install://done` settles the item either way.
    await cancelInstall(jobId).catch(() => undefined);
  }, []);

  const fetchRelease = useCallback(async (target: InstallTarget) => {
    dispatch({ type: "release_start", target });
    try {
      const release = await fetchInstallerRelease(target);
      dispatch({ type: "release_ok", target, release });
    } catch (e) {
      dispatch({ type: "release_failed", target, error: toWireError(e) });
    }
  }, []);

  const planRun = useCallback(async (target: InstallTarget, path: string) => {
    dispatch({ type: "run_plan_start", target });
    try {
      const result = await planInstallerRun(target, path);
      dispatch({ type: "run_plan_ok", target, plan: result });
    } catch (e) {
      dispatch({ type: "run_plan_failed", target, error: toWireError(e) });
    }
  }, []);

  const download = useCallback(
    async (target: InstallTarget, release: InstallerRelease) => {
      const jobId = newJobId();
      const startedAt = Date.now();
      dispatch({ type: "download_start", target, jobId });
      try {
        const result = await downloadFile({
          jobId,
          url: release.downloadUrl,
          fileName: release.assetName,
          expectedSha256: release.sha256,
        });
        dispatch({ type: "download_ok", target, download: result });
        track(
          installTelemetryEvent(
            result.verified === false ? "fail" : "pass",
            Date.now() - startedAt,
          ),
        );
        // Show the user what running the installer means right away (never runs by itself).
        if (result.verified !== false && runsInstaller(target)) await planRun(target, result.path);
      } catch (e) {
        dispatch({ type: "download_failed", target, error: toWireError(e) });
        track(installTelemetryEvent("fail", Date.now() - startedAt));
      }
    },
    [planRun],
  );

  const skip = useCallback(
    (target: InstallTarget) => {
      dispatch({ type: "skip", target });
      storeSkip(target);
    },
    [storeSkip],
  );

  const unskip = useCallback(
    (target: InstallTarget) => {
      dispatch({ type: "unskip", target });
      storeUnskip(target);
    },
    [storeUnskip],
  );

  const actions = useMemo<InstallActions>(
    () => ({ plan, run, cancel, fetchRelease, download, planRun, recheck, skip, unskip }),
    [plan, run, cancel, fetchRelease, download, planRun, recheck, skip, unskip],
  );

  return { state, actions };
}
