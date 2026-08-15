import { ClipboardCopy, FileDown, ShieldCheck, Stethoscope } from "lucide-react";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import {
  Button,
  Card,
  ErrorBanner,
  FixActionButtons,
  KeyValueList,
  StatusBadge,
} from "@/components/ui";
import { useCopy } from "@/hooks/useCopy";
import { cn } from "@/lib/cn";
import type { Diagnosis, EnvSnapshot, VerifyResult } from "@/lib/types";
import { useWizardStore } from "@/stores/wizard";

import { severityBadge, sortDiagnoses } from "./diagnoses";
import { exportDiagnosticReport, fetchReportMarkdown, type ExportOutcome } from "./report";

export interface DiagnosePanelProps {
  /** Findings from the rule engine (any order — the panel sorts by severity). */
  diagnoses: readonly Diagnosis[];
  /** Environment snapshot included in the report; `null` when the checks have not run. */
  snapshot: EnvSnapshot | null;
  /** Handler for `rerun` fix actions (e.g. verify again). Hidden when absent. */
  onRerun?: () => void;
  /** Extra controls rendered in the panel header (e.g. "Run diagnosis with full snapshot"). */
  headerActions?: ReactNode;
  /** Short note under the description (e.g. "auto-opened because verification failed"). */
  note?: ReactNode;
  title?: ReactNode;
  className?: string;
  id?: string;
}

/**
 * Diagnosis panel (M5 UI). Renders one card per finding — severity badge, guide fault id,
 * title, explanation, ordered checklist, details and `FixActionButtons` — plus a footer that
 * copies or exports the fully redacted Markdown report built by the Rust core.
 *
 * i18n contract (namespace `diagnose`): `<code>.title`, `<code>.explanation`,
 * `<code>.steps.<step>` (falls back to the shared `steps.<step>` so a step reused by another
 * rule still has text), `params.<key>` for detail labels. Used by the Verify screen and
 * standalone (e.g. from the environment check).
 */
export function DiagnosePanel({
  diagnoses,
  snapshot,
  onRerun,
  headerActions,
  note,
  title,
  className,
  id,
}: DiagnosePanelProps) {
  const { t } = useTranslation();
  const sorted = sortDiagnoses(diagnoses);

  return (
    <Card
      id={id}
      role="region"
      aria-label={t("diagnose:panel.title")}
      title={
        <span className="inline-flex items-center gap-2">
          <Stethoscope className="text-brand-600 size-4" aria-hidden />
          {title ?? t("diagnose:panel.title")}
        </span>
      }
      description={t("diagnose:panel.description")}
      actions={headerActions}
      className={cn("space-y-4", className)}
      data-testid="diagnose-panel"
    >
      {note && <p className="text-sm text-neutral-600 dark:text-neutral-300">{note}</p>}

      {sorted.length === 0 ? (
        <p className="text-sm text-neutral-500">{t("diagnose:panel.empty")}</p>
      ) : (
        <>
          <p className="text-xs text-neutral-500">
            {t("diagnose:panel.count", { count: sorted.length })}
          </p>
          <ol className="space-y-3">
            {sorted.map((d) => (
              <li key={`${d.ruleId}:${d.code}`}>
                <DiagnosisCard diagnosis={d} onRerun={onRerun} />
              </li>
            ))}
          </ol>
        </>
      )}

      <ReportFooter snapshot={snapshot} diagnoses={sorted} />
    </Card>
  );
}

