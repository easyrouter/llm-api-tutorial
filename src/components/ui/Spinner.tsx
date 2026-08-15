import { LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";

import { cn } from "@/lib/cn";

export type SpinnerSize = "xs" | "sm" | "md" | "lg";

export interface SpinnerProps {
  size?: SpinnerSize;
  /** Accessible name; defaults to `common:ui.loading`. */
  label?: string;
  /**
   * Purely visual — no `role="status"`, hidden from assistive tech. Use when neighbouring text
   * already says what is happening (e.g. inside `StatusBadge running`).
   */
  decorative?: boolean;
  className?: string;
}

const sizes: Record<SpinnerSize, string> = {
  xs: "size-3",
  sm: "size-4",
  md: "size-6",
  lg: "size-10",
};

/** Indeterminate activity indicator (announced once as a status, spins visually). */
export function Spinner({ size = "md", label, decorative = false, className }: SpinnerProps) {
  const { t } = useTranslation();
  const a11y = decorative
    ? { "aria-hidden": true as const }
    : { role: "status", "aria-label": label ?? t("ui.loading") };
  return (
    <span
      {...a11y}
      className={cn("text-brand-600 dark:text-brand-500 inline-flex shrink-0", className)}
    >
      <LoaderCircle className={cn("animate-spin", sizes[size])} aria-hidden />
    </span>
  );
}
