import { Send } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Alert, Button, Card, ConfirmDialog, CopyField, ErrorBanner } from "@/components/ui";
import { useAsync } from "@/hooks";
import { openCcSwitchImport, previewCcSwitchImport } from "@/lib/tauri";
import type { CcSwitchImportRequest, ToolId } from "@/lib/types";

export interface CcSwitchImportCardProps {
  tool: ToolId;
  providerName: string;
  baseUrl: string;
  model: string;
  /** Never stored here; embedded in the deep link only after the user confirms the preview. */
  apiKey: string;
}

/**
 * One-click provider hand-off to the local CC Switch via its `ccswitch://v1/import` deep link
 * (ADR-0006). The flow is confirm-twice by design: this card shows the masked link first
 * ("show before run", hard rule 4), then CC Switch shows its own import dialog and does its
 * own writing — this app never touches `~/.cc-switch`. The manual steps below stay available.
 */
export function CcSwitchImportCard({
  tool,
  providerName,
  baseUrl,
  model,
  apiKey,
}: CcSwitchImportCardProps) {
  const { t } = useTranslation();
  const [dialogOpen, setDialogOpen] = useState(false);
  const [sent, setSent] = useState(false);
  const preview = useAsync(previewCcSwitchImport, { onSuccess: () => setDialogOpen(true) });
  const launch = useAsync(openCcSwitchImport, {
    onSuccess: () => {
      setDialogOpen(false);
      setSent(true);
    },
  });

  const ready = providerName.trim() !== "" && baseUrl.trim() !== "" && apiKey.length > 0;
  const request: CcSwitchImportRequest = {
    tool,
    providerName: providerName.trim(),
    baseUrl: baseUrl.trim(),
    apiKey,
    model: model.trim(),
  };

  const requestImport = () => {
    setSent(false);
    void preview.run(request);
  };

  return (
    <Card title={t("guide:import.title")} description={t("guide:import.description")}>
      <div className="space-y-3">
        <div className="flex flex-wrap items-center gap-3">
          <Button
            onClick={requestImport}
            disabled={!ready}
            loading={preview.loading}
            leftIcon={<Send className="size-4" aria-hidden />}
            data-testid="cc-switch-import"
          >
            {t("guide:import.button")}
          </Button>
          {!ready && (
            <span className="text-sm text-neutral-500">{t("guide:import.needsValues")}</span>
          )}
        </div>
        <p className="text-xs text-neutral-500">{t("guide:import.editNote")}</p>
        {sent && (
          <Alert variant="success" data-testid="import-sent">
            {t("guide:import.sent")}
          </Alert>
        )}
        {preview.error && <ErrorBanner error={preview.error} onRetry={requestImport} />}
      </div>

      <ConfirmDialog
        open={dialogOpen && preview.data != null}
        title={t("guide:import.confirmTitle")}
        description={t("guide:import.confirmBody")}
        confirmLabel={t("guide:import.confirmAction")}
        confirmLoading={launch.loading}
        onConfirm={() => void launch.run(request)}
        onCancel={() => setDialogOpen(false)}
      >
        <div className="space-y-3">
          {preview.data && (
            <CopyField label={t("guide:import.linkLabel")} value={preview.data.displayUrl} />
          )}
          {launch.error && (
            <div className="space-y-2">
              <ErrorBanner error={launch.error} />
              <p className="text-sm text-neutral-600 dark:text-neutral-400">
                {t("guide:import.failed")}
              </p>
            </div>
          )}
        </div>
      </ConfirmDialog>
    </Card>
  );
}
