import { SearchCheck } from "lucide-react";
import { useTranslation } from "react-i18next";

import {
  Button,
  Card,
  CopyField,
  ErrorBanner,
  FixActionButtons,
  StatusBadge,
} from "@/components/ui";
import { useAsync } from "@/hooks";
import { runEnvCheck } from "@/lib/tauri";
import type { CheckId, ConfigGuide, GuideStep } from "@/lib/types";

import { guideStepParams } from "./guide-display";

export interface GuideStepListProps {
  guide: ConfigGuide;
}

/**
 * Numbered walkthrough of the CC Switch steps returned by `get_config_guide`. Each step shows
 * `guide:<code>.title` / `.body`; a `copyValue` becomes a `CopyField`; a `verifyCheck` adds an
 * "I did this — check" button that re-runs that M1 check and shows the result inline.
 */
export function GuideStepList({ guide }: GuideStepListProps) {
  const { t } = useTranslation();
  return (
    <Card title={t("guide:steps.title")} description={t("guide:steps.description")}>
      <ol className="space-y-4" data-testid="guide-steps">
        {guide.steps.map((step, index) => (
          <GuideStepItem key={step.id} guide={guide} step={step} index={index} />
        ))}
      </ol>
    </Card>
  );
}

function GuideStepItem({
  guide,
  step,
  index,
}: {
  guide: ConfigGuide;
  step: GuideStep;
  index: number;
}) {
  const { t } = useTranslation();
  const params = guideStepParams(t, guide, step);
  return (
    <li className="flex gap-3" data-step-code={step.code}>
      <span
        aria-hidden
        className="bg-brand-50 text-brand-700 dark:bg-brand-700/20 dark:text-brand-100 flex size-7 shrink-0 items-center justify-center rounded-full text-sm font-semibold"
      >
        {index + 1}
      </span>
      <div className="min-w-0 flex-1 space-y-2">
        <h3 className="text-sm font-semibold">{t(`guide:${step.code}.title`, params)}</h3>
        <p className="text-sm leading-6 whitespace-pre-line text-neutral-700 dark:text-neutral-300">
          {t(`guide:${step.code}.body`, params)}
        </p>
        {step.copyValue && <CopyField value={step.copyValue} />}
        {step.verifyCheck && <StepCheck check={step.verifyCheck} />}
      </div>
    </li>
  );
}

/** "I did this — check" button + inline result for a step linked to an M1 check. */
function StepCheck({ check }: { check: CheckId }) {
  const { t } = useTranslation();
  const result = useAsync(runEnvCheck);
  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-3">
        <Button
          variant="secondary"
          size="sm"
          onClick={() => void result.run(check)}
          loading={result.loading}
          leftIcon={<SearchCheck className="size-4" aria-hidden />}
        >
          {result.loading ? t("guide:steps.checking") : t("guide:steps.checkDone")}
        </Button>
        {result.data && (
          <span className="inline-flex items-center gap-2 text-sm">
            <StatusBadge status={result.data.status} />
            <span data-selectable>
              {t(`checks:${result.data.code}`, {
                ...result.data.params,
                defaultValue: result.data.code,
              })}
            </span>
          </span>
        )}
      </div>
      {result.data && result.data.fixes.length > 0 && (
        <FixActionButtons fixes={result.data.fixes} onRerun={() => void result.run(check)} />
      )}
      {result.error && <ErrorBanner error={result.error} onRetry={() => void result.run(check)} />}
    </div>
  );
}
