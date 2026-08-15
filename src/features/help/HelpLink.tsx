import { CircleHelp } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { cn } from "@/lib/cn";
import { useWizardStore } from "@/stores/wizard";

export interface HelpLinkProps {
  /** Help section id (`overview`, `env-check`, `install-node`, `verify`, `faq`, …). */
  sectionId: string;
  /** Defaults to `common:actions.learnMore`. */
  children?: ReactNode;
  className?: string;
}

/**
 * "Learn more" affordance for any screen: opens the help drawer on `sectionId`
 * (`wizard.openHelp(sectionId)`). Looks like a small text link; drop it next to the thing it
 * explains. Section ids are listed in `src-tauri/resources/docs/README.md`.
 */
export function HelpLink({ sectionId, children, className }: HelpLinkProps) {
  const { t } = useTranslation();
  const openHelp = useWizardStore((s) => s.openHelp);
  return (
    <button
      type="button"
      onClick={() => openHelp(sectionId)}
      data-help-section={sectionId}
      className={cn(
        "text-brand-700 hover:text-brand-600 focus-visible:ring-brand-500 dark:text-brand-100 inline-flex items-center gap-1 rounded-sm text-sm underline underline-offset-2 focus-visible:ring-2 focus-visible:outline-none",
        className,
      )}
    >
      <CircleHelp className="size-3.5" aria-hidden />
      {children ?? t("actions.learnMore")}
    </button>
  );
}
