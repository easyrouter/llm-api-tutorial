import { Download, FolderOpen, ShieldAlert, ShieldCheck, ShieldOff, ShieldX } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Alert, Button, ErrorBanner, KeyValueList, ProgressBar } from "@/components/ui";
import { DiagnosePanel } from "@/features/diagnose/DiagnosePanel";
import { useAsync } from "@/hooks";
import { formatBytes } from "@/lib/format";
import { diagnose, openDownloadedFile } from "@/lib/tauri";
import type { AppConfig, Diagnosis, DownloadResult, Platform } from "@/lib/types";
import { useWizardStore } from "@/stores/wizard";

import { OpenPageButton, PhaseSpinner, RecheckButton, RecheckFeedback } from "./ItemParts";
import type { InstallActions } from "./useInstallController";
import type { DownloadProgress, ItemState } from "./install-state";
import { ccSwitchDownloadPage } from "./install-targets";

export interface CcSwitchItemBodyProps {
  item: ItemState;
  progress: DownloadProgress | null;
  platform: Platform | null;
  config: AppConfig | null;
  actions: InstallActions;
}

function VerificationBadge({ download }: { download: DownloadResult }) {
  const { t } = useTranslation();
  if (download.verified === true) {
    return (
      <span
        className="text-success-500 inline-flex items-center gap-1 text-sm font-medium"
        data-testid="verified-badge"
      >
        <ShieldCheck className="size-4" aria-hidden />
        {t("install:ccSwitch.verified")}
      </span>
    );
  }
  if (download.verified === false) {
    return (
      <span className="text-danger-500 inline-flex items-center gap-1 text-sm font-medium">
        <ShieldX className="size-4" aria-hidden />
        {t("install:ccSwitch.verifiedFalse")}
      </span>
    );
  }
  return (
    <span className="text-warning-500 inline-flex items-center gap-1 text-sm font-medium">
      <ShieldOff className="size-4" aria-hidden />
      {t("install:ccSwitch.unverified")}
    </span>
  );
}

/** App name handed to the rule engine for guide fault G (shown verbatim in the diagnosis). */
export const CC_SWITCH_APP_NAME = "CC Switch";

/**
 * "The installer was blocked or does not open" — runs the rule engine with the
 * `app_blocked_by_os` symptom (guide fault G: SmartScreen / Gatekeeper) and shows the resulting
 * checklist inline, so the fault is reachable from the wizard and not only from the docs.
 */
function BlockedInstallerHelp({ disabled }: { disabled?: boolean }) {
  const { t } = useTranslation();
  const snapshot = useWizardStore((s) => s.snapshot);
  const run = useAsync(diagnose);
  const diagnoses: Diagnosis[] | null = run.data;
  const ask = () =>
    void run.run({
      symptoms: [{ kind: "app_blocked_by_os", app: CC_SWITCH_APP_NAME }],
      snapshot,
    });
  return (
    <>
      <Button
        variant="secondary"
        size="sm"
        onClick={ask}
        disabled={disabled}
        loading={run.loading}
        leftIcon={<ShieldAlert className="size-4" aria-hidden />}
        data-testid="installer-blocked"
      >
        {t("install:actions.installerBlocked")}
      </Button>
      {run.error && <ErrorBanner error={run.error} className="basis-full" onRetry={ask} />}
      {diagnoses && (
        <DiagnosePanel
          diagnoses={diagnoses}
          snapshot={snapshot}
          note={t("install:ccSwitch.blockedNote")}
          className="basis-full"
        />
      )}
    </>
  );
}

function OpenInstallerButton({ path, disabled }: { path: string; disabled?: boolean }) {
  const { t } = useTranslation();
  const [error, setError] = useState<unknown>(null);
  const open = () => {
    setError(null);
    openDownloadedFile(path).catch((e: unknown) => setError(e));
  };
  return (
    <>
      <Button
        size="sm"
        onClick={open}
        disabled={disabled}
        leftIcon={<FolderOpen className="size-4" aria-hidden />}
      >
        {t("install:actions.openInstaller")}
      </Button>
      {error !== null && <ErrorBanner error={error} className="basis-full" />}
    </>
  );
}

/**
 * CC Switch: fetch the latest release → show version / asset / hash availability → download
 * (SHA-256 verified when the release ships a hash) → hand the installer to the user → done
 * once a re-check passes. Nothing is executed on the user's behalf.
 */
