/**
 * Final wizard screen: what was set up (per selected tool, from `wizard.verifyResults`), how
 * to start using it (terminal command per tool, support contact) and two exits — start over or
 * open the FAQ in the help drawer. Sends one best-effort `wizard_done` telemetry event when an
 * endpoint is configured (opt-in is enforced on the Rust side).
 */
import { BookOpen, PartyPopper, RotateCcw } from "lucide-react";
import { useEffect, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";

import { Button, Card, CopyField, StatusBadge } from "@/components/ui";
import { trackEvent } from "@/lib/tauri";
import { useAppStore } from "@/stores/app";
import { useWizardStore } from "@/stores/wizard";

import { allVerified, DONE_HELP_SECTION, summarizeTools, type ToolSummary } from "./done-summary";

interface ToolRowProps {
  summary: ToolSummary;
  onBackToVerify: () => void;
}

function ToolRow({ summary, onBackToVerify }: ToolRowProps) {
  const { t } = useTranslation();
  const { tool, outcome, version } = summary;
  const ok = outcome === "ok";
  const label =
    outcome === "ok"
      ? version
        ? t("help:done.verified", { version })
        : t("help:done.verifiedNoVersion")
      : outcome === "failed"
        ? t("help:done.failed")
        : t("help:done.notVerified");

  return (
    <li
      data-tool={tool}
      data-outcome={outcome}
      className="flex flex-wrap items-center justify-between gap-3 py-2"
    >
      <span className="font-medium">{t(`tools.${tool}`)}</span>
      <div className="flex flex-wrap items-center gap-2">
        <StatusBadge status={ok ? "pass" : "warn"} label={label} />
        {!ok && (
          <Button size="sm" variant="ghost" onClick={onBackToVerify}>
            {t("help:done.backToVerify")}
          </Button>
        )}
      </div>
    </li>
  );
}

export function DoneScreen() {
  const { t } = useTranslation();
  const config = useAppStore((s) => s.config);
  const selectedTools = useWizardStore((s) => s.selectedTools);
  const verifyResults = useWizardStore((s) => s.verifyResults);
  const goTo = useWizardStore((s) => s.goTo);
  const reset = useWizardStore((s) => s.reset);
  const openHelp = useWizardStore((s) => s.openHelp);

  const summaries = useMemo(
    () => summarizeTools(selectedTools, verifyResults, config),
    [selectedTools, verifyResults, config],
  );
  const allOk = allVerified(summaries);
  const telemetryConfigured = Boolean(config?.telemetry.endpoint);

  // One best-effort `wizard_done` per visit (the ref also absorbs StrictMode double effects).
  const sentRef = useRef(false);
  useEffect(() => {
    if (!telemetryConfigured || sentRef.current) return;
    sentRef.current = true;
    trackEvent({
      name: "wizard_done",
      step: "done",
      status: allOk ? "ok" : "fail",
      durationMs: null,
      errorClass: null,
      ruleId: null,
    }).catch(() => undefined);
  }, [telemetryConfigured, allOk]);

  const backToVerify = () => goTo("verify");
  const contact = config?.company.supportContact ?? "";

  return (
    <div className="mx-auto max-w-3xl space-y-5">
      <div className="flex items-center gap-3">
        <PartyPopper className="text-success-500 size-7 shrink-0" aria-hidden />
        <h1 className="text-2xl font-semibold">{t("done.title")}</h1>
      </div>

      <Card title={t("done.summary")}>
        {summaries.length === 0 ? (
          <p className="text-sm text-neutral-500">{t("help:done.noTools")}</p>
        ) : (
          <ul className="divide-y divide-neutral-200 text-sm dark:divide-neutral-800">
            {summaries.map((summary) => (
              <ToolRow key={summary.tool} summary={summary} onBackToVerify={backToVerify} />
            ))}
          </ul>
        )}
      </Card>

      <Card title={t("done.nextSteps")}>
        <div className="space-y-4 text-sm leading-6 text-neutral-700 dark:text-neutral-200">
          {summaries.map(({ tool, command }) => (
            <div key={tool} className="space-y-1.5">
              <p>{t("done.openTerminalHint", { command })}</p>
              <CopyField
                label={t("help:done.commandLabel", { tool: t(`tools.${tool}`) })}
                value={command}
              />
            </div>
          ))}
          <p>{t("done.helpHint", { contact })}</p>
          <p className="text-neutral-500 dark:text-neutral-400">{t("help:done.wrapUp")}</p>
        </div>
      </Card>

      <div className="flex items-center justify-between gap-4 border-t border-neutral-200 pt-5 dark:border-neutral-800">
        <Button
          variant="secondary"
          onClick={reset}
          leftIcon={<RotateCcw className="size-4" aria-hidden />}
        >
          {t("actions.startOver")}
        </Button>
        <Button
          variant="secondary"
          onClick={() => openHelp(DONE_HELP_SECTION)}
          leftIcon={<BookOpen className="size-4" aria-hidden />}
        >
          {t("help.openPanel")}
        </Button>
      </div>
    </div>
  );
}