function DiagnosisCard({ diagnosis, onRerun }: { diagnosis: Diagnosis; onRerun?: () => void }) {
  const { t } = useTranslation();
  const { code, params, ruleId, severity, checklist, actions } = diagnosis;
  const details = Object.entries(params);

  return (
    <article
      data-rule-id={ruleId}
      data-code={code}
      className="rounded-lg border border-neutral-200 p-4 dark:border-neutral-800"
    >
      <header className="flex flex-wrap items-center gap-2">
        <StatusBadge status={severityBadge(severity)} label={t(`diagnose:severity.${severity}`)} />
        <span className="font-mono text-xs text-neutral-500">
          {t("diagnose:panel.ruleLabel", { ruleId })}
        </span>
      </header>
      <h3 className="mt-2 text-sm font-semibold">{t(`diagnose:${code}.title`, params)}</h3>
      <p className="mt-1 text-sm leading-6 whitespace-pre-line text-neutral-700 dark:text-neutral-300">
        {t(`diagnose:${code}.explanation`, params)}
      </p>

      {checklist.length > 0 && (
        <div className="mt-3">
          <div className="text-xs font-medium text-neutral-500 uppercase">
            {t("diagnose:panel.checklist")}
          </div>
          <ol className="mt-1 list-decimal space-y-1 pl-5 text-sm text-neutral-700 dark:text-neutral-300">
            {checklist.map((step) => (
              <li key={step}>
                {t([`diagnose:${code}.steps.${step}`, `diagnose:steps.${step}`], params)}
              </li>
            ))}
          </ol>
        </div>
      )}

      {details.length > 0 && (
        <div className="mt-3">
          <div className="text-xs font-medium text-neutral-500 uppercase">
            {t("diagnose:panel.details")}
          </div>
          <KeyValueList
            className="mt-1"
            items={details.map(([key, value]) => ({
              key,
              label: t(`diagnose:params.${key}`, { defaultValue: key }),
              value,
              mono: true,
            }))}
          />
        </div>
      )}

      {actions.length > 0 && (
        <FixActionButtons fixes={actions} onRerun={onRerun} className="mt-3" />
      )}
    </article>
  );
}

type Busy = "copy" | "export" | null;

/** Verification results kept by the wizard (one per tool), for the report's Verification section. */
function latestVerifyResults(): VerifyResult[] {
  return Object.values(useWizardStore.getState().verifyResults).filter(
    (r): r is VerifyResult => r !== undefined,
  );
}

function ReportFooter({
  snapshot,
  diagnoses,
}: {
  snapshot: EnvSnapshot | null;
  diagnoses: readonly Diagnosis[];
}) {
  const { t } = useTranslation();
  const { copied, failed, copy } = useCopy();
  const [busy, setBusy] = useState<Busy>(null);
  const [outcome, setOutcome] = useState<ExportOutcome | null>(null);
  const [error, setError] = useState<unknown>(null);

  const copyReport = async () => {
    setBusy("copy");
    setError(null);
    setOutcome(null);
    try {
      const markdown = await fetchReportMarkdown(snapshot, diagnoses, latestVerifyResults());
      await copy(markdown);
    } catch (e) {
      setError(e);
    } finally {
      setBusy(null);
    }
  };

  const exportReport = async () => {
    setBusy("export");
    setError(null);
    setOutcome(null);
    try {
      setOutcome(await exportDiagnosticReport(snapshot, diagnoses, latestVerifyResults()));
    } catch (e) {
      setError(e);
    } finally {
      setBusy(null);
    }
  };

  return (
    <footer className="space-y-3 border-t border-neutral-200 pt-4 dark:border-neutral-800">
      <p className="flex items-start gap-2 text-xs text-neutral-500">
        <ShieldCheck className="text-success-500 mt-0.5 size-3.5 shrink-0" aria-hidden />
        {t("diagnose:panel.redactedNote")}
      </p>
      <div className="flex flex-wrap items-center gap-2">
        <Button
          variant="secondary"
          size="sm"
          onClick={() => void copyReport()}
          loading={busy === "copy"}
          disabled={busy !== null}
          leftIcon={<ClipboardCopy className="size-4" aria-hidden />}
        >
          {copied ? t("diagnose:panel.copied") : t("diagnose:panel.copyReport")}
        </Button>
        <Button
          variant="secondary"
          size="sm"
          onClick={() => void exportReport()}
          loading={busy === "export"}
          disabled={busy !== null}
          leftIcon={<FileDown className="size-4" aria-hidden />}
        >
          {busy === "export" ? t("diagnose:panel.exporting") : t("diagnose:panel.exportReport")}
        </Button>
      </div>
      <p
        role="status"
        aria-live="polite"
        className="text-xs text-neutral-600 dark:text-neutral-300"
      >
        {failed && <span className="text-danger-500">{t("ui.copyFailed")}</span>}
        {outcome?.kind === "saved" && (
          <span data-selectable>{t("diagnose:panel.exported", { path: outcome.path })}</span>
        )}
        {outcome?.kind === "cancelled" && <span>{t("diagnose:panel.exportCancelled")}</span>}
      </p>
      {error !== null && <ErrorBanner error={error} title={t("diagnose:panel.reportError")} />}
    </footer>
  );
}
