import { Check, Copy, FileCog, History, RotateCw } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { Alert, Button, Card, ConfirmDialog, ErrorBanner, KeyValueList } from "@/components/ui";
import { HelpLink } from "@/features/help/HelpLink";
import { useAsync, useCopy } from "@/hooks";
import { cn } from "@/lib/cn";
import { maskSecret } from "@/lib/format";
import {
  applyCodexConfig,
  codexConfigStatus,
  getCodexConfigTemplate,
  restoreCodexConfig,
} from "@/lib/tauri";
import type {
  AutoCompactScope,
  CodexConfigApplyResult,
  CodexConfigTemplate,
  ProviderPreset,
} from "@/lib/types";

/** Must match `guide::CODEX_CONFIG_KEY_PLACEHOLDER` on the Rust side. */
export const CODEX_CONFIG_KEY_PLACEHOLDER = "<API-KEY>";

/** Delay before an input change regenerates the template / refreshes the file status. */
const REGENERATE_DELAY_MS = 250;

const SCOPES: readonly AutoCompactScope[] = ["body_after_prefix", "total"];

/** Help section that explains what every one-click action does behind the scenes. */
const HELP_SECTION = "one-click";

export interface CodexConfigCardProps {
  preset: ProviderPreset;
  providerName: string;
  baseUrl: string;
  model: string;
  /** Only read at copy / apply time to substitute the placeholder — never rendered. */
  apiKey: string;
}

/** The template with the real key substituted for the placeholder (memory only). */
function substituteKey(template: string, apiKey: string): string {
  const key = apiKey.trim();
  return key === "" ? template : template.split(CODEX_CONFIG_KEY_PLACEHOLDER).join(key);
}

/** The template with the key replaced by its masked form — safe to render. */
function maskedPreview(template: string, apiKey: string): string {
  const key = apiKey.trim();
  return key === "" ? template : template.split(CODEX_CONFIG_KEY_PLACEHOLDER).join(maskSecret(key));
}

/** The two limits the template embeds, formatted for the card's copy. */
interface TemplateLimits {
  /** `model_auto_compact_token_limit` as typed in the file, e.g. `300000`. */
  limit: string;
  /** `model_context_window` rounded to thousands, e.g. `372k`. */
  context: string;
}

/**
 * `null` until the first template response (and for a missing one), so the strings that quote
 * a number are left out instead of rendering `undefined` / `NaN`. Re-runs keep the previous
 * response's numbers — `useAsync` holds `data` while a new run is in flight.
 */
function templateLimits(template: CodexConfigTemplate | null): TemplateLimits | null {
  if (!template) return null;
  return {
    limit: String(template.modelAutoCompactTokenLimit),
    context: `${Math.round(template.modelContextWindow / 1000)}k`,
  };
}

/**
 * Recommended Codex `config.toml` (codex tab only): rendered by the Rust core from the live
 * provider values (`CodexConfigTemplate`: the toml plus the two limits it embeds, which the
 * card's copy quotes instead of hard-coding them) and shown in an *editable* text box, so users
 * can tune any default (e.g. a stricter `model_auto_compact_token_limit`) before applying it.
 * The primary action writes the file to `~/.codex/config.toml` (show-before-run dialog with a
 * masked preview, automatic backup, one-click restore); copying stays available for users who
 * prefer to paste it into CC Switch themselves. The template carries an `<API-KEY>`
 * placeholder; the real key is substituted only into the copied / written text, never shown on
 * screen. Manual edits freeze auto-regeneration until the user explicitly restores the
 * generated template.
 */
