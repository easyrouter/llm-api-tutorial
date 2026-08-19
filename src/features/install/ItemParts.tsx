import {
  ChevronDown,
  ChevronUp,
  Download,
  ExternalLink as ExternalLinkIcon,
  FolderOpen,
  Play,
  RefreshCw,
  ShieldAlert,
  ShieldCheck,
  ShieldOff,
  ShieldX,
  Square,
} from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";

import {
  Alert,
  Button,
  ErrorBanner,
  FixActionButtons,
  KeyValueList,
  LogView,
  ProgressBar,
  Spinner,
} from "@/components/ui";
import { checkMessage } from "@/features/env-check/snapshot-utils";
import { HelpLink } from "@/features/help/HelpLink";
import { formatBytes } from "@/lib/format";
import { openDownloadedFile, openExternal } from "@/lib/tauri";
import type { DownloadResult, InstallPlan, InstallerRelease, Platform } from "@/lib/types";

import type { DownloadProgress, JobLog, RecheckState } from "./install-state";
import { releaseSourceLabel } from "./item-text";
import { PlanDetails } from "./PlanDetails";

/** Help section that explains what every one-click button does behind the scenes. */
export const ONE_CLICK_HELP_SECTION = "one-click";

/** Spinner + phase label, used while an IPC call is in flight. */
export function PhaseSpinner({ labelKey }: { labelKey: string }) {
  const { t } = useTranslation();
  return (
    <div className="flex items-center gap-2 text-sm text-neutral-600 dark:text-neutral-300">
      <Spinner size="sm" decorative />
      <span>{t(`install:state.${labelKey}`)}</span>
    </div>
  );
}

export interface RecheckFeedbackProps {
  recheck: RecheckState;
  /** Key under `install:` for a passing result (receives `message`). */
  passedKey: string;
  /** Key under `install:` for a non-passing result (receives `message`). */
  notPassedKey: string;
  /** Handler for the `rerun` fix of a non-passing result (usually the item's re-check). */
  onRerun?: () => void;
}

/**
 * Outcome of the "I installed it — re-check" action (or the automatic re-check after a job).
 * A non-passing result also renders the fixes Rust attached to it (PATH repair, instructions…)
 * — except `install`, which is what this screen already is.
 */
export function RecheckFeedback({
  recheck,
  passedKey,
  notPassedKey,
  onRerun,
}: RecheckFeedbackProps) {
  const { t } = useTranslation();
  switch (recheck.status) {
    case "idle":
      return null;
    case "running":
      return <PhaseSpinner labelKey="rechecking" />;
    case "failed":
      return <ErrorBanner error={recheck.error} title={t("install:recheck.failed")} />;
    case "done": {
      if (!recheck.result) return null;
      const message = checkMessage(t, recheck.result);
      const passed = recheck.result.status === "pass";
      const fixes = passed ? [] : recheck.result.fixes.filter((f) => f.kind !== "install");
      return (
        <Alert variant={passed ? "success" : "warning"} data-testid="recheck-feedback">
          <p>{t(passed ? passedKey : notPassedKey, { message })}</p>
          {fixes.length > 0 && (
            <FixActionButtons fixes={fixes} onRerun={onRerun} className="mt-2" />
          )}
        </Alert>
      );
    }
  }
}

export interface RecheckButtonProps {
  onClick: () => void;
  disabled?: boolean;
}

export function RecheckButton({ onClick, disabled }: RecheckButtonProps) {
  const { t } = useTranslation();
  return (
    <Button
      variant="primary"
      size="sm"
      onClick={onClick}
      disabled={disabled}
      leftIcon={<RefreshCw className="size-4" aria-hidden />}
    >
      {t("install:actions.installedRecheck")}
    </Button>
  );
}

export interface OpenPageButtonProps {
  url: string;
  label: string;
  disabled?: boolean;
}

/** Secondary button that opens `url` in the system browser and shows failures inline. */
export function OpenPageButton({ url, label, disabled }: OpenPageButtonProps) {
  const [error, setError] = useState<unknown>(null);
  const open = () => {
    setError(null);
    openExternal(url).catch((e: unknown) => setError(e));
  };
  return (
    <>
      <Button
        variant="secondary"
        size="sm"
        onClick={open}
        disabled={disabled}
        title={url}
        leftIcon={<ExternalLinkIcon className="size-4" aria-hidden />}
      >
        {label}
      </Button>
      {error !== null && <ErrorBanner error={error} className="basis-full" />}
    </>
  );
}

export interface OpenDownloadedFileButtonProps {
  path: string;
  label: string;
  disabled?: boolean;
  variant?: "primary" | "secondary";
}

/** Opens the downloaded installer with the OS (`openDownloadedFile`); errors shown inline. */
export function OpenDownloadedFileButton({
  path,
  label,
  disabled,
  variant = "primary",
}: OpenDownloadedFileButtonProps) {
  const [error, setError] = useState<unknown>(null);
  const open = () => {
    setError(null);
    openDownloadedFile(path).catch((e: unknown) => setError(e));
  };
  return (
    <>
      <Button
        size="sm"
        variant={variant}
        onClick={open}
        disabled={disabled}
        leftIcon={<FolderOpen className="size-4" aria-hidden />}
      >
        {label}
      </Button>
      {error !== null && <ErrorBanner error={error} className="basis-full" />}
    </>
  );
}

// ---------------------------------------------------------------------------
// Installer flow: release → download → downloaded → run
// ---------------------------------------------------------------------------

export interface ReleaseCardProps {
  release: InstallerRelease;
  /** Extra facts appended to the list (e.g. an approximate size). */
  extraItems?: { label: string; value: string }[];
}