export function CcSwitchItemBody({
  item,
  progress,
  platform,
  config,
  actions,
}: CcSwitchItemBodyProps) {
  const { t } = useTranslation();
  const { target, step, recheck } = item;
  const rechecking = recheck.status === "running";
  const releasePage = ccSwitchDownloadPage(config);
  const feedback = (
    <RecheckFeedback
      recheck={recheck}
      passedKey="install:ccSwitch.recheckPassed"
      notPassedKey="install:ccSwitch.recheckNotPassed"
    />
  );

  switch (step.phase) {
    case "idle":
    case "skipped":
      return (
        <Button size="sm" onClick={() => void actions.fetchRelease(target)}>
          {t("install:actions.prepare")}
        </Button>
      );
    case "fetching_release":
      return <PhaseSpinner labelKey="fetching_release" />;
    case "release": {
      const { release } = step;
      return (
        <div className="space-y-3">
          <p className="text-sm text-neutral-700 dark:text-neutral-300">
            {t("install:ccSwitch.intro")}
          </p>
          <KeyValueList
            items={[
              { label: t("install:ccSwitch.release.version"), value: release.version, mono: true },
              { label: t("install:ccSwitch.release.asset"), value: release.assetName, mono: true },
              { label: t("install:ccSwitch.release.source"), value: release.source, mono: true },
              {
                label: t("install:ccSwitch.release.hash"),
                value: release.sha256
                  ? t("install:ccSwitch.release.hashAvailable")
                  : t("install:ccSwitch.release.hashMissing"),
              },
            ]}
          />
          {!release.sha256 && (
            <Alert variant="warning">{t("install:ccSwitch.release.hashMissing")}</Alert>
          )}
          <div className="flex flex-wrap items-center gap-2">
            <Button
              size="sm"
              onClick={() => void actions.download(target, release)}
              leftIcon={<Download className="size-4" aria-hidden />}
            >
              {t("install:actions.download")}
            </Button>
            <OpenPageButton url={releasePage} label={t("install:actions.openReleasePage")} />
          </div>
        </div>
      );
    }
    case "downloading": {
      const downloaded = progress?.downloaded ?? 0;
      const total = progress?.total ?? null;
      const detail =
        total && total > 0
          ? t("install:download.sizeKnown", {
              downloaded: formatBytes(downloaded),
              total: formatBytes(total),
            })
          : t("install:download.sizeUnknown", { downloaded: formatBytes(downloaded) });
      return (
        <ProgressBar
          value={total && total > 0 ? downloaded / total : undefined}
          label={t("install:ccSwitch.downloading", { asset: step.release.assetName })}
          detail={detail}
        />
      );
    }
    case "downloaded": {
      const { download } = step;
      const runnable = download.verified !== false;
      return (
        <div className="space-y-3">
          <Alert
            variant={runnable ? "success" : "danger"}
            title={t("install:ccSwitch.downloadedTitle")}
          >
            <p data-selectable className="font-mono text-xs break-all">
              {t("install:ccSwitch.downloadedPath", { path: download.path })}
            </p>
            <p className="mt-1">
              <VerificationBadge download={download} />
            </p>
          </Alert>
          {runnable && (
            <div>
              <p className="text-sm font-medium">{t("install:ccSwitch.afterDownload.title")}</p>
              <ol className="mt-1 list-decimal space-y-1 pl-5 text-sm text-neutral-700 dark:text-neutral-300">
                <li>{t("install:ccSwitch.afterDownload.open")}</li>
                {platform === "macos" ? (
                  <li>{t("install:ccSwitch.afterDownload.macos")}</li>
                ) : (
                  <li>{t("install:ccSwitch.afterDownload.windows")}</li>
                )}
                <li>{t("install:ccSwitch.afterDownload.recheck")}</li>
              </ol>
            </div>
          )}
          <div className="flex flex-wrap items-center gap-2">
            {runnable && <OpenInstallerButton path={download.path} disabled={rechecking} />}
            <RecheckButton onClick={() => void actions.recheck(target)} disabled={rechecking} />
            <Button
              variant="secondary"
              size="sm"
              onClick={() => void actions.fetchRelease(target)}
              disabled={rechecking}
            >
              {t("install:actions.retry")}
            </Button>
            {runnable && <BlockedInstallerHelp disabled={rechecking} />}
          </div>
          {feedback}
        </div>
      );
    }
    case "done":
      return feedback;
    case "failed": {
      const failedRelease = step.stage === "download" ? step.release : null;
      const retry = failedRelease
        ? () => void actions.download(target, failedRelease)
        : () => void actions.fetchRelease(target);
      return (
        <div className="space-y-3">
          <ErrorBanner
            error={step.error}
            title={step.stage === "release" ? t("install:ccSwitch.fetchFailed") : undefined}
            onRetry={retry}
            actions={
              <OpenPageButton url={releasePage} label={t("install:actions.openReleasePage")} />
            }
          />
          <div className="flex flex-wrap items-center gap-2">
            <RecheckButton onClick={() => void actions.recheck(target)} disabled={rechecking} />
          </div>
          {feedback}
        </div>
      );
    }
    default:
      return (
        <Button size="sm" variant="secondary" onClick={() => void actions.fetchRelease(target)}>
          {t("install:actions.retry")}
        </Button>
      );
  }
}
