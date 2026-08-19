import { ChevronDown, ChevronUp, RefreshCw } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";

import { Alert, Button, ErrorBanner } from "@/components/ui";
import { HelpLink } from "@/features/help/HelpLink";
import type { AppConfig, InstallerRelease, Platform } from "@/lib/types";

import {
  DownloadButton,
  DownloadedAlert,
  DownloadProgressView,
  JobFailureAlert,
  ONE_CLICK_HELP_SECTION,
  OpenDownloadedFileButton,
  OpenPageButton,
  OutputPane,
  PhaseSpinner,
  RecheckButton,
  RecheckFeedback,
  ReleaseCard,
  RunInstallerPanel,
  RunningInstallerView,
} from "./ItemParts";
import { jobFailureMessage, releaseSourceLabel } from "./item-text";
import type { InstallActions } from "./useInstallController";
import { releaseOf, type DownloadProgress, type ItemState, type JobLog } from "./install-state";
import { nodeDownloadPageFor } from "./install-targets";
import { PlanDetails } from "./PlanDetails";

export interface NodeItemBodyProps {
  item: ItemState;
  progress: DownloadProgress | null;
  log: JobLog | null;
  toolName: string;
  platform: Platform | null;
  config: AppConfig | null;
  actions: InstallActions;
}

const STEP_KEYS = ["open", "run", "close", "recheck"] as const;

interface ManualInstallProps {
  url: string;
  onRecheck: () => void;
  disabled?: boolean;
  /** Expanded by default (when the one-click path is unavailable). */
  defaultOpen?: boolean;
}

/** Collapsible "install it yourself" alternative: download page, steps, re-check. */
function ManualInstall({ url, onRecheck, disabled, defaultOpen = false }: ManualInstallProps) {
  const { t } = useTranslation();
  const id = useId();
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="border-t border-neutral-200 pt-2 dark:border-neutral-800">
      <Button
        variant="ghost"
        size="sm"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        aria-controls={id}
        className="-ml-3"
        data-testid="manual-install-toggle"
        leftIcon={
          open ? (
            <ChevronUp className="size-4" aria-hidden />
          ) : (
            <ChevronDown className="size-4" aria-hidden />
          )
        }
      >
        {t("install:node.manual.title")}
      </Button>
      <div id={id} hidden={!open} className="mt-2 space-y-3">
        <p className="text-sm font-medium">{t("install:node.steps.title")}</p>
        <ol className="list-decimal space-y-1 pl-5 text-sm text-neutral-700 dark:text-neutral-300">
          {STEP_KEYS.map((k) => (
            <li key={k}>{t(`install:node.steps.${k}`)}</li>
          ))}
        </ol>
        <p className="text-xs text-neutral-500" data-selectable>
          {t("install:node.downloadPage", { url })}
        </p>
        <div className="flex flex-wrap items-center gap-2">
          <OpenPageButton
            url={url}
            label={t("install:actions.openDownloadPage")}
            disabled={disabled}
          />
          <RecheckButton onClick={onRecheck} disabled={disabled} />
        </div>
      </div>
    </div>
  );
}

function ReleaseIntro({ release }: { release: InstallerRelease }) {
  const { t } = useTranslation();
  return (
    <div className="space-y-3">
      <p className="text-sm text-neutral-700 dark:text-neutral-300">
        {t("install:node.oneClick.intro")}
      </p>
      <ReleaseCard release={release} />
      <p className="text-xs text-neutral-500" data-testid="node-source">
        {t("install:node.oneClick.sourceChosen", { source: releaseSourceLabel(t, release.source) })}
      </p>
    </div>
  );
}

/**
 * Node.js one-click install: fetch the LTS installer from the dist mirror the network probe
 * picked → download (SHA-256 verified) → show the exact installer command → "Install now
 * (needs administrator rights)" → live output → done → automatic re-check. The manual path
 * (download page + steps + re-check) stays available as a collapsible alternative.
 */
