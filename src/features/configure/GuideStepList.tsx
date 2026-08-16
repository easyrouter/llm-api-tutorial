import { ListTree, SearchCheck } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import {
  Alert,
  Button,
  Card,
  CopyField,
  ErrorBanner,
  FixActionButtons,
  StatusBadge,
} from "@/components/ui";
import { useAsync } from "@/hooks";
import { listGatewayModels, runEnvCheck } from "@/lib/tauri";
import type { CheckId, ConfigGuide, GuideStep, ModelList, Protocol } from "@/lib/types";

import { guideStepParams, liveCopyValue, type LiveProviderValues } from "./guide-display";

/** Connection values the model-list fetch uses (from the editable provider card). */
export interface ModelProbeValues {
  baseUrl: string;
  apiKey: string;
  protocol: Protocol;
}

export interface GuideStepListProps {
  guide: ConfigGuide;
  /** Live values from the editable provider card; steps fall back to the preset without it. */
  live?: LiveProviderValues;
  /** When set, the `set_model` step offers "fetch the model list from the gateway". */
  probe?: ModelProbeValues;
  onPickModel?: (model: string) => void;
}

/**
 * Numbered walkthrough of the CC Switch steps returned by `get_config_guide`. Each step shows
 * `guide:<code>.title` / `.body`; a `copyValue` becomes a `CopyField` (live edits from the
 * provider card win over the preset); a `verifyCheck` adds an "I did this — check" button; the
 * `set_model` step can fetch the gateway's model list and fill the model with one click.
 */
export function GuideStepList({ guide, live, probe, onPickModel }: GuideStepListProps) {
  const { t } = useTranslation();
  return (
    <Card title={t("guide:steps.title")} description={t("guide:steps.description")}>
      <ol className="space-y-4" data-testid="guide-steps">
        {guide.steps.map((step, index) => (
          <GuideStepItem
            key={step.id}
            guide={guide}
            step={step}
            index={index}
            live={live}
            probe={probe}
            onPickModel={onPickModel}
          />
        ))}
      </ol>
    </Card>
  );
}

function GuideStepItem({
  guide,
  step,
  index,
  live,
  probe,
  onPickModel,
}: {
  guide: ConfigGuide;
  step: GuideStep;
  index: number;
  live?: LiveProviderValues;
  probe?: ModelProbeValues;
  onPickModel?: (model: string) => void;
}) {
  const { t } = useTranslation();
  const params = guideStepParams(t, guide, step, live);
  const copyValue = liveCopyValue(step, live);
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
        {copyValue && <CopyField value={copyValue} />}
        {step.code === "set_model" && probe && onPickModel && (
          <ModelListFetch probe={probe} onPick={onPickModel} />
        )}
        {step.verifyCheck && <StepCheck check={step.verifyCheck} />}
      </div>
    </li>
  );
}

/** "Fetch the model list" button + clickable result chips for the `set_model` step. */
function ModelListFetch({
  probe,
  onPick,
}: {
  probe: ModelProbeValues;
  onPick: (model: string) => void;
}) {
  const { t } = useTranslation();
  const [picked, setPicked] = useState<string | null>(null);
  const models = useAsync(listGatewayModels);
  const ready = probe.baseUrl.trim() !== "" && probe.apiKey.length > 0;

  const fetchModels = () => {
    setPicked(null);
    void models.run({
      baseUrl: probe.baseUrl.trim(),
      apiKey: probe.apiKey,
      model: "",
      protocol: probe.protocol,
    });
  };

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-3">
        <Button
          variant="secondary"
          size="sm"
          onClick={fetchModels}
          disabled={!ready}
          loading={models.loading}
          leftIcon={<ListTree className="size-4" aria-hidden />}
          data-testid="fetch-models"
        >
          {models.loading ? t("guide:models.fetching") : t("guide:models.fetch")}
        </Button>
        <span className="text-xs text-neutral-500">
          {ready ? t("guide:models.hint") : t("guide:models.needsKey")}
        </span>
      </div>
      {models.error && <ErrorBanner error={models.error} onRetry={fetchModels} />}
      {models.data && (
        <ModelListResult
          list={models.data}
          picked={picked}
          onPick={(model) => {
            setPicked(model);
            onPick(model);
          }}
        />
      )}
    </div>
  );
}

function ModelListResult({
  list,
  picked,
  onPick,
}: {
  list: ModelList;
  picked: string | null;
  onPick: (model: string) => void;
}) {
  const { t } = useTranslation();
  if (!list.gateway.ok) {
    return (
      <Alert
        variant="danger"
        title={
          list.gateway.errorClass ? t(`verify:errorClass.${list.gateway.errorClass}`) : undefined
        }
        data-testid="models-error"
      >
        {list.gateway.message}
      </Alert>
    );
  }
  if (list.models.length === 0) {
    return (
      <Alert variant="info" data-testid="models-empty">
        {t("guide:models.empty")}
      </Alert>
    );
  }
  return (
    <div className="space-y-1.5" data-testid="models-list">
      <div className="text-xs text-neutral-500">{t("guide:models.listLabel")}</div>
      <ul className="flex flex-wrap gap-1.5">
        {list.models.map((model) => (
          <li key={model}>
            <button
              type="button"
              onClick={() => onPick(model)}
              data-model={model}
              className="hover:border-brand-500 hover:text-brand-700 dark:hover:text-brand-100 rounded-full border border-neutral-300 bg-white/60 px-2.5 py-1 font-mono text-xs transition-colors dark:border-neutral-700 dark:bg-neutral-900/60"
            >
              {model}
            </button>
          </li>
        ))}
      </ul>
      {picked && (
        <p className="text-success-500 text-sm" data-testid="model-picked">
          {t("guide:models.picked", { model: picked })}
        </p>
      )}
    </div>
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
