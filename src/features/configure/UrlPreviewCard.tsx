import { Link, RotateCw } from "lucide-react";
import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";

import { Alert, Button, Card, CopyField, ErrorBanner } from "@/components/ui";
import { useAsync } from "@/hooks";
import { previewEffectiveUrl } from "@/lib/tauri";
import type { UrlPreview } from "@/lib/types";

import { useDebouncedValue } from "./useDebouncedValue";

export interface UrlPreviewCardProps {
  /** Company gateway base URL — pre-filled and restorable with one click. */
  presetBaseUrl: string;
}

/**
 * "URL rules preview" card (guide fault B). The input starts with the preset base URL; every
 * debounced change is sent to `preview_effective_url`, and the card shows the effective URL,
 * which rule applied (`guide:url.rule.<rule>`) and any warnings (`guide:url.warning.<w>`).
 */
export function UrlPreviewCard({ presetBaseUrl }: UrlPreviewCardProps) {
  const { t } = useTranslation();
  const inputId = useId();
  const [url, setUrl] = useState(presetBaseUrl);
  const debouncedUrl = useDebouncedValue(url);
  const preview = useAsync(previewEffectiveUrl);
  const { run } = preview;

  useEffect(() => {
    void run(debouncedUrl);
  }, [debouncedUrl, run]);

  return (
    <Card title={t("guide:url.title")} description={t("guide:url.description")}>
      <div className="space-y-3">
        <label htmlFor={inputId} className="block text-sm font-medium">
          {t("guide:url.label")}
        </label>
        <div className="flex items-stretch gap-2">
          <input
            id={inputId}
            type="text"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            autoComplete="off"
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            data-testid="url-input"
            className="focus-visible:ring-brand-500 min-w-0 flex-1 rounded-md border border-neutral-300 bg-white px-3 py-2 font-mono text-sm focus-visible:ring-2 focus-visible:outline-none dark:border-neutral-700 dark:bg-neutral-950"
          />
          <Button
            variant="secondary"
            onClick={() => setUrl(presetBaseUrl)}
            disabled={url === presetBaseUrl}
            leftIcon={<RotateCw className="size-4" aria-hidden />}
          >
            {t("guide:url.resetToPreset")}
          </Button>
        </div>

        {preview.loading && !preview.data && (
          <p className="text-sm text-neutral-500">{t("guide:url.checking")}</p>
        )}
        {preview.data && <UrlVerdict preview={preview.data} />}
        {preview.error && (
          <ErrorBanner error={preview.error} onRetry={() => void run(debouncedUrl)} />
        )}
      </div>
    </Card>
  );
}

function UrlVerdict({ preview }: { preview: UrlPreview }) {
  const { t } = useTranslation();
  const invalid = preview.rule === "invalid";
  return (
    <div className="space-y-3" data-testid="url-verdict" data-rule={preview.rule}>
      {!invalid && (
        <CopyField
          label={
            <span className="inline-flex items-center gap-1.5">
              <Link className="size-3.5" aria-hidden />
              {t("guide:url.effective")}
            </span>
          }
          value={preview.effectiveUrl}
        />
      )}
      <Alert variant={invalid ? "danger" : "info"} title={t("guide:url.ruleLabel")}>
        {t(`guide:url.rule.${preview.rule}`)}
      </Alert>
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
    </div>
  );
}
