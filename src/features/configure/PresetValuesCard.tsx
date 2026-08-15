import { KeyRound } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Card, CopyField } from "@/components/ui";
import type { ProviderPreset, ToolId } from "@/lib/types";

import { modelHintText, protocolLabel } from "./guide-display";

export interface PresetValuesCardProps {
  tool: ToolId;
  preset: ProviderPreset;
}

/**
 * "Values to enter" card: everything the user copies into CC Switch's provider form.
 * The base URL is company-wide; model / reasoning effort are hints (PRD #1). Codex shows the
 * protocol (Responses); Claude Code shows a note that no protocol needs choosing.
 */
export function PresetValuesCard({ tool, preset }: PresetValuesCardProps) {
  const { t } = useTranslation();
  const modelHint = preset.modelHint.trim();
  const effortHint = preset.reasoningEffortHint.trim();

  return (
    <Card title={t("guide:values.title")} description={t("guide:values.description")}>
      <div className="space-y-4">
        <CopyField
          label={t("guide:values.providerName")}
          value={preset.providerName}
          mono={false}
          hint={t("guide:values.providerNameHint")}
        />
        <CopyField
          label={t("guide:values.baseUrl")}
          value={preset.baseUrl}
          hint={t("guide:values.baseUrlHint")}
        />
        {tool === "codex" ? (
          <CopyField
            label={t("guide:values.protocol")}
            value={protocolLabel(t, preset.protocol)}
            mono={false}
            hint={t("guide:values.protocolHint")}
          />
        ) : (
          <div className="space-y-1.5">
            <div className="text-sm font-medium text-neutral-700 dark:text-neutral-200">
              {t("guide:values.protocol")}
            </div>
            <p className="text-sm text-neutral-600 dark:text-neutral-400">
              {t("guide:values.protocolNoteClaude")}
            </p>
          </div>
        )}
        {modelHint ? (
          <CopyField label={t("guide:values.modelHint")} value={modelHint} />
        ) : (
          <div className="space-y-1.5">
            <div className="text-sm font-medium text-neutral-700 dark:text-neutral-200">
              {t("guide:values.modelHint")}
            </div>
            <p className="text-sm text-neutral-600 dark:text-neutral-400">
              {modelHintText(t, modelHint)}
            </p>
          </div>
        )}
        {effortHint && (
          <CopyField label={t("guide:values.reasoningEffortHint")} value={effortHint} />
        )}
        <div className="flex items-start gap-2 rounded-md border border-dashed border-neutral-300 p-3 text-sm dark:border-neutral-700">
          <KeyRound className="mt-0.5 size-4 shrink-0 text-neutral-500" aria-hidden />
          <div>
            <div className="font-medium text-neutral-700 dark:text-neutral-200">
              {t("guide:values.apiKeyRow")}
            </div>
            <p className="text-neutral-600 dark:text-neutral-400">
              {t("guide:values.apiKeyRowValue")}
            </p>
          </div>
        </div>
      </div>
    </Card>
  );
}
