import { Eye, EyeOff } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { HelpLink } from "@/features/help/HelpLink";
import { cn } from "@/lib/cn";
import { applyEnvCleanup, planEnvCleanup } from "@/lib/tauri";
import type { EnvCleanupItem, EnvCleanupPlan, EnvCleanupResult } from "@/lib/types";

import { Alert } from "./Alert";
import { Button } from "./Button";
import { ConfirmDialog } from "./ConfirmDialog";

export interface EnvCleanupDialogProps {
  /** Environment variable names to remove from their persistent sources. */
  names: string[];
  /** Called after the clean-up ran; the caller then unmounts the dialog (`onClose` follows). */
  onApplied: (result: EnvCleanupResult) => void;
  /** Planning or applying failed; the caller shows the error (`onClose` follows). */
  onError: (error: unknown) => void;
  /** Cancel / done — the caller unmounts the dialog. */
  onClose: () => void;
}

type Phase =
  | { kind: "planning" }
  | { kind: "ready"; plan: EnvCleanupPlan }
  | { kind: "applying"; plan: EnvCleanupPlan };

/**
 * "Show before run" dialog for the one-click env-var clean-up (`FixAction.clean_env_vars`).
 * Mount it to open it: `planEnvCleanup(names)` runs on mount → every item is listed with its
 * source, the **full** current value (shown by default — the product owner wants the real base
 * URL / token visible before deletion; a toggle hides them; nothing is logged), the rc line,
 * the action (user registry / machine registry = UAC / comment out with backup / launchctl /
 * cannot remove) and the exact command → `applyEnvCleanup(plan)` on confirm. A warning is
 * shown when the plan `requiresAdmin`.
 */
export function EnvCleanupDialog({ names, onApplied, onError, onClose }: EnvCleanupDialogProps) {
  const { t } = useTranslation();
  const [phase, setPhase] = useState<Phase>({ kind: "planning" });
  const [showValues, setShowValues] = useState(true);
  const callbacks = useRef({ onApplied, onError, onClose });
  useEffect(() => {
    callbacks.current = { onApplied, onError, onClose };
  });

  // `names` comes straight from the FixAction, which is referentially stable per check result.
  useEffect(() => {
    let cancelled = false;
    planEnvCleanup(names).then(
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
  }, [names]);

  const plan = phase.kind === "planning" ? null : phase.plan;
  const busy = phase.kind !== "ready";
  const removable = plan?.items.some((i) => i.action !== "none") ?? false;
  const requiresAdmin = plan?.requiresAdmin ?? false;

  const confirm = () => {
    if (phase.kind !== "ready") return;
    const { plan: current } = phase;
    setPhase({ kind: "applying", plan: current });
    applyEnvCleanup(current).then(
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
      title={t("remediate.env.title")}
      description={t("remediate.env.description")}
      confirmLabel={requiresAdmin ? t("remediate.env.confirmAdmin") : t("remediate.env.confirm")}
      confirmLoading={busy}
      danger
      onConfirm={confirm}
      onCancel={onClose}
    >
      <div className="space-y-3" data-testid="env-cleanup-dialog">
        {plan === null ? (
          <p className="text-sm text-neutral-500">{t("remediate.env.planning")}</p>
        ) : (
          <>
            {requiresAdmin && <Alert variant="warning">{t("remediate.env.adminWarning")}</Alert>}
            {plan.items.length === 0 ? (
              <p className="text-sm text-neutral-500">{t("remediate.env.empty")}</p>
            ) : (
              <>
                <div className="flex items-center justify-between gap-2">
                  <p className="text-xs text-neutral-500">{t("remediate.env.valuesShownNote")}</p>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => setShowValues((v) => !v)}
                    aria-pressed={showValues}
                    leftIcon={
                      showValues ? (
                        <EyeOff className="size-4" aria-hidden />
                      ) : (
                        <Eye className="size-4" aria-hidden />
                      )
                    }
                  >
                    {showValues ? t("remediate.env.hideValues") : t("remediate.env.showValues")}
                  </Button>
                </div>
                <ul className="max-h-80 space-y-3 overflow-auto">
                  {plan.items.map((item, index) => (
                    <EnvCleanupRow key={index} item={item} showValue={showValues} />
                  ))}
                </ul>
              </>
            )}
            {!removable && plan.items.length > 0 && (
              <Alert variant="info">{t("remediate.env.action.none")}</Alert>
            )}
          </>
        )}
        <HelpLink sectionId="one-click">{t("remediate.helpLink")}</HelpLink>
      </div>
    </ConfirmDialog>
  );
}

function EnvCleanupRow({ item, showValue }: { item: EnvCleanupItem; showValue: boolean }) {
  const { t } = useTranslation();
  const none = item.action === "none";
  return (
    <li
      data-testid="env-cleanup-item"
      data-action={item.action}
      className={cn(
        "rounded-md border p-3 text-sm",
        none
          ? "border-neutral-200 bg-neutral-50 dark:border-neutral-800 dark:bg-neutral-950"
          : "border-warning-500/30",
      )}
    >
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <code data-selectable className="font-mono font-semibold">
          {item.name}
        </code>
        <span className="text-xs text-neutral-500">
          {t(`checks:envVars.source.${item.source.kind}`, { defaultValue: item.source.kind })}
          {item.source.location && (
            <>
              {" · "}
              <code data-selectable className="font-mono break-all">
                {item.source.location}
              </code>
            </>
          )}
        </span>
      </div>
      <dl className="mt-2 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-xs">
        {item.value !== null && (
          <>
            <dt className="text-neutral-500">{t("remediate.env.columns.value")}</dt>
            <dd data-selectable data-testid="env-cleanup-value" className="font-mono break-all">
              {showValue ? item.value : t("remediate.env.hidden")}
            </dd>
          </>
        )}
        {item.line !== null && (
          <>
            <dt className="text-neutral-500">{t("remediate.env.columns.line")}</dt>
            <dd data-selectable className="font-mono break-all">
              {showValue ? item.line : t("remediate.env.hidden")}
            </dd>
          </>
        )}
        <dt className="text-neutral-500">{t("remediate.env.columns.action")}</dt>
        <dd className={cn(none && "text-neutral-500")}>
          {t(`remediate.env.action.${item.action}`)}
        </dd>
        {item.displayCommand && (
          <>
            <dt className="text-neutral-500">{t("remediate.env.columns.command")}</dt>
            <dd data-selectable className="font-mono break-all">
              {item.displayCommand}
            </dd>
          </>
        )}
      </dl>
    </li>
  );
}
