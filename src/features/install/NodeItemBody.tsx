import { useTranslation } from "react-i18next";

import { Alert, Button, ErrorBanner } from "@/components/ui";
import type { AppConfig, Platform } from "@/lib/types";

import { OpenPageButton, PhaseSpinner, RecheckButton, RecheckFeedback } from "./ItemParts";
import type { InstallActions } from "./useInstallController";
import type { ItemState } from "./install-state";
import { nodeDownloadPage } from "./install-targets";
import { PlanDetails } from "./PlanDetails";

export interface NodeItemBodyProps {
  item: ItemState;
  toolName: string;
  platform: Platform | null;
  config: AppConfig | null;
  actions: InstallActions;
}

const STEP_KEYS = ["open", "run", "close", "recheck"] as const;

function ManualSteps() {
  const { t } = useTranslation();
  return (
    <div>
      <p className="text-sm font-medium">{t("install:node.steps.title")}</p>
      <ol className="mt-1 list-decimal space-y-1 pl-5 text-sm text-neutral-700 dark:text-neutral-300">
        {STEP_KEYS.map((k) => (
          <li key={k}>{t(`install:node.steps.${k}`)}</li>
        ))}
      </ol>
    </div>
  );
}

/**
 * Node.js is installed from the official installer (no command is run for the user): the plan
 * only carries the download page of the chosen mirror. The item is done once a re-check passes.
 */
export function NodeItemBody({ item, toolName, platform, config, actions }: NodeItemBodyProps) {
  const { t } = useTranslation();
  const { target, step, recheck } = item;
  const rechecking = recheck.status === "running";

  const manualControls = (url: string) => (
    <div className="flex flex-wrap items-center gap-2">
      <OpenPageButton
        url={url}
        label={t("install:actions.openDownloadPage")}
        disabled={rechecking}
      />
      <RecheckButton onClick={() => void actions.recheck(target)} disabled={rechecking} />
    </div>
  );

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
    case "confirm": {
      const url = nodeDownloadPage(step.plan, config);
      return (
        <div className="space-y-3">
          <PlanDetails plan={step.plan} toolName={toolName} platform={platform} />
          <ManualSteps />
          <p className="text-xs text-neutral-500" data-selectable>
            {t("install:node.downloadPage", { url })}
          </p>
          {manualControls(url)}
          <RecheckFeedback
            recheck={recheck}
            passedKey="install:node.recheckPassed"
            notPassedKey="install:node.recheckNotPassed"
          />
        </div>
      );
    }
    case "done":
      return (
        <RecheckFeedback
          recheck={recheck}
          passedKey="install:node.recheckPassed"
          notPassedKey="install:node.recheckNotPassed"
        />
      );
    case "failed": {
      // Planning failed (e.g. mirror probe error): the manual path still works.
      const url = nodeDownloadPage(null, config);
      return (
        <div className="space-y-3">
          <ErrorBanner error={step.error} onRetry={() => void actions.plan(target)} />
          <ManualSteps />
          {manualControls(url)}
          <RecheckFeedback
            recheck={recheck}
            passedKey="install:node.recheckPassed"
            notPassedKey="install:node.recheckNotPassed"
          />
        </div>
      );
    }
    default:
      return (
        <Alert variant="info">
          <Button size="sm" variant="secondary" onClick={() => void actions.plan(target)}>
            {t("install:actions.retry")}
          </Button>
        </Alert>
      );
  }
}
