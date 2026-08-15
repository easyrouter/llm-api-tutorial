import { X } from "lucide-react";
import { useTranslation } from "react-i18next";

import { useWizardStore } from "@/stores/wizard";

/** TODO(impl): M6 help panel — docs index + page viewer (react-markdown), wizard-step linkage. */
export function HelpPanel() {
  const { t } = useTranslation();
  const closeHelp = useWizardStore((s) => s.closeHelp);
  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center justify-between border-b border-neutral-200 px-4 py-3 dark:border-neutral-800">
        <h2 className="text-sm font-semibold">{t("help.panelTitle")}</h2>
        <button
          type="button"
          onClick={closeHelp}
          className="rounded p-1 text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-800"
          aria-label={t("help.closePanel")}
        >
          <X className="size-4" aria-hidden />
        </button>
      </header>
      <div className="flex-1 overflow-y-auto p-4 text-sm text-neutral-500">TODO</div>
    </div>
  );
}
