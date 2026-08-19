import { Play, RefreshCw, Square } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Alert, Button, ErrorBanner } from "@/components/ui";
import type { AppConfig, Platform } from "@/lib/types";

import { OutputPane, PhaseSpinner, RecheckFeedback } from "./ItemParts";
import { jobFailureMessage } from "./item-text";
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
            onRerun={() => void actions.recheck(target)}
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
              title={jobFailureMessage(t, step.done)}
              onRetry={retry}
              retryLabel={retryLabel}
            />
          ) : (
            <Alert
              variant="danger"
              title={jobFailureMessage(t, step.done)}
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
