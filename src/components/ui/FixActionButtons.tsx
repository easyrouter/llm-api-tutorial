import {
  ArrowRight,
  BookOpen,
  ChevronUp,
  Download,
  Eraser,
  ExternalLink as ExternalLinkIcon,
  Globe,
  RefreshCw,
  Store,
  Wrench,
} from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";

import { cn } from "@/lib/cn";
import { openExternal, openSystemUri } from "@/lib/tauri";
import type {
  EnvCleanupResult,
  FixAction,
  InstallTarget,
  PathRepairResult,
  SystemUri,
} from "@/lib/types";
import { useWizardStore } from "@/stores/wizard";

import { Alert } from "./Alert";
import { Button, type ButtonProps } from "./Button";
import { EnvCleanupDialog } from "./EnvCleanupDialog";
import { ErrorBanner } from "./ErrorBanner";
import { PathRepairDialog } from "./PathRepairDialog";

export interface FixActionButtonsProps {
  fixes: FixAction[];
  /**
   * Handler for `rerun` fixes; the button is hidden when absent. Also called automatically
   * after a one-click remediation (`repair_path`, `clean_env_vars`) succeeded, so the check
   * re-runs without another click.
   */
  onRerun?: () => void;
  /**
   * Handler for `install` fixes. When absent the button navigates to the Install step, which
   * lists everything that is missing.
   */
  onInstall?: (target: InstallTarget) => void;
  size?: ButtonProps["size"];
  className?: string;
}

/** Which one-click dialog is open. */
type OpenDialog =
  { kind: "repair_path"; dir: string } | { kind: "clean_env_vars"; names: string[] };

/** Outcome of the last one-click remediation, shown under the buttons. */
type Outcome =
  | { kind: "repair_path"; dir: string; result: PathRepairResult }
  | { kind: "clean_env_vars"; result: EnvCleanupResult };

const SYSTEM_URI_ICON: Record<SystemUri, typeof Store> = {
  ms_store_codex_app: Store,
  windows_region_settings: Globe,
};

/**
 * Renders one button per `FixAction` attached to a check result or diagnosis:
 *
 * | kind              | label (common ns)                                  | click                                      |
 * | ----------------- | -------------------------------------------------- | ------------------------------------------ |
 * | `open_url`        | `fixes.open_url` + `fixes.labels.<labelCode>`      | `openExternal(url)` (Rust validates)       |
 * | `go_to_step`      | `fixes.go_to_step` + `steps.<step>`                | wizard store `goTo(step)`                  |
 * | `install`         | `fixes.install` + `tools.<tool>`                   | `onInstall(tool)` or `goTo("install")`     |
 * | `instructions`    | `fixes.instructions` / `ui.hideInstructions`       | toggles an inline Alert                    |
 * | `repair_path`     | `fixes.repair_path`                                | `PathRepairDialog` (plan → confirm → apply)|
 * | `clean_env_vars`  | `fixes.clean_env_vars`                             | `EnvCleanupDialog` (plan → confirm → apply)|
 * | `open_system_uri` | `fixes.open_system_uri.<uri>` (+ `_hint` tooltip)  | `openSystemUri(uri)`                       |
 * | `rerun`           | `fixes.rerun`                                      | `onRerun()`                                |
 *
 * The one-click kinds are self-contained: the component owns the dialogs and renders the
 * outcome (what changed, backups, "close your terminals") under the buttons, then calls
 * `onRerun` so the owning check re-runs. Callers need no extra wiring.
 *
 * `instructions.code` must be a **fully-qualified** i18n key (`"checks:env_vars.instructions"`,
 * i.e. `<namespace>:<path>`), because this component cannot know which namespace the Rust
 * module that produced the action belongs to. It is rendered as-is with `t(code, params)`.
 * Unknown `labelCode`s fall back to the raw code so a missing translation is visible, not
 * silent.
 */
