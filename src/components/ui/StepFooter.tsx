import { ArrowLeft, ArrowRight } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { cn } from "@/lib/cn";

import { Button } from "./Button";

export interface StepFooterProps {
  /** Omit to hide the Back button (first step). */
  onBack?: () => void;
  /** Omit to hide the Next button. */
  onNext?: () => void;
  backLabel?: ReactNode;
  nextLabel?: ReactNode;
  /** Disable Back (e.g. while a job that must not be abandoned is running). */
  backDisabled?: boolean;
  nextDisabled?: boolean;
  /** Spinner on Next (e.g. while checks are still running). */
  nextLoading?: boolean;
  /** Anything in the middle: a Skip button, a status line, a "re-check" action… */
  extra?: ReactNode;
  className?: string;
}

/** Bottom navigation used by every wizard screen: Back on the left, `extra` centre, Next right. */
export function StepFooter({
  onBack,
  onNext,
  backLabel,
  nextLabel,
  backDisabled = false,
  nextDisabled = false,
  nextLoading = false,
  extra,
  className,
}: StepFooterProps) {
  const { t } = useTranslation();
  return (
    <div
      className={cn(
        "flex items-center justify-between gap-4 border-t border-neutral-200 pt-5 dark:border-neutral-800",
        className,
      )}
    >
      <div className="flex min-w-0 items-center">
        {onBack && (
          <Button
            variant="secondary"
            onClick={onBack}
            disabled={backDisabled}
            leftIcon={<ArrowLeft className="size-4" aria-hidden />}
          >
            {backLabel ?? t("actions.back")}
          </Button>
        )}
      </div>
      <div className="flex min-w-0 flex-1 items-center justify-center gap-3">{extra}</div>
      <div className="flex min-w-0 items-center justify-end">
        {onNext && (
          <Button onClick={onNext} disabled={nextDisabled} loading={nextLoading}>
            {nextLabel ?? t("actions.next")}
            {!nextLoading && <ArrowRight className="size-4" aria-hidden />}
          </Button>
        )}
      </div>
    </div>
  );
}