export function NodeItemBody({
  item,
  progress,
  log,
  toolName,
  platform,
  config,
  actions,
}: NodeItemBodyProps) {
  const { t } = useTranslation();
  const { target, step, recheck } = item;
  const rechecking = recheck.status === "running";
  const release = releaseOf(step);
  const manualUrl = nodeDownloadPageFor(release?.source ?? null, config);
  const recheckNow = () => void actions.recheck(target);
  const feedback = (
    <RecheckFeedback
      recheck={recheck}
      passedKey="install:node.recheckPassed"
      notPassedKey="install:node.recheckNotPassed"
      onRerun={recheckNow}
    />
  );
  const manual = (defaultOpen = false) => (
    <ManualInstall
      url={manualUrl}
      onRecheck={recheckNow}
      disabled={rechecking}
      defaultOpen={defaultOpen}
    />
  );

  switch (step.phase) {
    case "idle":
    case "skipped":
      return (
        <div className="space-y-3">
          <div className="flex flex-wrap items-center gap-3">
            <Button size="sm" onClick={() => void actions.fetchRelease(target)}>
              {t("install:actions.prepareOneClick")}
            </Button>
            <HelpLink sectionId={ONE_CLICK_HELP_SECTION}>
              {t("install:oneClick.whatHappens")}
            </HelpLink>
          </div>
          {manual()}
        </div>
      );
    case "fetching_release":
      return <PhaseSpinner labelKey="fetching_release" />;
    case "release":
      return (
        <div className="space-y-3">
          <ReleaseIntro release={step.release} />
          {step.release.requiresAdmin && (
            <p className="text-sm text-neutral-700 dark:text-neutral-300">
              {t("install:node.oneClick.adminAhead")}
            </p>
          )}
          <div className="flex flex-wrap items-center gap-3">
            <DownloadButton onClick={() => void actions.download(target, step.release)} />
            <HelpLink sectionId={ONE_CLICK_HELP_SECTION}>
              {t("install:oneClick.whatHappens")}
            </HelpLink>
          </div>
          {manual()}
          {feedback}
        </div>
      );
    case "downloading":
      return <DownloadProgressView release={step.release} progress={progress} />;
    case "run_planning":
      return (
        <div className="space-y-3">
          <DownloadedAlert download={step.download} />
          <PhaseSpinner labelKey="run_planning" />
        </div>
      );
    case "downloaded": {
      const { download, runPlan } = step;
      const runnable = download.verified !== false;
      return (
        <div className="space-y-3">
          <DownloadedAlert download={download} />
          {runnable && runPlan && (
            <RunInstallerPanel
              plan={runPlan}
              toolName={toolName}
              platform={platform}
              onRun={() => void actions.run(target, runPlan)}
              disabled={rechecking}
            />
          )}
          <div className="flex flex-wrap items-center gap-2">
            {runnable && !runPlan && (
              <Button
                size="sm"
                onClick={() => void actions.planRun(target, download.path)}
                disabled={rechecking}
              >
                {t("install:actions.prepareRun")}
              </Button>
            )}
            {runnable && (
              <OpenDownloadedFileButton
                path={download.path}
                label={t("install:actions.openInstaller")}
                variant="secondary"
                disabled={rechecking}
              />
            )}
            {!runnable && (
              <Button
                size="sm"
                onClick={() => void actions.download(target, step.release)}
                leftIcon={<RefreshCw className="size-4" aria-hidden />}
              >
                {t("install:actions.redownload")}
              </Button>
            )}
            <RecheckButton onClick={recheckNow} disabled={rechecking} />
          </div>
          {manual()}
          {feedback}
        </div>
      );
    }
    case "starting":
      return (
        <RunInstallerPanel
          plan={step.plan}
          toolName={toolName}
          platform={platform}
          onRun={() => undefined}
          starting
        />
      );
    case "running":
      return (
        <RunningInstallerView
          plan={step.plan}
          toolName={toolName}
          platform={platform}
          log={log}
          cancelling={step.cancelling}
          onCancel={() => void actions.cancel(target, step.jobId)}
        />
      );
    case "done":
      return (
        <div className="space-y-3">
          {step.jobId && (recheck.status === "idle" || recheck.status === "running") ? (
            <Alert variant="success">{t("install:installer.successHint")}</Alert>
          ) : null}
          {feedback}
          {log && log.lines.length > 0 && <OutputPane log={log} defaultOpen={false} />}
        </div>
      );
    case "failed": {
      const retryFetch = () => void actions.fetchRelease(target);
      switch (step.stage) {
        case "run_plan": {
          const download = step.download;
          return (
            <div className="space-y-3">
              {download && <DownloadedAlert download={download} />}
              <ErrorBanner
                error={step.error}
                title={t("install:installer.planFailed")}
                onRetry={download ? () => void actions.planRun(target, download.path) : retryFetch}
              />
              <div className="flex flex-wrap items-center gap-2">
                {download && download.verified !== false && (
                  <OpenDownloadedFileButton
                    path={download.path}
                    label={t("install:actions.openInstaller")}
                    disabled={rechecking}
                  />
                )}
                <RecheckButton onClick={recheckNow} disabled={rechecking} />
              </div>
              {manual(true)}
              {feedback}
            </div>
          );
        }
        case "start":
        case "run": {
          const plan = step.plan;
          const retry = plan ? () => void actions.run(target, plan) : retryFetch;
          return (
            <div className="space-y-3">
              {plan && <PlanDetails plan={plan} toolName={toolName} platform={platform} />}
              <JobFailureAlert
                error={step.error}
                title={jobFailureMessage(t, step.done, "installer")}
                onRetry={retry}
                retryLabel={plan ? t("install:actions.installNowAgain") : undefined}
              />
              {log && log.lines.length > 0 && <OutputPane log={log} defaultOpen />}
              <div className="flex flex-wrap items-center gap-2">
                {plan?.installerPath && (
                  <OpenDownloadedFileButton
                    path={plan.installerPath}
                    label={t("install:actions.openInstaller")}
                    variant="secondary"
                    disabled={rechecking}
                  />
                )}
                <RecheckButton onClick={recheckNow} disabled={rechecking} />
              </div>
              {manual(true)}
              {feedback}
            </div>
          );
        }
        case "download": {
          const failedRelease = step.release;
          return (
            <div className="space-y-3">
              <ErrorBanner
                error={step.error}
                title={t("install:installer.downloadFailed")}
                onRetry={
                  failedRelease ? () => void actions.download(target, failedRelease) : retryFetch
                }
              />
              {manual(true)}
              {feedback}
            </div>
          );
        }
        default:
          // release (or a stale plan failure): the manual path still works.
          return (
            <div className="space-y-3">
              <ErrorBanner
                error={step.error}
                title={t("install:node.oneClick.fetchFailed")}
                onRetry={retryFetch}
              />
              {manual(true)}
              {feedback}
            </div>
          );
      }
    }
    default:
      return (
        <Button size="sm" variant="secondary" onClick={() => void actions.fetchRelease(target)}>
          {t("install:actions.retry")}
        </Button>
      );
  }
}
