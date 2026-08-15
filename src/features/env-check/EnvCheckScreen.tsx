import { RefreshCw } from "lucide-react";
import { useCallback, useEffect, useMemo, useReducer, useRef } from "react";
import { useTranslation } from "react-i18next";

import {
  Alert,
  Card,
  ErrorBanner,
  KeyValueList,
  Spinner,
  StepFooter,
  Button,
  type AlertVariant,
} from "@/components/ui";
import { onCheckProgress } from "@/lib/events";
import { toWireError } from "@/lib/errors";
import { runEnvCheck, runEnvChecks, trackEvent } from "@/lib/tauri";
import type { CheckId, CheckStatus, EnvSnapshot, InstallTarget } from "@/lib/types";
import { useInstallStore } from "@/stores/install";
import { useWizardStore } from "@/stores/wizard";

import { CheckRow } from "./CheckRow";
import {
  envCheckReducer,
  initialEnvCheckState,
  nextGate,
  overallOf,
  rowStatus,
  summarize,
  visibleCheckIds,
  type CheckSummary,
} from "./env-check-state";
import { EnvVarsPanel } from "./EnvVarsPanel";
import { applyResultToSnapshot } from "./snapshot-utils";

const noop = () => undefined;

function summaryVariant(overall: CheckStatus, running: boolean): AlertVariant {
  if (running) return "info";
  switch (overall) {
    case "pass":
      return "success";
    case "warn":
      return "warning";
    case "fail":
      return "danger";
    case "skipped":
      return "info";
  }
}

function SummaryAlert({ summary, running }: { summary: CheckSummary; running: boolean }) {
  const { t } = useTranslation();
  const key = running ? "running" : summary.overall;
  return (
    <Alert
      variant={summaryVariant(summary.overall, running)}
      hideIcon={running}
      data-testid="env-summary"
      role="status"
    >
      <span className="inline-flex items-center gap-2">
        {running && <Spinner size="sm" decorative />}
        {t(`checks:summary.${key}`, { ...summary })}
      </span>
    </Alert>
  );
}

/** RFC 3339 timestamp → local date/time in the current UI language; raw string if unparsable. */
function formatTimestamp(iso: string, lang: string): string {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleString(lang);
}

function SystemFacts({ snapshot }: { snapshot: EnvSnapshot }) {
  const { t, i18n } = useTranslation();
  const os = snapshot.os;
  return (
    <Card title={t("checks:screen.system")}>
      <KeyValueList
        columns={2}
        items={[
          { label: t("checks:screen.systemItems.platform"), value: os.platform, mono: true },
          { label: t("checks:screen.systemItems.version"), value: os.version, mono: true },
          { label: t("checks:screen.systemItems.arch"), value: os.arch, mono: true },
          { label: t("checks:screen.systemItems.shell"), value: os.shell || "—", mono: true },
          {
            label: t("checks:screen.systemItems.generatedAt"),
            value: formatTimestamp(snapshot.generatedAt, i18n.language),
          },
        ]}
      />
    </Card>
  );
}

/**
 * M1 — Environment check. Runs every check on mount (and on "Re-check everything"), streams
 * rows in as `checks://progress` events arrive, stores the final `EnvSnapshot` in the wizard
 * store and gates Next on the outcome (see `nextGate`).
 */
