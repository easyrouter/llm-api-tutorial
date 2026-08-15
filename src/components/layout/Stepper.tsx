import { Check } from "lucide-react";
import { useTranslation } from "react-i18next";

import { cn } from "@/lib/cn";
import { stepIndex, useWizardStore, WIZARD_STEPS } from "@/stores/wizard";

/**
 * Vertical step list. Completed and current steps are clickable; future steps are locked, and
 * every other step is locked while a screen holds the navigation lock (running install job).
 */
export function Stepper() {
  const { t } = useTranslation();
  const step = useWizardStore((s) => s.step);
  const furthest = useWizardStore((s) => s.furthestStep);
  const navigationLocked = useWizardStore((s) => s.navigationLocked);
  const goTo = useWizardStore((s) => s.goTo);

  const current = stepIndex(step);
  const reachable = stepIndex(furthest);

  return (
    <nav aria-label="wizard steps">
      <ol className="space-y-1">
        {WIZARD_STEPS.map((s, i) => {
          const isCurrent = i === current;
          const isDone = i < current;
          const isLocked = i > reachable || (navigationLocked && !isCurrent);
          return (
            <li key={s}>
              <button
                type="button"
                disabled={isLocked}
                onClick={() => goTo(s)}
                aria-current={isCurrent ? "step" : undefined}
                className={cn(
                  "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm transition-colors",
                  isCurrent &&
                    "bg-brand-50 text-brand-700 dark:bg-brand-700/20 dark:text-brand-100 font-medium",
                  !isCurrent &&
                    !isLocked &&
                    "text-neutral-700 hover:bg-neutral-100 dark:text-neutral-200 dark:hover:bg-neutral-800",
                  isLocked && "cursor-not-allowed text-neutral-400 dark:text-neutral-600",
                )}
              >
                <span
                  className={cn(
                    "inline-flex size-5 shrink-0 items-center justify-center rounded-full border text-[11px]",
                    isDone && "border-success-500 bg-success-500 text-white",
                    isCurrent && "border-brand-600 text-brand-700 dark:text-brand-100",
                    !isDone && !isCurrent && "border-neutral-300 dark:border-neutral-700",
                  )}
                  aria-hidden
                >
                  {isDone ? <Check className="size-3" /> : i + 1}
                </span>
                {t(`steps.${s}`)}
              </button>
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
