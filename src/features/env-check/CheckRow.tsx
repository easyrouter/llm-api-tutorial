import { ChevronDown, ChevronUp } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";

import {
  Button,
  ErrorBanner,
  FixActionButtons,
  StatusBadge,
  type BadgeStatus,
} from "@/components/ui";
import { cn } from "@/lib/cn";
import { formatDuration } from "@/lib/format";
import type { CheckId, CheckResult, InstallTarget } from "@/lib/types";

import { checkMessage } from "./snapshot-utils";

export interface CheckRowProps {
  id: CheckId;
  status: BadgeStatus;
  /** `null` until the first result arrives. */
  result: CheckResult | null;
  /** Error of the last individual re-run, if any. */
  error?: unknown;
  onRerun: (id: CheckId) => void;
  onInstall: (target: InstallTarget) => void;
}

/**
 * One environment check: badge + title, the localised message (`checks:<code>`), a collapsible
 * monospace details block (raw paths / versions from Rust) and the fix buttons.
 */
export function CheckRow({ id, status, result, error, onRerun, onInstall }: CheckRowProps) {
  const { t } = useTranslation();
  const detailsId = useId();
  const [detailsOpen, setDetailsOpen] = useState(false);
  const hasDetails = Boolean(result && result.details.length > 0);
  const busy = status === "running" || status === "pending";

  return (
    <li
      data-check-id={id}
      data-status={status}
      className="border-b border-neutral-200 py-4 last:border-b-0 dark:border-neutral-800"
    >
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <StatusBadge status={status} className="shrink-0" />
        <h3 className="min-w-0 flex-1 text-sm font-semibold">{t(`checks:${id}.title`)}</h3>
        {result && !busy && (
          <span className="text-xs text-neutral-500">
            {t("checks:screen.duration", { duration: formatDuration(result.durationMs) })}
          </span>
        )}
      </div>
      <div className="mt-2 min-w-0 space-y-2">
        {result && !busy && (
          <p className="text-sm text-neutral-700 dark:text-neutral-300">
            {checkMessage(t, result)}
          </p>
        )}
        {busy && (
          <p className="text-sm text-neutral-500">
            {status === "running" ? t("checks:screen.checking") : t("common:status.pending")}
          </p>
        )}
        {hasDetails && result && (
          <div>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setDetailsOpen((v) => !v)}
              aria-expanded={detailsOpen}
              aria-controls={detailsId}
              leftIcon={
                detailsOpen ? (
                  <ChevronUp className="size-4" aria-hidden />
                ) : (
                  <ChevronDown className="size-4" aria-hidden />
                )
              }
              className="-ml-3"
            >
              {detailsOpen ? t("checks:screen.details.hide") : t("checks:screen.details.show")}
            </Button>
            <pre
              id={detailsId}
              hidden={!detailsOpen}
              data-selectable
              className={cn(
                "mt-1 max-h-48 overflow-auto rounded-md bg-neutral-100 p-3 font-mono text-xs leading-5 whitespace-pre-wrap text-neutral-800 dark:bg-neutral-950 dark:text-neutral-200",
              )}
            >
              {result.details.join("\n")}
            </pre>
          </div>
        )}
        {result && !busy && (
          <FixActionButtons
            fixes={result.fixes}
            onRerun={() => onRerun(id)}
            onInstall={onInstall}
          />
        )}
        {error != null && <ErrorBanner error={error} onRetry={() => onRerun(id)} />}
      </div>
    </li>
  );
}
