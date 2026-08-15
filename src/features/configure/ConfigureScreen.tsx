import { CircleHelp, SquareTerminal } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Alert, Button, StepFooter } from "@/components/ui";
import { cn } from "@/lib/cn";
import type { ToolId } from "@/lib/types";
import { useWizardStore } from "@/stores/wizard";

import { ToolGuide } from "./ToolGuide";

/**
 * Configure step (M3, positioning A): the user configures inside CC Switch; this screen supplies
 * the values, walks through the steps, validates key format / URL rules and reminds the user to
 * close every terminal afterwards. One tab per selected tool; the active tab's `ToolGuide` is
 * keyed by tool so switching tabs discards any typed key.
 */
export function ConfigureScreen() {
  const { t } = useTranslation();
  const selectedTools = useWizardStore((s) => s.selectedTools);
  const next = useWizardStore((s) => s.next);
  const back = useWizardStore((s) => s.back);
  const openHelp = useWizardStore((s) => s.openHelp);
  const [chosenTool, setChosenTool] = useState<ToolId | null>(null);
  // Derived, so the tab stays valid if the selection changed (e.g. after going back to Welcome).
  const activeTool =
    chosenTool && selectedTools.includes(chosenTool) ? chosenTool : (selectedTools[0] ?? null);

  return (
    <div className="mx-auto max-w-3xl space-y-5">
      <div>
        <h1 className="text-2xl font-semibold">{t("guide:title")}</h1>
        <p className="mt-2 text-sm leading-6 text-neutral-600 dark:text-neutral-300">
          {t("guide:intro")}
        </p>
      </div>

      {selectedTools.length > 1 && activeTool && (
        <ToolTabs tools={selectedTools} active={activeTool} onSelect={setChosenTool} />
      )}

      {activeTool ? (
        <div id={`configure-panel-${activeTool}`} role="tabpanel">
          <ToolGuide key={activeTool} tool={activeTool} />
        </div>
      ) : (
        <Alert variant="warning">{t("welcome.selectTools")}</Alert>
      )}

      <Alert
        variant="warning"
        title={
          <span className="inline-flex items-center gap-2">
            <SquareTerminal className="size-4" aria-hidden />
            {t("guide:reminders.closeTerminals.title")}
          </span>
        }
        actions={
          <Button
            variant="secondary"
            size="sm"
            onClick={() => openHelp("verify")}
            leftIcon={<CircleHelp className="size-4" aria-hidden />}
          >
            {t("guide:reminders.closeTerminals.help")}
          </Button>
        }
      >
        {t("guide:reminders.closeTerminals.body")}
      </Alert>

      <p className="text-xs text-neutral-500">{t("guide:reminders.noWrite")}</p>

      <StepFooter onBack={back} onNext={next} nextLabel={t("guide:footer.next")} />
    </div>
  );
}

function ToolTabs({
  tools,
  active,
  onSelect,
}: {
  tools: readonly ToolId[];
  active: ToolId;
  onSelect: (tool: ToolId) => void;
}) {
  const { t } = useTranslation();
  return (
    <div
      role="tablist"
      aria-label={t("guide:tabs.label")}
      className="flex gap-1 border-b border-neutral-200 dark:border-neutral-800"
    >
      {tools.map((tool) => {
        const selected = tool === active;
        return (
          <button
            key={tool}
            type="button"
            role="tab"
            aria-selected={selected}
            aria-controls={`configure-panel-${tool}`}
            onClick={() => onSelect(tool)}
            className={cn(
              "-mb-px border-b-2 px-4 py-2 text-sm font-medium transition-colors",
              selected
                ? "border-brand-600 text-brand-700 dark:text-brand-100"
                : "border-transparent text-neutral-600 hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100",
            )}
          >
            {t(`tools.${tool}`)}
          </button>
        );
      })}
    </div>
  );
}
