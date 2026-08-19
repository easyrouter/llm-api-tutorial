import { Download, Globe, RefreshCw, Store } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Alert, Button, ErrorBanner } from "@/components/ui";
import { HelpLink } from "@/features/help/HelpLink";
import { openSystemUri } from "@/lib/tauri";
import type { AppConfig, Platform, SystemUri } from "@/lib/types";

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
import { jobFailureMessage } from "./item-text";
import type { InstallActions } from "./useInstallController";
import type { DownloadProgress, ItemState, JobLog } from "./install-state";
import { codexAppDownloadPage } from "./install-targets";
import { PlanDetails } from "./PlanDetails";

export interface CodexAppItemBodyProps {
  item: ItemState;
  progress: DownloadProgress | null;
  log: JobLog | null;
  toolName: string;
  platform: Platform | null;
  config: AppConfig | null;
  actions: InstallActions;
}

interface SystemUriButtonProps {
  uri: SystemUri;
  label: string;
  icon: "store" | "region";
  variant?: "primary" | "secondary";
  disabled?: boolean;
}

/** Opens a well-known OS URI (Store page / region settings); failures are shown inline. */
function SystemUriButton({
  uri,
  label,
  icon,
  variant = "primary",
  disabled,
}: SystemUriButtonProps) {
  const [error, setError] = useState<unknown>(null);
  const open = () => {
    setError(null);
    openSystemUri(uri).catch((e: unknown) => setError(e));
  };
  const Icon = icon === "store" ? Store : Globe;
  return (
    <>
      <Button
        size="sm"
        variant={variant}
        onClick={open}
        disabled={disabled}
        data-testid={`system-uri-${uri}`}
        leftIcon={<Icon className="size-4" aria-hidden />}
      >
        {label}
      </Button>
      {error !== null && <ErrorBanner error={error} className="basis-full" />}
    </>
  );
}

/** "The Store says not available in your region" → open Windows region settings + steps. */
function RegionHint({ disabled }: { disabled?: boolean }) {
  const { t } = useTranslation();
  return (
    <Alert variant="info" title={t("install:codexApp.region.title")} data-testid="region-hint">
      <p>{t("install:codexApp.region.body")}</p>
      <ol className="mt-2 list-decimal space-y-1 pl-5">
        <li>{t("install:codexApp.region.step1")}</li>
        <li>{t("install:codexApp.region.step2")}</li>
        <li>{t("install:codexApp.region.step3")}</li>
      </ol>
      <div className="mt-3 flex flex-wrap items-center gap-2">
        <SystemUriButton
          uri="windows_region_settings"
          icon="region"
          variant="secondary"
          label={t("install:codexApp.actions.openRegionSettings")}
          disabled={disabled}
        />
      </div>
    </Alert>
  );
}

/**
 * Codex desktop client (part of the ChatGPT desktop app; optional but recommended).
 * Windows: Microsoft Store page, or the offline MSIX (download → `Add-AppxPackage`, no admin),
 * plus the region-settings hint. macOS: download the DMG → mount it and copy the app into
 * /Applications (no admin), or open the image by hand. Done once a re-check passes.
 */