export function CodexConfigCard({
  preset,
  providerName,
  baseUrl,
  model,
  apiKey,
}: CodexConfigCardProps) {
  const { t } = useTranslation();
  const textId = useId();
  const [scope, setScope] = useState<AutoCompactScope>("body_after_prefix");
  const [text, setText] = useState("");
  const [edited, setEdited] = useState(false);
  const editedRef = useRef(false);
  const { copied, failed, copy } = useCopy();

  const template = useAsync(getCodexConfigTemplate, {
    onSuccess: (response: CodexConfigTemplate) => {
      if (!editedRef.current) setText(response.toml);
    },
  });
  const { run } = template;
  const reasoningEffort = preset.reasoningEffortHint;
  const limits = templateLimits(template.data);
  const liveModel = model.trim();

  useEffect(() => {
    if (edited) return undefined;
    const timer = window.setTimeout(() => {
      void run({
        providerName: providerName.trim(),
        baseUrl: baseUrl.trim(),
        model: model.trim(),
        reasoningEffort: reasoningEffort.trim(),
        autoCompactScope: scope,
      });
    }, REGENERATE_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [providerName, baseUrl, model, reasoningEffort, scope, edited, run]);

  const markEdited = (value: string) => {
    setText(value);
    setEdited(true);
    editedRef.current = true;
  };

  const restoreTemplate = () => {
    setEdited(false);
    editedRef.current = false;
    void run({
      providerName: providerName.trim(),
      baseUrl: baseUrl.trim(),
      model: model.trim(),
      reasoningEffort: reasoningEffort.trim(),
      autoCompactScope: scope,
    });
  };

  const copyConfig = () => {
    void copy(substituteKey(text, apiKey));
  };

  /**
   * The `body_after_prefix` explanation quotes the context window, so it waits for the numbers;
   * `total` has nothing to interpolate. (`context` is an interpolation value here — the key has
   * no i18next context variants, so the lookup falls through to the base string.)
   */
  const scopeExplanation = (value: AutoCompactScope): string | null => {
    if (value !== "body_after_prefix") return t(`guide:config.scope.${value}.explanation`);
    return limits
      ? t("guide:config.scope.body_after_prefix.explanation", { context: limits.context })
      : null;
  };

  return (
    <Card
      title={t("guide:config.title")}
      description={t("guide:config.description", {
        model: liveModel === "" ? t("guide:config.modelFallback") : liveModel,
      })}
      data-testid="codex-config-card"
    >
      <div className="space-y-4">
        <fieldset className="space-y-2">
          <legend className="text-sm font-medium">{t("guide:config.scopeLabel")}</legend>
          {limits && (
            <p className="text-xs text-neutral-500" data-testid="config-scope-hint">
              {t("guide:config.scopeHint", { limit: limits.limit })}
            </p>
          )}
          {SCOPES.map((value) => {
            const explanation = scopeExplanation(value);
            return (
              <label
                key={value}
                className="flex cursor-pointer items-start gap-2 rounded-md border border-neutral-200 p-2.5 text-sm dark:border-neutral-800"
              >
                <input
                  type="radio"
                  name={`${textId}-scope`}
                  className="accent-brand-600 mt-1 size-4"
                  checked={scope === value}
                  onChange={() => setScope(value)}
                  data-testid={`config-scope-${value}`}
                />
                <span>
                  <span className="font-mono font-medium">
                    {t(`guide:config.scope.${value}.label`)}
                  </span>
                  {explanation && (
                    <span
                      className="mt-0.5 block text-neutral-600 dark:text-neutral-400"
                      data-testid={`config-scope-${value}-explanation`}
                    >
                      {explanation}
                    </span>
                  )}
                </span>
              </label>
            );
          })}
        </fieldset>

        <div className="space-y-1.5">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <label htmlFor={textId} className="text-sm font-medium">
              {t("guide:config.templateLabel")}
            </label>
            <div className="flex items-center gap-2">
              {edited && (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={restoreTemplate}
                  leftIcon={<RotateCw className="size-4" aria-hidden />}
                  data-testid="config-toml-restore"
                >
                  {t("guide:config.regenerate")}
                </Button>
              )}
              <Button
                variant="secondary"
                size="sm"
                onClick={copyConfig}
                disabled={text === ""}
                leftIcon={
                  copied ? (
                    <Check className="text-success-500 size-4" aria-hidden />
                  ) : (
                    <Copy className="size-4" aria-hidden />
                  )
                }
                data-testid="config-toml-copy"
              >
                {copied ? t("actions.copied") : t("actions.copy")}
              </Button>
            </div>
          </div>
          <textarea
            id={textId}
            value={text}
            onChange={(e) => markEdited(e.target.value)}
            rows={16}
            autoComplete="off"
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            placeholder={template.loading && text === "" ? t("guide:config.loading") : undefined}
            data-testid="config-toml-input"
            className={cn(
              "focus-visible:ring-brand-500 w-full rounded-md border border-neutral-300 bg-white px-3 py-2 font-mono text-xs leading-5 focus-visible:ring-2 focus-visible:outline-none dark:border-neutral-700 dark:bg-neutral-950",
            )}
          />
          {failed && <p className="text-danger-500 text-xs">{t("ui.copyFailed")}</p>}
          <p className="text-xs text-neutral-500">
            {apiKey.length > 0 ? t("guide:config.keySubstitution") : t("guide:config.keyMissing")}
          </p>
          {edited && (
            <Alert variant="info" data-testid="config-toml-edited">
              {t("guide:config.edited")}
            </Alert>
          )}
        </div>

        <ApplySection text={text} apiKey={apiKey} />
      </div>
    </Card>
  );
}

/**
 * "Apply to this machine" — the one-click write of `~/.codex/config.toml`. Confirm-first: the
 * dialog shows the target path, whether a file exists (and that it gets backed up), a masked
 * preview of the exact content, and the CC Switch caveat. Restore puts the newest backup back.
 */
function ApplySection({ text, apiKey }: { text: string; apiKey: string }) {
  const { t } = useTranslation();
  const [applyOpen, setApplyOpen] = useState(false);
  const [restoreOpen, setRestoreOpen] = useState(false);
  const [needsKey, setNeedsKey] = useState(false);
  const [applied, setApplied] = useState<CodexConfigApplyResult | null>(null);
  const [restored, setRestored] = useState<CodexConfigApplyResult | null>(null);

  const status = useAsync(codexConfigStatus);
  const { run: refreshStatus } = status;
  const hasKey = apiKey.trim() !== "";
  const content = substituteKey(text, apiKey);

  // Status line: compare the live file with the final content (key substituted) when a key is
  // present; without one the template cannot match, so only existence is reported.
  useEffect(() => {
    const timer = window.setTimeout(() => {
      void refreshStatus(hasKey && text !== "" ? content : null);
    }, REGENERATE_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [content, hasKey, text, refreshStatus]);

  const apply = useAsync(applyCodexConfig, {
    onSuccess: (result) => {
      setApplyOpen(false);
      setRestored(null);
      setApplied(result);
      void refreshStatus(content);
    },
  });
  const restore = useAsync(restoreCodexConfig, {
    onSuccess: (result) => {
      setRestoreOpen(false);
      setApplied(null);
      setRestored(result);
      void refreshStatus(hasKey ? content : null);
    },
  });

  const requestApply = () => {
    setApplied(null);
    setRestored(null);
    if (!hasKey) {
      setNeedsKey(true);
      return;
    }
    setNeedsKey(false);
    apply.reset();
    setApplyOpen(true);
  };

  const requestRestore = () => {
    setApplied(null);
    setRestored(null);
    restore.reset();
    setRestoreOpen(true);
  };

  const backups = status.data?.backups ?? [];
  const statusText = (() => {
    const s = status.data;
    if (!s) return null;
    if (!s.exists) return t("guide:config.apply.status.missing");
    if (s.matchesTemplate === true) return t("guide:config.apply.status.upToDate");
    if (s.matchesTemplate === false) {
      return t("guide:config.apply.status.differs", { n: backups.length });
    }
    return t("guide:config.apply.status.exists", { n: backups.length });
  })();

  return (
    <div className="space-y-3 border-t border-neutral-200 pt-4 dark:border-neutral-800">
      <div className="flex flex-wrap items-center gap-3">
        <Button
          onClick={requestApply}
          disabled={text === ""}
          leftIcon={<FileCog className="size-4" aria-hidden />}
          data-testid="config-apply"
        >
          {t("guide:config.apply.button")}
        </Button>
        {backups.length > 0 && (
          <Button
            variant="secondary"
            size="sm"
            onClick={requestRestore}
            leftIcon={<History className="size-4" aria-hidden />}
            data-testid="config-restore"
          >
            {t("guide:config.apply.restore")}
          </Button>
        )}
        <HelpLink sectionId={HELP_SECTION}>{t("guide:config.apply.whatItDoes")}</HelpLink>
      </div>
      {statusText && (
        <p className="text-xs text-neutral-500" data-testid="config-status">
          {t("guide:config.apply.status.label")}
          {statusText}
        </p>
      )}
      {needsKey && !hasKey && (
        <Alert variant="warning" data-testid="config-apply-needs-key">
          {t("guide:config.apply.needsKey")}
        </Alert>
      )}
      {applied && (
        <Alert variant="success" data-testid="config-applied">
          {applied.backupPath
            ? t("guide:config.apply.appliedWithBackup", {
                path: applied.path,
                backup: applied.backupPath,
              })
            : t("guide:config.apply.applied", { path: applied.path })}
        </Alert>
      )}
      {restored && (
        <Alert variant="success" data-testid="config-restored">
          {t("guide:config.apply.restored", { path: restored.path })}
        </Alert>
      )}
      {status.error && <ErrorBanner error={status.error} />}

      <ConfirmDialog
        open={applyOpen}
        title={t("guide:config.apply.confirmTitle")}
        description={t("guide:config.apply.confirmBody")}
        confirmLabel={t("guide:config.apply.confirmAction")}
        confirmLoading={apply.loading}
        onConfirm={() => void apply.run({ content })}
        onCancel={() => setApplyOpen(false)}
      >
        <div className="space-y-3">
          <KeyValueList
            items={[
              {
                label: t("guide:config.apply.targetPath"),
                value: status.data?.path ?? t("guide:config.apply.defaultPath"),
                mono: true,
              },
              {
                label: t("guide:config.apply.existing"),
                value: status.data?.exists
                  ? t("guide:config.apply.existingYes")
                  : t("guide:config.apply.existingNo"),
              },
            ]}
          />
          <div className="space-y-1">
            <div className="text-xs font-medium">{t("guide:config.apply.previewLabel")}</div>
            <pre
              className="max-h-56 overflow-auto rounded-md border border-neutral-200 bg-neutral-50 p-2 font-mono text-xs leading-5 whitespace-pre-wrap dark:border-neutral-800 dark:bg-neutral-950"
              data-testid="config-apply-preview"
            >
              {maskedPreview(text, apiKey)}
            </pre>
          </div>
          <p className="text-xs text-neutral-600 dark:text-neutral-400">
            {t("guide:config.apply.ccSwitchNote")}
          </p>
          <HelpLink sectionId={HELP_SECTION}>{t("guide:config.apply.whatItDoes")}</HelpLink>
          {apply.error && <ErrorBanner error={apply.error} />}
        </div>
      </ConfirmDialog>

      <ConfirmDialog
        open={restoreOpen}
        danger
        title={t("guide:config.apply.restoreTitle")}
        description={t("guide:config.apply.restoreBody", { backup: backups[0] ?? "" })}
        confirmLabel={t("guide:config.apply.restoreAction")}
        confirmLoading={restore.loading}
        onConfirm={() => void restore.run()}
        onCancel={() => setRestoreOpen(false)}
      >
        {restore.error && <ErrorBanner error={restore.error} />}
      </ConfirmDialog>
    </div>
  );
}
