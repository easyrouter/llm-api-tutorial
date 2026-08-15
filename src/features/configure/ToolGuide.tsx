import { useEffect } from "react";
import { useTranslation } from "react-i18next";

import { ErrorBanner, Spinner } from "@/components/ui";
import { useAsync } from "@/hooks";
import { getConfigGuide } from "@/lib/tauri";
import type { ToolId } from "@/lib/types";

import { GuideStepList } from "./GuideStepList";
import { KeyFormatCard } from "./KeyFormatCard";
import { PresetValuesCard } from "./PresetValuesCard";
import { UrlPreviewCard } from "./UrlPreviewCard";

export interface ToolGuideProps {
  tool: ToolId;
}

/**
 * Walkthrough for one tool: loads `get_config_guide(tool)` and renders the values card, the
 * numbered steps, the key format check and the URL rules preview. Mount it with `key={tool}` so
 * switching tabs remounts everything — including the key input, which must not survive a tab
 * change.
 */
export function ToolGuide({ tool }: ToolGuideProps) {
  const { t } = useTranslation();
  const guide = useAsync(getConfigGuide);
  const { run } = guide;

  useEffect(() => {
    void run(tool);
  }, [run, tool]);

  if (guide.error) {
    return <ErrorBanner error={guide.error} onRetry={() => void run(tool)} />;
  }
  if (!guide.data) {
    return (
      <div className="flex items-center gap-3 py-8 text-sm text-neutral-500">
        <Spinner size="sm" />
        {t("guide:loading", { tool: t(`tools.${tool}`) })}
      </div>
    );
  }

  return (
    <div className="space-y-5" data-testid={`tool-guide-${tool}`}>
      <PresetValuesCard tool={tool} preset={guide.data.preset} />
      <GuideStepList guide={guide.data} />
      <KeyFormatCard />
      <UrlPreviewCard presetBaseUrl={guide.data.preset.baseUrl} />
    </div>
  );
}
