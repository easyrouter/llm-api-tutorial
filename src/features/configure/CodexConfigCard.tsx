import { Check, Copy, RotateCw } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { Alert, Button, Card } from "@/components/ui";
import { useAsync, useCopy } from "@/hooks";
import { cn } from "@/lib/cn";
import { getCodexConfigTemplate } from "@/lib/tauri";
import type { AutoCompactScope, ProviderPreset } from "@/lib/types";

/** Must match `guide::CODEX_CONFIG_KEY_PLACEHOLDER` on the Rust side. */
export const CODEX_CONFIG_KEY_PLACEHOLDER = "<API-KEY>";

/** Delay before an input change regenerates the template. */
const REGENERATE_DELAY_MS = 250;

const SCOPES: readonly AutoCompactScope[] = ["body_after_prefix", "total"];

export interface CodexConfigCardProps {
  preset: ProviderPreset;
  providerName: string;
  baseUrl: string;
  model: string;
  /** Only read at copy time to substitute the placeholder — never rendered. */
  apiKey: string;
}

/**
 * Recommended Codex `config.toml` (codex tab only): rendered by the Rust core from the live
 * provider values and shown in an *editable* text box, so users can tune any default (e.g. a
 * stricter `model_auto_compact_token_limit`) before pasting it into the CC Switch provider's
 * config editor. The template carries an `<API-KEY>` placeholder; the real key is substituted
 * only into the copied text, never shown on screen. Manual edits freeze auto-regeneration
 * until the user explicitly restores the generated template.
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
    onSuccess: (toml: string) => {
      if (!editedRef.current) setText(toml);
    },
  });
  const { run } = template;
  const reasoningEffort = preset.reasoningEffortHint;

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
    const key = apiKey.trim();
    const out = key === "" ? text : text.split(CODEX_CONFIG_KEY_PLACEHOLDER).join(key);
    void copy(out);
  };

  return (
    <Card title={t("guide:config.title")} description={t("guide:config.description")}>
      <div className="space-y-4">
        <fieldset className="space-y-2">
          <legend className="text-sm font-medium">{t("guide:config.scopeLabel")}</legend>
          <p className="text-xs text-neutral-500">{t("guide:config.scopeHint")}</p>
          {SCOPES.map((value) => (
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
                <span className="mt-0.5 block text-neutral-600 dark:text-neutral-400">
                  {t(`guide:config.scope.${value}.explanation`)}
                </span>
              </span>
            </label>
          ))}
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
      </div>
    </Card>
  );
}
