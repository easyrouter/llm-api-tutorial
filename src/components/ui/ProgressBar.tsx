import { useId, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { cn } from "@/lib/cn";
import { formatPercent } from "@/lib/format";

import "./ProgressBar.css";

export interface ProgressBarProps {
  /** 0..1. `undefined` renders an indeterminate (busy) bar. */
  value?: number;
  label?: ReactNode;
  /** Show the percentage next to the label (determinate only). Default true. */
  showPercent?: boolean;
  /** Free text at the right (e.g. `12.3 MB / 40 MB`); replaces the percentage when given. */
  detail?: ReactNode;
  className?: string;
}

/** Clamps to 0..1 and drops NaN. */
function clamp01(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(1, Math.max(0, value));
}

/** Determinate or indeterminate progress bar with an accessible `progressbar` role. */
export function ProgressBar({
  value,
  label,
  showPercent = true,
  detail,
  className,
}: ProgressBarProps) {
  const { t } = useTranslation();
  const id = useId();
  const determinate = value !== undefined;
  const ratio = determinate ? clamp01(value) : 0;
  const right = detail ?? (determinate && showPercent ? formatPercent(ratio) : null);
  const hasHeader = Boolean(label) || right !== null;

  return (
    <div className={cn("space-y-1.5", className)}>
      {hasHeader && (
        <div className="flex items-baseline justify-between gap-3 text-sm">
          <span id={id} className="text-neutral-700 dark:text-neutral-200">
            {label}
          </span>
          {right !== null && <span className="text-xs text-neutral-500 tabular-nums">{right}</span>}
        </div>
      )}
      <div
        role="progressbar"
        aria-labelledby={label ? id : undefined}
        aria-label={label ? undefined : t("ui.progress")}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={determinate ? Math.round(ratio * 100) : undefined}
        aria-valuetext={determinate ? undefined : t("ui.indeterminate")}
        aria-busy={!determinate}
        className="h-2 w-full overflow-hidden rounded-full bg-neutral-200 dark:bg-neutral-800"
      >
        <div
          data-testid="progress-fill"
          className={cn(
            "bg-brand-600 h-full rounded-full transition-[width] duration-300 ease-out",
            !determinate && "ui-progress-indeterminate w-2/5",
          )}
          style={determinate ? { width: `${ratio * 100}%` } : undefined}
        />
      </div>
    </div>
  );
}
