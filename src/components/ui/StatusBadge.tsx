import { CircleCheck, CircleDashed, CircleMinus, CircleX, TriangleAlert } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { cn } from "@/lib/cn";
import type { CheckStatus } from "@/lib/types";

import { Spinner } from "./Spinner";

/** `CheckStatus` from Rust plus the two UI-only transient states. */
export type BadgeStatus = CheckStatus | "running" | "pending";

export interface StatusBadgeProps {
  status: BadgeStatus;
  /** Override the label (defaults to `common:status.<status>`). */
  label?: ReactNode;
  /** Icon-only badge; the label is still exposed to assistive tech. */
  compact?: boolean;
  className?: string;
}

const styles: Record<BadgeStatus, string> = {
  pass: "border-success-500/30 bg-success-500/10 text-success-500",
  warn: "border-warning-500/30 bg-warning-500/10 text-warning-500",
  fail: "border-danger-500/30 bg-danger-500/10 text-danger-500",
  skipped:
    "border-neutral-300 bg-neutral-100 text-neutral-600 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-300",
  pending:
    "border-neutral-300 bg-neutral-50 text-neutral-500 dark:border-neutral-700 dark:bg-neutral-900 dark:text-neutral-400",
  running:
    "border-brand-500/30 bg-brand-50 text-brand-700 dark:bg-brand-700/20 dark:text-brand-100",
};

function StatusIcon({ status }: { status: BadgeStatus }) {
  const cls = "size-4 shrink-0";
  switch (status) {
    case "pass":
      return <CircleCheck className={cls} aria-hidden />;
    case "warn":
      return <TriangleAlert className={cls} aria-hidden />;
    case "fail":
      return <CircleX className={cls} aria-hidden />;
    case "skipped":
      return <CircleMinus className={cls} aria-hidden />;
    case "pending":
      return <CircleDashed className={cls} aria-hidden />;
    case "running":
      return <Spinner size="sm" decorative className="text-current" />;
  }
}

/**
 * Three-state check result pill (pass / warn / fail) plus skipped, pending and running.
 * Colours follow the design tokens: pass=success, warn=warning, fail=danger, running=brand.
 */
export function StatusBadge({ status, label, compact = false, className }: StatusBadgeProps) {
  const { t } = useTranslation();
  const text = label ?? t(`status.${status}`);
  return (
    <span
      data-status={status}
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-xs font-medium whitespace-nowrap",
        styles[status],
        className,
      )}
    >
      <StatusIcon status={status} />
      <span className={cn(compact && "sr-only")}>{text}</span>
    </span>
  );
}
