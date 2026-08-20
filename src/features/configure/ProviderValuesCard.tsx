import {
  Check,
  CircleAlert,
  Eye,
  EyeOff,
  Info,
  Pencil,
  PlugZap,
  RotateCw,
  ShieldCheck,
} from "lucide-react";
import { useId, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { Alert, Button, Card, CopyField, ErrorBanner } from "@/components/ui";
import { GatewayCheckView, ModelListCheckView } from "@/features/verify/VerifyResultView";
import { useAsync } from "@/hooks";
import { cn } from "@/lib/cn";
import { testConnectivity } from "@/lib/tauri";
import type {
  ConnectivityReport,
  GuideBranch,
  KeyIssue,
  KeyValidation,
  Protocol,
  ProviderPreset,
  ToolId,
  UrlPreview,
} from "@/lib/types";

import { modelHintText, protocolLabel } from "./guide-display";
import { splitKeyIssues } from "./key-issues";

const inputClass =
  "focus-visible:ring-brand-500 min-w-0 flex-1 rounded-md border border-neutral-300 bg-white px-3 py-2 text-sm focus-visible:ring-2 focus-visible:outline-none dark:border-neutral-700 dark:bg-neutral-950";

export interface ProviderValuesCardProps {
  tool: ToolId;
  preset: ProviderPreset;
  /** Codex account path; on `chatgpt_login` the key only feeds the config.toml template. */
  branch?: GuideBranch | null;
  /** Wire protocol used for the live probe (Claude Code always speaks Anthropic Messages). */
  probeProtocol: Protocol;
  providerName: string;
  onProviderNameChange: (value: string) => void;
  baseUrl: string;
  onBaseUrlChange: (value: string) => void;
  model: string;
  onModelChange: (value: string) => void;
  /** Lives only in the parent's state; discarded when the tab unmounts. */
  apiKey: string;
  onApiKeyChange: (value: string) => void;
}

/**
 * "Values to enter" card — the editable provider panel. Every value the user copies into CC
 * Switch can be edited in place (edit button per row), the API key is typed here once, and
 * "Test connectivity" sends one combined check to the Rust core: URL rules + key format +
 * (when both allow it) a live gateway probe. This replaces the former separate "key format
 * check" and "URL rules preview" cards.
 */
export function ProviderValuesCard({
  tool,
  preset,
  branch = null,
  probeProtocol,
  providerName,
  onProviderNameChange,
  baseUrl,
  onBaseUrlChange,
  model,
  onModelChange,
  apiKey,
  onApiKeyChange,
}: ProviderValuesCardProps) {
  const { t } = useTranslation();
  const keyId = useId();
  const [keyVisible, setKeyVisible] = useState(false);
  const connectivity = useAsync(testConnectivity);
  const { run, reset } = connectivity;

  const effortHint = preset.reasoningEffortHint.trim();
  const testReady = baseUrl.trim() !== "" && apiKey.length > 0;

  const runTest = () => {
    void run({
      baseUrl: baseUrl.trim(),
      apiKey,
      model: model.trim(),
      protocol: probeProtocol,
    });
  };

  const clearKey = () => {
    onApiKeyChange("");
    reset();
  };

  return (
    <Card
      title={t("guide:values.title")}
      description={
        branch === "chatgpt_login"
          ? t("guide:values.descriptionLogin")
          : t("guide:values.description")
      }
    >
      <div className="space-y-4">
        <EditableRow
          label={t("guide:values.providerName")}
          value={providerName}
          presetValue={preset.providerName}
          onChange={onProviderNameChange}
          mono={false}
          hint={t("guide:values.providerNameHint")}
          testId="provider-name"
        />
        <EditableRow
          label={t("guide:values.baseUrl")}
          value={baseUrl}
          presetValue={preset.baseUrl}
          onChange={onBaseUrlChange}
          hint={t("guide:values.baseUrlHint")}
          testId="base-url"
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
        <EditableRow
          label={t("guide:values.modelHint")}
          value={model}
          presetValue={preset.modelHint}
          onChange={onModelChange}
          hint={model.trim() === "" ? modelHintText(t, "") : undefined}
          testId="model"
        />
        {effortHint && (
          <CopyField label={t("guide:values.reasoningEffortHint")} value={effortHint} />
        )}

        <div className="space-y-1.5">
          <label htmlFor={keyId} className="block text-sm font-medium">
            {t("guide:values.apiKeyRow")}
          </label>
          <div className="flex items-stretch gap-2">
            <input
              id={keyId}
              type={keyVisible ? "text" : "password"}
              value={apiKey}
              onChange={(e) => onApiKeyChange(e.target.value)}
              placeholder={t("guide:key.placeholder")}
              autoComplete="off"
              autoCapitalize="off"
              autoCorrect="off"
              spellCheck={false}
              data-testid="key-input"
              className={cn(inputClass, "font-mono")}
            />
            <Button
              variant="secondary"
              onClick={() => setKeyVisible((v) => !v)}
              aria-pressed={keyVisible}
              leftIcon={
                keyVisible ? (
                  <EyeOff className="size-4" aria-hidden />
                ) : (
                  <Eye className="size-4" aria-hidden />
                )
              }
            >
              {keyVisible ? t("guide:key.hide") : t("guide:key.show")}
            </Button>
            <Button variant="ghost" onClick={clearKey} disabled={apiKey.length === 0}>
              {t("guide:key.clear")}
            </Button>
          </div>
          <p className="flex items-start gap-2 text-xs text-neutral-500">
            <ShieldCheck className="text-success-500 mt-0.5 size-3.5 shrink-0" aria-hidden />
            {t("guide:key.privacy")}
          </p>
        </div>

        <div className="space-y-3 border-t border-neutral-200 pt-4 dark:border-neutral-800">
          <div className="flex flex-wrap items-center gap-3">
            <Button
              onClick={runTest}
              disabled={!testReady}
              loading={connectivity.loading}
              leftIcon={<PlugZap className="size-4" aria-hidden />}
              data-testid="test-connectivity"
            >
              {connectivity.loading ? t("guide:test.testing") : t("guide:test.button")}
            </Button>
            <span className="text-sm text-neutral-500">
              {testReady ? t("guide:test.description") : t("guide:test.needs")}
            </span>
          </div>
          {connectivity.error && <ErrorBanner error={connectivity.error} onRetry={runTest} />}
          {connectivity.data && <ConnectivityResultView report={connectivity.data} />}
        </div>
      </div>
    </Card>
  );
}

/**
 * One provider value: a copyable read-only row with an edit button that swaps it for an input
 * (plus a one-click reset to the company preset).
 */
function EditableRow({
  label,
  value,
  presetValue,
  onChange,
  mono = true,
  hint,
  testId,
}: {
  label: string;
  value: string;
  presetValue: string;
  onChange: (value: string) => void;
  mono?: boolean;
  hint?: ReactNode;
  testId: string;
}) {
  const { t } = useTranslation();
  const inputId = useId();
  const [editing, setEditing] = useState(false);

  if (!editing) {
    return (
      <div data-testid={`row-${testId}`}>
        <CopyField
          label={label}
          value={value}
          mono={mono}
          hint={hint}
          actions={
            <Button
              variant="secondary"
              onClick={() => setEditing(true)}
              leftIcon={<Pencil className="size-4" aria-hidden />}
              data-testid={`edit-${testId}`}
              className="shrink-0"
            >
              {t("guide:values.edit")}
            </Button>
          }
        />
      </div>
    );
  }

  return (
    <div className="space-y-1.5" data-testid={`row-${testId}`}>
      <label htmlFor={inputId} className="block text-sm font-medium">
        {label}
      </label>
      <div className="flex items-stretch gap-2">
        <input
          id={inputId}
          type="text"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          autoComplete="off"
          autoCapitalize="off"
          autoCorrect="off"
          spellCheck={false}
          data-testid={`input-${testId}`}
          className={cn(inputClass, mono && "font-mono")}
        />
        <Button
          variant="secondary"
          onClick={() => onChange(presetValue)}
          disabled={value === presetValue}
          leftIcon={<RotateCw className="size-4" aria-hidden />}
        >
          {t("guide:values.resetToPreset")}
        </Button>
        <Button
          variant="primary"
          onClick={() => setEditing(false)}
          leftIcon={<Check className="size-4" aria-hidden />}
          data-testid={`done-${testId}`}
        >
          {t("guide:values.doneEditing")}
        </Button>
      </div>
      {hint && <p className="text-xs text-neutral-500">{hint}</p>}
    </div>
  );
}

/**
 * Combined verdict: URL rules, key format, the model list and the protocol probe (or why
 * nothing was sent). The model list is shown first: it is the fast check, so when the protocol
 * probe fails it already says whether the address and key themselves are fine.
 */
function ConnectivityResultView({ report }: { report: ConnectivityReport }) {
  const { t } = useTranslation();
  const sent = report.gateway !== null || report.models !== null;
  return (
    <div className="space-y-3" data-testid="connectivity-result">
      <UrlVerdict preview={report.url} />
      <KeyVerdict validation={report.key} />
      {report.models && <ModelListCheckView list={report.models} />}
      {report.gateway && <GatewayCheckView gateway={report.gateway} />}
      {report.models?.gateway.ok && report.gateway && !report.gateway.ok && (
        <Alert variant="info" data-testid="connectivity-model-hint">
          {t("guide:test.modelsOkProbeFailed")}
        </Alert>
      )}
      {!sent && (
        <Alert variant="warning" data-testid="connectivity-not-sent">
          {t("guide:test.notSent")}
        </Alert>
      )}
    </div>
  );
}

function UrlVerdict({ preview }: { preview: UrlPreview }) {
  const { t } = useTranslation();
  const invalid = preview.rule === "invalid";
  return (
    <div className="space-y-2" data-testid="url-verdict" data-rule={preview.rule}>
      {invalid ? (
        <Alert variant="danger">{t("guide:url.rule.invalid")}</Alert>
      ) : (
        <>
          <CopyField label={t("guide:url.effective")} value={preview.effectiveUrl} />
          {preview.rule !== "already_versioned" && (
            <Alert variant="info">{t(`guide:url.rule.${preview.rule}`)}</Alert>
          )}
          {preview.warnings.length > 0 && (
            <Alert variant="warning" title={t("guide:url.warningsLabel")}>
              <ul className="list-disc space-y-1 pl-5">
                {preview.warnings.map((w) => (
                  <li key={w} data-warning={w}>
                    {t(`guide:url.warning.${w}`)}
                  </li>
                ))}
              </ul>
            </Alert>
          )}
        </>
      )}
    </div>
  );
}

function KeyVerdict({ validation }: { validation: KeyValidation }) {
  const { t } = useTranslation();
  const { blocking, hints } = splitKeyIssues(validation);

  return (
    <div className="space-y-2" data-testid="key-verdict" data-valid={validation.valid}>
      {validation.valid ? (
        <Alert variant="success">{t("guide:key.valid", { length: validation.length })}</Alert>
      ) : (
        <Alert variant="danger" title={t("guide:key.invalid")}>
          <IssueList issues={blocking} kind="blocking" />
        </Alert>
      )}
      {hints.length > 0 && (
        <Alert variant="info" title={t("guide:key.hintLabel")}>
          <IssueList issues={hints} kind="hint" />
        </Alert>
      )}
    </div>
  );
}

function IssueList({ issues, kind }: { issues: KeyIssue[]; kind: "blocking" | "hint" }) {
  const { t } = useTranslation();
  const Icon = kind === "blocking" ? CircleAlert : Info;
  return (
    <ul className="space-y-1">
      {issues.map((issue) => (
        <li
          key={issue}
          data-issue={issue}
          data-kind={kind}
          className={cn("flex items-start gap-2", kind === "blocking" && "font-medium")}
        >
          <Icon
            className={cn(
              "mt-1 size-3.5 shrink-0",
              kind === "blocking" ? "text-danger-500" : "text-brand-600",
            )}
            aria-hidden
          />
          <span>{t(`guide:key.issue.${issue}`)}</span>
        </li>
      ))}
    </ul>
  );
}