export function CodexAppItemBody({
  item,
  progress,
  log,
  toolName,
  platform,
  config,
  actions,
}: CodexAppItemBodyProps) {
  const { t } = useTranslation();
  const { target, step, recheck } = item;
  const rechecking = recheck.status === "running";
  const windows = platform === "windows";
  const macos = platform === "macos";
  const downloadPage = codexAppDownloadPage(config);
  const recheckNow = () => void actions.recheck(target);
  const fetch = () => void actions.fetchRelease(target);
  const feedback = (
    <RecheckFeedback
      recheck={recheck}
      passedKey="install:codexApp.recheckPassed"
      notPassedKey="install:codexApp.recheckNotPassed"
      onRerun={recheckNow}
    />
  );
  const intro = (
    <p className="text-sm text-neutral-700 dark:text-neutral-300">{t("install:codexApp.intro")}</p>
  );
  const helpLink = (
    <HelpLink sectionId={ONE_CLICK_HELP_SECTION}>{t("install:oneClick.whatHappens")}</HelpLink>
  );
  const downloadLabel = windows
    ? t("install:codexApp.actions.downloadOffline")
    : t("install:codexApp.actions.downloadAndInstall");

  /** Platform-specific entry points (idle, and again next to errors). */
  const entryPoints = (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        {windows && (
          <SystemUriButton
            uri="ms_store_codex_app"
            icon="store"
            label={t("install:codexApp.actions.openStore")}
            disabled={rechecking}
          />
        )}
        {windows || macos ? (
          <Button
            size="sm"
            variant={windows ? "secondary" : "primary"}
            onClick={fetch}
            disabled={rechecking}
            data-testid="codex-app-download"
            leftIcon={<Download className="size-4" aria-hidden />}
          >
            {downloadLabel}
          </Button>
        ) : (
          <OpenPageButton
            url={downloadPage}
            label={t("install:actions.openDownloadPage")}
            disabled={rechecking}
          />
        )}
        <RecheckButton onClick={recheckNow} disabled={rechecking} />
        {helpLink}
      </div>
      {windows && <RegionHint disabled={rechecking} />}
    </div>
  );

  switch (step.phase) {
    case "idle":
    case "skipped":
      return (
        <div className="space-y-3">
          {intro}
          {entryPoints}
          {feedback}
        </div>
      );
    case "fetching_release":
      return <PhaseSpinner labelKey="fetching_release" />;
    case "release": {
      const { release } = step;
      return (
        <div className="space-y-3">
          {intro}
          <ReleaseCard
            release={release}
            extraItems={[
              {
                label: t("install:codexApp.release.size"),
                value: macos
                  ? t("install:codexApp.release.sizeDmg")
                  : t("install:codexApp.release.sizeMsix"),
              },
            ]}
          />
          <p className="text-sm text-neutral-700 dark:text-neutral-300">
            {macos
              ? t("install:codexApp.afterDownload.macos")
              : t("install:codexApp.afterDownload.windows")}
          </p>
          <div className="flex flex-wrap items-center gap-3">
            <DownloadButton
              onClick={() => void actions.download(target, release)}
              label={downloadLabel}
            />
            {helpLink}
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <RecheckButton onClick={recheckNow} disabled={rechecking} />
          </div>
          {feedback}
        </div>
      );
    }
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
                variant="secondary"
                label={
                  macos
                    ? t("install:codexApp.actions.openImage")
                    : t("install:codexApp.actions.openPackage")
                }
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
    case "failed":
      switch (step.stage) {
        case "run_plan": {
          const download = step.download;
          return (
            <div className="space-y-3">
              {download && <DownloadedAlert download={download} />}
              <ErrorBanner
                error={step.error}
                title={t("install:installer.planFailed")}
                onRetry={download ? () => void actions.planRun(target, download.path) : fetch}
              />
              <div className="flex flex-wrap items-center gap-2">
                {download && download.verified !== false && (
                  <OpenDownloadedFileButton
                    path={download.path}
                    label={
                      macos
                        ? t("install:codexApp.actions.openImage")
                        : t("install:codexApp.actions.openPackage")
                    }
                    disabled={rechecking}
                  />
                )}
                <RecheckButton onClick={recheckNow} disabled={rechecking} />
              </div>
              {feedback}
            </div>
          );
        }
        case "start":
        case "run": {
          const plan = step.plan;
          return (
            <div className="space-y-3">
              {plan && <PlanDetails plan={plan} toolName={toolName} platform={platform} />}
              <JobFailureAlert
                error={step.error}
                title={jobFailureMessage(t, step.done, "installer")}
                onRetry={plan ? () => void actions.run(target, plan) : fetch}
                retryLabel={plan ? t("install:actions.installNowAgain") : undefined}
              />
              {log && log.lines.length > 0 && <OutputPane log={log} defaultOpen />}
              {entryPoints}
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
                onRetry={failedRelease ? () => void actions.download(target, failedRelease) : fetch}
              />
              {entryPoints}
              {feedback}
            </div>
          );
        }
        default:
          return (
            <div className="space-y-3">
              <ErrorBanner
                error={step.error}
                title={t("install:codexApp.fetchFailed")}
                onRetry={fetch}
                actions={
                  <OpenPageButton
                    url={downloadPage}
                    label={t("install:actions.openDownloadPage")}
                  />
                }
              />
              {entryPoints}
              {feedback}
            </div>
          );
      }
    default:
      return (
        <Button size="sm" variant="secondary" onClick={fetch}>
          {t("install:actions.retry")}
        </Button>
      );
  }
}
