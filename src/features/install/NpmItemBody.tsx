import { ChevronDown, ChevronUp, Play, RefreshCw, Square } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";

import { Alert, Button, ErrorBanner, LogView } from "@/components/ui";
import type { AppConfig, InstallDoneEvent, Platform } from "@/lib/types";

import { PhaseSpinner, RecheckFeedback } from "./ItemParts";
import type { InstallActions } from "./useInstallController";
import type { ItemState, JobLog } from "./install-state";
import { registryToAvoidOnRetry } from "./install-targets";
import { PlanDetails } from "./PlanDetails";

export interface NpmItemBodyProps {
  item: ItemState;
  log: JobLog | null;
  toolName: string;
  platform: Platform | null;
  config: AppConfig | null;
  actions: InstallActions;
}

/** Collapsible output pane shown once a job has produced (or finished producing) output. */
function OutputPane({ log, defaultOpen }: { log: JobLog | null; defaultOpen: boolean }) {
  const { t } = useTranslation();
  const id = useId();
  const [open, setOpen] = useState(defaultOpen);
  const lines = log?.lines ?? [];
  if (defaultOpen) {
    return (
      <div className="mt-3 space-y-1">
        <p className="text-xs font-medium text-neutral-500">{t("install:npm.outputTitle")}</p>
        <LogView lines={lines} />
      </div>
    );
  }
  return (
    <div className="mt-3">
      <Button
        variant="ghost"
        size="sm"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        aria-controls={id}
        className="-ml-3"
        leftIcon={
          open ? (
            <ChevronUp className="size-4" aria-hidden />
          ) : (
            <ChevronDown className="size-4" aria-hidden />
          )
        }
      >
        {open ? t("install:actions.hideOutput") : t("install:actions.showOutput")}
      </Button>
      <div id={id} hidden={!open}>
        <LogView lines={lines} className="mt-1" />
      </div>
    </div>
  );
}

function failureMessage(
  t: (key: string, opts?: Record<string, unknown>) => string,
  done: InstallDoneEvent | null,
): string {
  if (!done) return t("install:npm.startFailed");
  if (done.cancelled) return t("install:npm.cancelled");
  if (done.exitCode === null) return t("install:npm.failedNoCode");
  return t("install:npm.failedHint", { code: done.exitCode });
}

/**
 * Codex / Claude Code via `npm install -g`: plan → confirm (command shown) → live output →
 * done (auto re-check) | failed. Retry re-plans; when the job itself failed and another npm
 * registry is configured, the registry it failed on is excluded so the new plan switches
 * mirror ("Switch mirror and retry"), otherwise it is a plain retry.
 */
export function NpmItemBody({ item, log, toolName, platform, config, actions }: NpmItemBodyProps) {
  const { t } = useTranslation();
  const { target, step } = item;

  switch (step.phase) {
    case "idle":
    case "skipped":
      return (
        <Button size="sm" onClick={() => void actions.plan(target)}>
          {t("install:actions.prepare")}
        </Button>
      );
    case "planning":
      return <PhaseSpinner labelKey="planning" />;
    case "confirm":
    case "starting":
      return (
        <div className="space-y-3">
          <PlanDetails plan={step.plan} toolName={toolName} platform={platform} />
          <Button
            onClick={() => void actions.run(target, step.plan)}
            loading={step.phase === "starting"}
            leftIcon={<Play className="size-4" aria-hidden />}
          >
            {t("install:actions.run")}
          </Button>
        </div>
      );
    case "running":
      return (
        <div className="space-y-3">
          <PlanDetails plan={step.plan} toolName={toolName} platform={platform} />
          <OutputPane log={log} defaultOpen />
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void actions.cancel(target, step.jobId)}
            disabled={step.cancelling}
            loading={step.cancelling}
            leftIcon={<Square className="size-4" aria-hidden />}
          >
            {t("install:actions.cancel")}
          </Button>
        </div>
      );
    case "done":
      return (
        <div className="space-y-3">
          {item.recheck.status === "idle" || item.recheck.status === "running" ? (
            <Alert variant="success">{t("install:npm.successHint")}</Alert>
          ) : null}
          <RecheckFeedback
            recheck={item.recheck}
            passedKey="install:npm.recheckAfterDone"
            notPassedKey="install:npm.recheckStillMissing"
          />
          {log && log.lines.length > 0 && <OutputPane log={log} defaultOpen={false} />}
        </div>
      );
    case "failed": {
      const avoidRegistry = step.stage === "run" ? registryToAvoidOnRetry(step.plan, config) : null;
      const retryLabel = avoidRegistry
        ? t("install:actions.changeMirror")
        : t("install:actions.retry");
      const retry = () => void actions.plan(target, { excludeRegistry: avoidRegistry });
      return (
        <div className="space-y-3">
          {step.plan && step.stage !== "plan" && (
            <PlanDetails plan={step.plan} toolName={toolName} platform={platform} />
          )}
          {step.error ? (
            <ErrorBanner
              error={step.error}
              title={failureMessage(t, step.done)}
              onRetry={retry}
              retryLabel={retryLabel}
            />
          ) : (
            <Alert
              variant="danger"
              title={failureMessage(t, step.done)}
              actions={
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={retry}
                  leftIcon={<RefreshCw className="size-4" aria-hidden />}
                >
                  {retryLabel}
                </Button>
              }
            />
          )}
          {log && log.lines.length > 0 && <OutputPane log={log} defaultOpen />}
        </div>
      );
    }
    default:
      // Release/download phases never occur for npm targets.
      return (
        <Button
          size="sm"
          variant="secondary"
          onClick={() => void actions.plan(target)}
          leftIcon={<RefreshCw className="size-4" aria-hidden />}
        >
          {t("install:actions.retry")}
        </Button>
      );
  }
}
