import { ChevronDown, ChevronUp, Eye, EyeOff, Play, ShieldCheck, Stethoscope } from "lucide-react";
import { useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { Alert, Button, Card, ErrorBanner } from "@/components/ui";
import { DiagnosePanel } from "@/features/diagnose/DiagnosePanel";
import { mergeDiagnoses } from "@/features/diagnose/diagnoses";
import { useAsync } from "@/hooks";
import { cn } from "@/lib/cn";
import { diagnose, listRunningTerminals, trackEvent, verifySetup } from "@/lib/tauri";
import type { Diagnosis, TerminalProcess, ToolId, VerifyRequest, VerifyResult } from "@/lib/types";
import { useAppStore } from "@/stores/app";
import { useWizardStore } from "@/stores/wizard";

import { CliCheckView, GatewayCheckView, TerminalsAlert } from "./VerifyResultView";
import { symptomsFromResult, toolBinary, verifyTelemetryEvent } from "./verify-logic";

export interface VerifyCardProps {
  tool: ToolId;
}

const inputClass =
  "focus-visible:ring-brand-500 min-w-0 flex-1 rounded-md border border-neutral-300 bg-white px-3 py-2 text-sm focus-visible:ring-2 focus-visible:outline-none dark:border-neutral-700 dark:bg-neutral-950";

/**
 * Verification card for one tool: fresh-session CLI check, optional gateway probe (the API key
 * lives in component state only — cleared the moment the request is dispatched and gone on
 * unmount), running-terminal warning, and the diagnosis panel that opens automatically when the
 * result is not ok. Results are kept in the wizard store so the screen survives navigation.
 */
export function VerifyCard({ tool }: VerifyCardProps) {
  const { t } = useTranslation();
  const ids = useId();
  const config = useAppStore((s) => s.config);
  const snapshot = useWizardStore((s) => s.snapshot);
  const result = useWizardStore((s) => s.verifyResults[tool]);
  const setVerifyResult = useWizardStore((s) => s.setVerifyResult);

  const [gatewayEnabled, setGatewayEnabled] = useState(false);
  const [baseUrl, setBaseUrl] = useState(config?.gateway.baseUrl ?? "");
  const [model, setModel] = useState(config?.gateway.defaultModel ?? "");
  const [apiKey, setApiKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [showMissing, setShowMissing] = useState(false);
  // A failed result from an earlier visit re-opens the panel when the user comes back.
  const [panelOpen, setPanelOpen] = useState(() => (result ? !result.ok : false));
  const [extraDiagnoses, setExtraDiagnoses] = useState<Diagnosis[]>([]);
  const [terminalsOverride, setTerminalsOverride] = useState<TerminalProcess[] | null>(null);
  const startedAt = useRef(0);

  const verify = useAsync(verifySetup, {
    onSuccess: (r: VerifyResult) => {
      setVerifyResult(tool, r);
      setExtraDiagnoses([]);
      setTerminalsOverride(null);
      setPanelOpen(!r.ok);
      trackEvent(verifyTelemetryEvent(r, Date.now() - startedAt.current)).catch(() => undefined);
    },
  });
  const terminals = useAsync(listRunningTerminals, {
    onSuccess: (list: TerminalProcess[]) => setTerminalsOverride(list),
  });
  const diagnosis = useAsync(diagnose, {
    onSuccess: (list: Diagnosis[]) => setExtraDiagnoses(list),
  });

  const protocol = config?.gateway.protocol ?? "responses";
  const gatewayFieldsMissing =
    gatewayEnabled && (baseUrl.trim() === "" || model.trim() === "" || apiKey.length === 0);

  const runVerify = () => {
    if (gatewayFieldsMissing) {
      setShowMissing(true);
      return;
    }
    setShowMissing(false);
    const request: VerifyRequest = {
      tool,
      gateway: gatewayEnabled
        ? { baseUrl: baseUrl.trim(), apiKey, model: model.trim(), protocol }
        : null,
    };
    // The key is used for this one request only: drop it from state before the call resolves.
    setApiKey("");
    startedAt.current = Date.now();
    void verify.run(request);
  };

  const runDiagnosis = () => {
    if (!result) return;
    void diagnosis.run({ symptoms: symptomsFromResult(result), snapshot });
  };

  const diagnoses = result ? mergeDiagnoses(result.diagnoses, extraDiagnoses) : [];
  const shownTerminals = terminalsOverride ?? result?.runningTerminals ?? [];
  const toolName = t(`tools.${tool}`);

  return (
    <Card
      title={t("verify:card.title", { tool: toolName })}
      description={t("verify:card.description", {
        command: `${toolBinary(config, tool)} --version`,
      })}
      data-testid={`verify-card-${tool}`}
    >
      <div className="space-y-4">
        <label className="inline-flex cursor-pointer items-center gap-2 text-sm">
          <input
            type="checkbox"
            className="accent-brand-600 size-4"
            checked={gatewayEnabled}
            onChange={(e) => setGatewayEnabled(e.target.checked)}
            data-testid="gateway-toggle"
          />
          {t("verify:gateway.toggle")}
        </label>

        {gatewayEnabled && (
          <div className="space-y-3 rounded-md border border-neutral-200 p-3 dark:border-neutral-800">
            <p className="text-sm text-neutral-600 dark:text-neutral-300">
              {t("verify:gateway.description")}
            </p>
            <div className="space-y-1.5">
              <label htmlFor={`${ids}-url`} className="block text-sm font-medium">
                {t("verify:gateway.baseUrl")}
              </label>
              <input
                id={`${ids}-url`}
                type="text"
                value={baseUrl}
                onChange={(e) => setBaseUrl(e.target.value)}
                autoComplete="off"
                spellCheck={false}
                className={cn(inputClass, "w-full font-mono")}
                data-testid="gateway-url"
              />
            </div>
            <div className="space-y-1.5">
              <label htmlFor={`${ids}-model`} className="block text-sm font-medium">
                {t("verify:gateway.model")}
              </label>
              <input
                id={`${ids}-model`}
                type="text"
                value={model}
                onChange={(e) => setModel(e.target.value)}
                placeholder={t("verify:gateway.modelPlaceholder")}
                autoComplete="off"
                spellCheck={false}
                className={cn(inputClass, "w-full font-mono")}
                data-testid="gateway-model"
              />
            </div>
            <div className="space-y-1.5">
              <label htmlFor={`${ids}-key`} className="block text-sm font-medium">
                {t("verify:gateway.apiKey")}
              </label>
              <div className="flex items-stretch gap-2">
                <input
                  id={`${ids}-key`}
                  type={showKey ? "text" : "password"}
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  placeholder={t("verify:gateway.apiKeyPlaceholder")}
                  autoComplete="off"
                  autoCapitalize="off"
                  autoCorrect="off"
                  spellCheck={false}
                  className={cn(inputClass, "font-mono")}
                  data-testid="gateway-key"
                />
                <Button
                  variant="secondary"
                  onClick={() => setShowKey((v) => !v)}
                  aria-pressed={showKey}
                  leftIcon={
                    showKey ? (
                      <EyeOff className="size-4" aria-hidden />
                    ) : (
                      <Eye className="size-4" aria-hidden />
                    )
                  }
                >
                  {showKey ? t("verify:gateway.hide") : t("verify:gateway.show")}
                </Button>
              </div>
              <p className="flex items-start gap-2 text-xs text-neutral-500">
                <ShieldCheck className="text-success-500 mt-0.5 size-3.5 shrink-0" aria-hidden />
                {t("verify:gateway.keyNote")}
              </p>
            </div>
            {tool === "codex" && (
              <p className="text-xs text-neutral-500">
                {t("verify:gateway.protocol", {
                  protocol: t(`guide:values.protocolValue.${protocol}`),
                })}
              </p>
            )}
            {showMissing && gatewayFieldsMissing && (
              <Alert variant="warning">{t("verify:gateway.missingFields")}</Alert>
            )}
          </div>
        )}

        <div className="flex flex-wrap items-center gap-3">
          <Button
            onClick={runVerify}
            loading={verify.loading}
            leftIcon={<Play className="size-4" aria-hidden />}
            data-testid="verify-button"
          >
            {verify.loading
              ? t("verify:actions.verifying")
              : result
                ? t("verify:actions.verifyAgain")
                : t("verify:actions.verify")}
          </Button>
          {!result && !verify.loading && (
            <span className="text-sm text-neutral-500">{t("verify:card.notRun")}</span>
          )}
        </div>

        {verify.error && <ErrorBanner error={verify.error} onRetry={runVerify} />}

        {result && (
          <div className="space-y-3" data-testid="verify-result" data-ok={result.ok}>
            <Alert variant={result.ok ? "success" : "danger"}>
              {result.ok
                ? t("verify:results.overallOk", { tool: toolName })
                : t("verify:results.overallFail", { tool: toolName })}
            </Alert>
            <CliCheckView cli={result.cli} />
            <GatewayCheckView gateway={result.gateway} />
            <TerminalsAlert
              terminals={shownTerminals}
              onRecheck={() => void terminals.run()}
              rechecking={terminals.loading}
              rechecked={terminalsOverride !== null}
            />
            {terminals.error && <ErrorBanner error={terminals.error} />}

            {(!result.ok || diagnoses.length > 0) && (
              <div className="space-y-3">
                <div className="flex flex-wrap items-center gap-2">
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => setPanelOpen((v) => !v)}
                    aria-expanded={panelOpen}
                    aria-controls={`${ids}-diagnose`}
                    leftIcon={
                      panelOpen ? (
                        <ChevronUp className="size-4" aria-hidden />
                      ) : (
                        <ChevronDown className="size-4" aria-hidden />
                      )
                    }
                  >
                    {panelOpen
                      ? t("verify:actions.hideDiagnosis")
                      : t("verify:actions.showDiagnosis")}
                  </Button>
                </div>
                {panelOpen && (
                  <DiagnosePanel
                    id={`${ids}-diagnose`}
                    diagnoses={diagnoses}
                    snapshot={snapshot}
                    onRerun={runVerify}
                    note={
                      extraDiagnoses.length > 0
                        ? t("verify:diagnosis.merged")
                        : result.ok
                          ? undefined
                          : t("verify:diagnosis.autoOpened")
                    }
                    headerActions={
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={runDiagnosis}
                        loading={diagnosis.loading}
                        leftIcon={<Stethoscope className="size-4" aria-hidden />}
                        data-testid="run-diagnosis"
                      >
                        {diagnosis.loading
                          ? t("verify:actions.runningDiagnosis")
                          : t("verify:actions.runDiagnosis")}
                      </Button>
                    }
                  />
                )}
                {diagnosis.error && <ErrorBanner error={diagnosis.error} onRetry={runDiagnosis} />}
              </div>
            )}
          </div>
        )}
      </div>
    </Card>
  );
}
