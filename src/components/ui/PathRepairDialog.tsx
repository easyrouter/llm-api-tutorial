import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { HelpLink } from "@/features/help/HelpLink";
import { applyPathRepair, planPathRepair } from "@/lib/tauri";
import type { PathRepairPlan, PathRepairResult } from "@/lib/types";

import { Alert } from "./Alert";
import { ConfirmDialog } from "./ConfirmDialog";
import { CopyField } from "./CopyField";
import { KeyValueList } from "./KeyValueList";

export interface PathRepairDialogProps {
  /** Directory to add to the user's persistent PATH. */
  dir: string;
  /** Called after the repair ran; the caller then unmounts the dialog (`onClose` follows). */
  onApplied: (result: PathRepairResult) => void;
  /** Planning or applying failed; the caller shows the error (`onClose` follows). */
  onError: (error: unknown) => void;
  /** Cancel / done — the caller unmounts the dialog. */
  onClose: () => void;
}

type Phase =
  | { kind: "planning" }
  | { kind: "ready"; plan: PathRepairPlan }
  | { kind: "applying"; plan: PathRepairPlan };

/**
 * "Show before run" dialog for the one-click PATH repair (`FixAction.repair_path`). Mount it
 * to open it: `planPathRepair(dir)` runs on mount → the dialog shows the location
 * (`HKCU\Environment\Path` / `~/.zshrc`), the exact command, the backup note and — when the
 * dir is already present — an info note that confirming is a no-op → `applyPathRepair(plan)`
 * on confirm. Never needs admin. Errors and results go to the callbacks; the dialog itself
 * only renders the plan.
 */
export function PathRepairDialog({ dir, onApplied, onError, onClose }: PathRepairDialogProps) {
  const { t } = useTranslation();
  const [phase, setPhase] = useState<Phase>({ kind: "planning" });
  // Latest callbacks without re-planning when the parent re-renders with new closures.
  const callbacks = useRef({ onApplied, onError, onClose });
  useEffect(() => {
    callbacks.current = { onApplied, onError, onClose };
  });

  useEffect(() => {
    let cancelled = false;
    planPathRepair(dir).then(
      (plan) => {
        if (!cancelled) setPhase({ kind: "ready", plan });
      },
      (e: unknown) => {
        if (cancelled) return;
        callbacks.current.onError(e);
        callbacks.current.onClose();
      },
    );
    return () => {
      cancelled = true;
    };
  }, [dir]);

  const plan = phase.kind === "planning" ? null : phase.plan;
  const busy = phase.kind !== "ready";

  const confirm = () => {
    if (phase.kind !== "ready") return;
    const { plan: current } = phase;
    setPhase({ kind: "applying", plan: current });
    applyPathRepair(current).then(
      (result) => {
        callbacks.current.onApplied(result);
        callbacks.current.onClose();
      },
      (e: unknown) => {
        callbacks.current.onError(e);
        callbacks.current.onClose();
      },
    );
  };

  return (
    <ConfirmDialog
      open
      title={t("remediate.path.title")}
      description={t("remediate.path.description")}
      confirmLabel={t("remediate.path.confirm")}
      confirmLoading={busy}
      onConfirm={confirm}
      onCancel={onClose}
    >
      <div className="space-y-3" data-testid="path-repair-dialog">
        {plan === null ? (
          <p className="text-sm text-neutral-500">{t("remediate.path.planning")}</p>
        ) : (
          <>
            {plan.alreadyPresent && (
              <Alert variant="info">{t("remediate.path.alreadyPresent")}</Alert>
            )}
            <KeyValueList
              items={[
                { key: "dir", label: t("remediate.path.dir"), value: plan.dir, mono: true },
                {
                  key: "location",
                  label: t("remediate.path.location"),
                  value: plan.location,
                  mono: true,
                },
              ]}
            />
            <CopyField label={t("remediate.path.command")} value={plan.displayCommand} />
            {plan.createsBackup && (
              <p className="text-xs text-neutral-500">{t("remediate.path.backupNote")}</p>
            )}
          </>
        )}
        <HelpLink sectionId="one-click">{t("remediate.helpLink")}</HelpLink>
      </div>
    </ConfirmDialog>
  );
}
