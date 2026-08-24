import { CircleHelp, Languages } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { HelpPanel } from "@/features/help/HelpPanel";
import { setLang, type Lang } from "@/i18n";
import { useAppStore } from "@/stores/app";
import { useWizardStore } from "@/stores/wizard";

import { Stepper } from "./Stepper";

export function AppShell({ children }: { children: ReactNode }) {
  const { t, i18n } = useTranslation();
  const info = useAppStore((s) => s.info);
  const helpOpen = useWizardStore((s) => s.helpOpen);
  const openHelp = useWizardStore((s) => s.openHelp);
  const closeHelp = useWizardStore((s) => s.closeHelp);

  const toggleLang = () => {
    const next: Lang = i18n.language.startsWith("zh") ? "en" : "zh-CN";
    void setLang(next);
  };

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center justify-between border-b border-neutral-200 bg-white px-5 py-3 dark:border-neutral-800 dark:bg-neutral-900">
        <div className="flex items-center gap-2.5">
          <img src="/logo.png" alt="" aria-hidden className="size-6 shrink-0" />
          <div className="flex items-baseline gap-3">
            <h1 className="text-base font-semibold">{t("app.name")}</h1>
            {info && (
              <span className="text-xs text-neutral-500">
                {t("app.version", { version: info.version })}
              </span>
            )}
          </div>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={toggleLang}
            className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-sm text-neutral-600 hover:bg-neutral-100 dark:text-neutral-300 dark:hover:bg-neutral-800"
            aria-label={t("app.language")}
            title={t("app.language")}
          >
            <Languages className="size-4" aria-hidden />
            {i18n.language.startsWith("zh") ? "中文" : "EN"}
          </button>
          <button
            type="button"
            onClick={() => (helpOpen ? closeHelp() : openHelp())}
            className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-sm text-neutral-600 hover:bg-neutral-100 dark:text-neutral-300 dark:hover:bg-neutral-800"
            aria-pressed={helpOpen}
            title={t("actions.help")}
          >
            <CircleHelp className="size-4" aria-hidden />
            {t("actions.help")}
          </button>
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        <aside className="w-52 shrink-0 border-r border-neutral-200 bg-white p-4 dark:border-neutral-800 dark:bg-neutral-900">
          <Stepper />
        </aside>

        <main className="min-w-0 flex-1 overflow-y-auto p-6" data-selectable>
          {children}
        </main>

        {helpOpen && (
          <aside className="w-96 shrink-0 border-l border-neutral-200 bg-white dark:border-neutral-800 dark:bg-neutral-900">
            <HelpPanel />
          </aside>
        )}
      </div>

      <footer className="flex items-center justify-between border-t border-neutral-200 bg-white px-5 py-1.5 text-[11px] text-neutral-500 dark:border-neutral-800 dark:bg-neutral-900">
        <span>{info ? t(`footer.configSource.${info.configSource}`) : ""}</span>
        <span className="truncate" title={info?.logDir ?? ""}>
          {info ? t("footer.logDir", { path: info.logDir }) : ""}
        </span>
      </footer>
    </div>
  );
}