export function EnvCheckScreen() {
  const { t } = useTranslation();
  const selectedTools = useWizardStore((s) => s.selectedTools);
  const snapshot = useWizardStore((s) => s.snapshot);
  const setSnapshot = useWizardStore((s) => s.setSnapshot);
  const goTo = useWizardStore((s) => s.goTo);
  const next = useWizardStore((s) => s.next);
  const back = useWizardStore((s) => s.back);
  const requestInstall = useInstallStore((s) => s.request);

  const [state, dispatch] = useReducer(envCheckReducer, initialEnvCheckState);
  const ids = useMemo(() => visibleCheckIds(selectedTools), [selectedTools]);
  const summary = summarize(state, ids);
  const gate = nextGate(state, ids);
  const running = state.phase === "running";

  // A full run that finishes after a newer one started (or after unmount) must not win.
  const runIdRef = useRef(0);
  useEffect(
    () => () => {
      runIdRef.current += 1;
    },
    [],
  );

  const runAll = useCallback(async () => {
    const runId = ++runIdRef.current;
    const started = performance.now();
    dispatch({ type: "start_all" });
    try {
      const snap = await runEnvChecks();
      if (runIdRef.current !== runId) return;
      dispatch({ type: "all_done", snapshot: snap });
      setSnapshot(snap);
      const visible = snap.checks.filter((c) => ids.includes(c.id));
      trackEvent({
        name: "step_result",
        step: "env_check",
        status: overallOf(visible),
        durationMs: Math.round(performance.now() - started),
        errorClass: null,
        ruleId: null,
      }).catch(noop);
    } catch (e) {
      if (runIdRef.current !== runId) return;
      dispatch({ type: "all_failed", error: toWireError(e) });
    }
  }, [ids, setSnapshot]);

  // Subscribe to streamed results first, then start the run, so no early row is missed.
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    const boot = async () => {
      try {
        const fn = await onCheckProgress((result) => dispatch({ type: "result", result }));
        if (disposed) {
          fn();
          return;
        }
        unlisten = fn;
      } catch {
        // Without the event channel we still get the final snapshot from `runEnvChecks`.
      }
      if (!disposed) await runAll();
    };
    void boot();
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [runAll]);

  const rerun = useCallback(
    async (id: CheckId) => {
      dispatch({ type: "rerun_start", id });
      try {
        const result = await runEnvCheck(id);
        dispatch({ type: "rerun_done", result });
        const current = useWizardStore.getState().snapshot;
        setSnapshot(applyResultToSnapshot(current, result));
      } catch (e) {
        dispatch({ type: "rerun_failed", id, error: toWireError(e) });
      }
    },
    [setSnapshot],
  );

  const onInstall = useCallback(
    (target: InstallTarget) => {
      requestInstall(target);
      goTo("install");
    },
    [goTo, requestInstall],
  );

  return (
    <div className="mx-auto max-w-3xl space-y-5">
      <div>
        <h1 className="text-2xl font-semibold">{t("checks:screen.title")}</h1>
        <p className="mt-2 text-sm leading-6 text-neutral-600 dark:text-neutral-300">
          {t("checks:screen.intro")}
        </p>
      </div>

      {state.phase === "error" ? (
        <ErrorBanner
          error={state.error}
          title={t("checks:screen.runFailed")}
          onRetry={() => void runAll()}
        />
      ) : (
        <SummaryAlert summary={summary} running={running} />
      )}

      <Card>
        <ul className="-my-4" aria-busy={running} aria-label={t("checks:screen.title")}>
          {ids.map((id) => (
            <CheckRow
              key={id}
              id={id}
              status={rowStatus(state, id)}
              result={state.results[id] ?? null}
              error={state.rowErrors[id]}
              onRerun={(checkId) => void rerun(checkId)}
              onInstall={onInstall}
            />
          ))}
        </ul>
      </Card>

      {snapshot && <EnvVarsPanel findings={snapshot.envVars} />}
      {snapshot && <SystemFacts snapshot={snapshot} />}

      {gate.offerContinueAnyway && (
        <Alert variant="warning">
          <label className="inline-flex cursor-pointer items-start gap-2">
            <input
              type="checkbox"
              className="accent-brand-600 mt-1 size-4"
              checked={state.acknowledged}
              onChange={(e) => dispatch({ type: "acknowledge", value: e.target.checked })}
            />
            <span>{t("checks:summary.continueAnyway")}</span>
          </label>
          <p className="mt-1 text-xs text-neutral-600 dark:text-neutral-400">
            {t("checks:summary.continueAnywayHint")}
          </p>
        </Alert>
      )}

      <StepFooter
        onBack={back}
        onNext={next}
        nextDisabled={!gate.canProceed}
        nextLoading={running}
        extra={
          <Button
            variant="secondary"
            onClick={() => void runAll()}
            disabled={running}
            leftIcon={<RefreshCw className="size-4" aria-hidden />}
          >
            {t("checks:screen.recheckAll")}
          </Button>
        }
      />
    </div>
  );
}