/** Version / asset / source / hash availability of a resolved installer. */
export function ReleaseCard({ release, extraItems = [] }: ReleaseCardProps) {
  const { t } = useTranslation();
  return (
    <div className="space-y-3" data-testid="release-card">
      <KeyValueList
        items={[
          { label: t("install:release.version"), value: release.version, mono: true },
          { label: t("install:release.asset"), value: release.assetName, mono: true },
          { label: t("install:release.sourceLabel"), value: releaseSourceLabel(t, release.source) },
          ...extraItems,
          {
            label: t("install:release.hash"),
            value: release.sha256
              ? t("install:release.hashAvailable")
              : t("install:release.hashMissing"),
          },
        ]}
      />
      {!release.sha256 && <Alert variant="warning">{t("install:release.hashMissing")}</Alert>}
    </div>
  );
}

export interface DownloadButtonProps {
  onClick: () => void;
  label?: string;
  disabled?: boolean;
}

export function DownloadButton({ onClick, label, disabled }: DownloadButtonProps) {
  const { t } = useTranslation();
  return (
    <Button
      size="sm"
      onClick={onClick}
      disabled={disabled}
      leftIcon={<Download className="size-4" aria-hidden />}
    >
      {label ?? t("install:actions.download")}
    </Button>
  );
}

export interface DownloadProgressViewProps {
  release: InstallerRelease;
  progress: DownloadProgress | null;
}

/** Progress bar for a running download (`download://progress`). */
export function DownloadProgressView({ release, progress }: DownloadProgressViewProps) {
  const { t } = useTranslation();
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
      label={t("install:download.downloading", { asset: release.assetName })}
      detail={detail}
    />
  );
}

export function VerificationBadge({ download }: { download: DownloadResult }) {
  const { t } = useTranslation();
  if (download.verified === true) {
    return (
      <span
        className="text-success-500 inline-flex items-center gap-1 text-sm font-medium"
        data-testid="verified-badge"
      >
        <ShieldCheck className="size-4" aria-hidden />
        {t("install:download.verified")}
      </span>
    );
  }
  if (download.verified === false) {
    return (
      <span className="text-danger-500 inline-flex items-center gap-1 text-sm font-medium">
        <ShieldX className="size-4" aria-hidden />
        {t("install:download.verifiedFalse")}
      </span>
    );
  }
  return (
    <span className="text-warning-500 inline-flex items-center gap-1 text-sm font-medium">
      <ShieldOff className="size-4" aria-hidden />
      {t("install:download.unverified")}
    </span>
  );
}

/** "Download complete" box: file location + verification outcome. */
export function DownloadedAlert({ download }: { download: DownloadResult }) {
  const { t } = useTranslation();
  const runnable = download.verified !== false;
  return (
    <Alert variant={runnable ? "success" : "danger"} title={t("install:download.doneTitle")}>
      <p data-selectable className="font-mono text-xs break-all">
        {t("install:download.path", { path: download.path })}
      </p>
      <p className="mt-1">
        <VerificationBadge download={download} />
      </p>
    </Alert>
  );
}

export interface RunInstallerPanelProps {
  plan: InstallPlan;
  toolName: string;
  platform: Platform | null;
  onRun: () => void;
  starting?: boolean;
  disabled?: boolean;
}

/**
 * Show-before-run surface for a downloaded installer: the exact command, the explanation
 * (`install:plan.<code>`), the administrator note, the "Install now" button (labelled with
 * "needs administrator rights" when the plan elevates) and the help link to the one-click doc.
 */
export function RunInstallerPanel({
  plan,
  toolName,
  platform,
  onRun,
  starting = false,
  disabled = false,
}: RunInstallerPanelProps) {
  const { t } = useTranslation();
  return (
    <div className="space-y-3" data-testid="run-installer-panel">
      <PlanDetails plan={plan} toolName={toolName} platform={platform} />
      <div className="flex flex-wrap items-center gap-3">
        <Button
          onClick={onRun}
          loading={starting}
          disabled={disabled}
          data-testid="install-now"
          leftIcon={
            plan.requiresAdmin ? (
              <ShieldAlert className="size-4" aria-hidden />
            ) : (
              <Play className="size-4" aria-hidden />
            )
          }
        >
          {plan.requiresAdmin
            ? t("install:actions.installNowAdmin")
            : t("install:actions.installNow")}
        </Button>
        <HelpLink sectionId={ONE_CLICK_HELP_SECTION}>{t("install:oneClick.whatHappens")}</HelpLink>
      </div>
    </div>
  );
}

export interface RunningInstallerViewProps {
  plan: InstallPlan;
  toolName: string;
  platform: Platform | null;
  log: JobLog | null;
  cancelling: boolean;
  onCancel: () => void;
}

/** The installer job is running: plan, live output, cancel. */
export function RunningInstallerView({
  plan,
  toolName,
  platform,
  log,
  cancelling,
  onCancel,
}: RunningInstallerViewProps) {
  const { t } = useTranslation();
  return (
    <div className="space-y-3">
      <PlanDetails plan={plan} toolName={toolName} platform={platform} />
      {plan.requiresAdmin && <Alert variant="info">{t("install:oneClick.waitingForPrompt")}</Alert>}
      <OutputPane log={log} defaultOpen />
      <Button
        variant="secondary"
        size="sm"
        onClick={onCancel}
        disabled={cancelling}
        loading={cancelling}
        leftIcon={<Square className="size-4" aria-hidden />}
      >
        {t("install:actions.cancel")}
      </Button>
    </div>
  );
}

/** Collapsible output pane shown once a job has produced (or finished producing) output. */
export function OutputPane({ log, defaultOpen }: { log: JobLog | null; defaultOpen: boolean }) {
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