export function FixActionButtons({
  fixes,
  onRerun,
  onInstall,
  size = "sm",
  className,
}: FixActionButtonsProps) {
  const { t } = useTranslation();
  const goTo = useWizardStore((s) => s.goTo);
  const idBase = useId();
  const [openInstructions, setOpenInstructions] = useState<Set<number>>(() => new Set());
  const [error, setError] = useState<unknown>(null);
  const [dialog, setDialog] = useState<OpenDialog | null>(null);
  const [outcome, setOutcome] = useState<Outcome | null>(null);

  if (fixes.length === 0 && outcome === null && error === null) return null;

  const toggleInstructions = (index: number) =>
    setOpenInstructions((prev) => {
      const next = new Set(prev);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return next;
    });

  const openUrl = (url: string) => {
    setError(null);
    openExternal(url).catch((e: unknown) => setError(e));
  };

  const openSystem = (uri: SystemUri) => {
    setError(null);
    openSystemUri(uri).catch((e: unknown) => setError(e));
  };

  const openDialog = (next: OpenDialog) => {
    setError(null);
    setOutcome(null);
    setDialog(next);
  };

  const finish = (next: Outcome) => {
    setOutcome(next);
    onRerun?.();
  };

  const renderButton = (fix: FixAction, index: number) => {
    switch (fix.kind) {
      case "open_url":
        return (
          <Button
            key={index}
            variant="secondary"
            size={size}
            onClick={() => openUrl(fix.url)}
            title={fix.url}
            leftIcon={<ExternalLinkIcon className="size-4" aria-hidden />}
          >
            {t("fixes.open_url", {
              label: t(`fixes.labels.${fix.labelCode}`, { defaultValue: fix.labelCode }),
            })}
          </Button>
        );
      case "go_to_step":
        return (
          <Button
            key={index}
            variant="secondary"
            size={size}
            onClick={() => goTo(fix.step)}
            leftIcon={<ArrowRight className="size-4" aria-hidden />}
          >
            {t("fixes.go_to_step", { step: t(`steps.${fix.step}`) })}
          </Button>
        );
      case "install":
        return (
          <Button
            key={index}
            variant="primary"
            size={size}
            onClick={() => (onInstall ? onInstall(fix.tool) : goTo("install"))}
            leftIcon={<Download className="size-4" aria-hidden />}
          >
            {t("fixes.install", { tool: t(`tools.${fix.tool}`) })}
          </Button>
        );
      case "instructions": {
        const expanded = openInstructions.has(index);
        return (
          <Button
            key={index}
            variant="secondary"
            size={size}
            onClick={() => toggleInstructions(index)}
            aria-expanded={expanded}
            aria-controls={`${idBase}-instructions-${index}`}
            leftIcon={
              expanded ? (
                <ChevronUp className="size-4" aria-hidden />
              ) : (
                <BookOpen className="size-4" aria-hidden />
              )
            }
          >
            {expanded ? t("ui.hideInstructions") : t("fixes.instructions")}
          </Button>
        );
      }
      case "repair_path":
        return (
          <Button
            key={index}
            variant="primary"
            size={size}
            onClick={() => openDialog({ kind: "repair_path", dir: fix.dir })}
            title={fix.dir}
            leftIcon={<Wrench className="size-4" aria-hidden />}
          >
            {t("fixes.repair_path")}
          </Button>
        );
      case "clean_env_vars":
        return (
          <Button
            key={index}
            variant="primary"
            size={size}
            onClick={() => openDialog({ kind: "clean_env_vars", names: fix.names })}
            title={fix.names.join(", ")}
            leftIcon={<Eraser className="size-4" aria-hidden />}
          >
            {t("fixes.clean_env_vars")}
          </Button>
        );
      case "open_system_uri": {
        const Icon = SYSTEM_URI_ICON[fix.uri];
        return (
          <Button
            key={index}
            variant="secondary"
            size={size}
            onClick={() => openSystem(fix.uri)}
            title={t(`fixes.open_system_uri_hint.${fix.uri}`)}
            leftIcon={<Icon className="size-4" aria-hidden />}
          >
            {t(`fixes.open_system_uri.${fix.uri}`)}
          </Button>
        );
      }
      case "rerun":
        if (!onRerun) return null;
        return (
          <Button
            key={index}
            variant="secondary"
            size={size}
            onClick={onRerun}
            leftIcon={<RefreshCw className="size-4" aria-hidden />}
          >
            {t("fixes.rerun")}
          </Button>
        );
    }
  };

  return (
    <div className={cn("space-y-3", className)}>
      {fixes.length > 0 && (
        <div className="flex flex-wrap items-center gap-2">{fixes.map(renderButton)}</div>
      )}
      {fixes.map((fix, index) =>
        fix.kind === "instructions" && openInstructions.has(index) ? (
          <Alert key={index} variant="info" id={`${idBase}-instructions-${index}`}>
            <p className="whitespace-pre-line">{t(fix.code, fix.params)}</p>
          </Alert>
        ) : null,
      )}
      {outcome !== null && <RemediationOutcome outcome={outcome} />}
      {error !== null && <ErrorBanner error={error} />}
      {dialog?.kind === "repair_path" && (
        <PathRepairDialog
          dir={dialog.dir}
          onApplied={(result) => finish({ kind: "repair_path", dir: dialog.dir, result })}
          onError={setError}
          onClose={() => setDialog(null)}
        />
      )}
      {dialog?.kind === "clean_env_vars" && (
        <EnvCleanupDialog
          names={dialog.names}
          onApplied={(result) => finish({ kind: "clean_env_vars", result })}
          onError={setError}
          onClose={() => setDialog(null)}
        />
      )}
    </div>
  );
}

function RemediationOutcome({ outcome }: { outcome: Outcome }) {
  const { t } = useTranslation();
  if (outcome.kind === "repair_path") {
    const { dir, result } = outcome;
    return (
      <Alert
        variant="success"
        title={t("remediate.path.result.title")}
        data-testid="remediation-outcome"
      >
        <p>
          {result.changed
            ? t("remediate.path.result.changed", { dir, location: result.location })
            : t("remediate.path.result.unchanged", { dir })}
        </p>
        {result.backupPath && (
          <p data-selectable className="font-mono text-xs break-all">
            {t("remediate.path.result.backup", { path: result.backupPath })}
          </p>
        )}
        <p className="font-medium">{t("remediate.restartTerminals")}</p>
      </Alert>
    );
  }
  const { removed, failed, backups } = outcome.result;
  return (
    <Alert
      variant={failed.length > 0 ? "warning" : "success"}
      title={t("remediate.env.result.title")}
      data-testid="remediation-outcome"
    >
      {removed.length > 0 ? (
        <p>{t("remediate.env.result.removed", { names: removed.join(", ") })}</p>
      ) : (
        <p>{t("remediate.env.result.none")}</p>
      )}
      {failed.length > 0 && <p>{t("remediate.env.result.failed", { names: failed.join(", ") })}</p>}
      {backups.length > 0 && (
        <p data-selectable className="font-mono text-xs break-all">
          {t("remediate.env.result.backups", { paths: backups.join(", ") })}
        </p>
      )}
      <p className="font-medium">{t("remediate.restartTerminals")}</p>
    </Alert>
  );
}
