/**
 * Final wizard screen: what was set up (per selected tool, from `wizard.verifyResults`), how
 * to start using it (terminal command per tool, support contact) and two exits — start over or
 * open the FAQ in the help drawer. Sends one best-effort `wizard_done` telemetry event when an
 * endpoint is configured (opt-in is enforced on the Rust side).
 */
import { AppWindow, BookOpen, ExternalLink, PartyPopper, RotateCcw } from "lucide-react";
import { useEffect, useMemo, useRef, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { Button, Card, CopyField, StatusBadge } from "@/components/ui";
import { openExternal, trackEvent } from "@/lib/tauri";
import { useAppStore } from "@/stores/app";
import { startOver } from "@/stores/start-over";
import { useWizardStore } from "@/stores/wizard";

import {
  allVerified,
  CODEX_IDE_EXTENSION_URL,
  commandForTool,
  DONE_HELP_SECTION,
  launchOptions,
  summarizeTools,
  type LaunchOptionId,
  type ToolSummary,
} from "./done-summary";

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

/**
 * One "use it next" option (PRD revision R4): the Codex client opens the IDE-extension page
 * (it shares the freshly configured `~/.codex` with the CLI); the CLI options show the command
 * to run in a new terminal.
 */
function LaunchOption({ option }: { option: LaunchOptionId }) {
  const { t } = useTranslation();
  const config = useAppStore((s) => s.config);

  let action: ReactNode;
  switch (option) {
    case "codex-client":
      action = (
        <Button
          variant="secondary"
          size="sm"
          onClick={() => void openExternal(CODEX_IDE_EXTENSION_URL)}
          leftIcon={<ExternalLink className="size-4" aria-hidden />}
        >
          {t("done.launch.codexClient.open")}
        </Button>
      );
      break;
    case "codex-cli":
      action = (
        <CopyField
          label={t("help:done.commandLabel", { tool: t("tools.codex") })}
          value={commandForTool("codex", config)}
        />
      );
      break;
    case "claude-code":
      action = (
        <CopyField
          label={t("help:done.commandLabel", { tool: t("tools.claude-code") })}
          value={commandForTool("claude-code", config)}
        />
      );
      break;
  }

  return (
    <div
      className="space-y-1.5 rounded-md border border-neutral-200 p-3 dark:border-neutral-800"
      data-testid={`launch-${option}`}
    >
      <h3 className="inline-flex items-center gap-2 text-sm font-semibold">
        <AppWindow className="size-4 text-neutral-500" aria-hidden />
        {t(`done.launch.${optionKey(option)}.name`)}
      </h3>
      <p>{t(`done.launch.${optionKey(option)}.body`)}</p>
      {action}
    </div>
  );
}

/** i18n key segment of a launch option (`codex-client` → `codexClient`). */
function optionKey(option: LaunchOptionId): "codexClient" | "codexCli" | "claudeCode" {
  switch (option) {
    case "codex-client":
      return "codexClient";
    case "codex-cli":
      return "codexCli";
    case "claude-code":
      return "claudeCode";
  }
}

export function DoneScreen() {
  const { t } = useTranslation();
  const config = useAppStore((s) => s.config);
  const selectedTools = useWizardStore((s) => s.selectedTools);
  const verifyResults = useWizardStore((s) => s.verifyResults);
  const goTo = useWizardStore((s) => s.goTo);
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

      <Card title={t("done.launch.title")} description={t("done.launch.description")}>
        <div className="space-y-4 text-sm leading-6 text-neutral-700 dark:text-neutral-200">
          {launchOptions(selectedTools).map((option) => (
            <LaunchOption key={option} option={option} />
          ))}
          <p>{t("done.helpHint", { contact })}</p>
          <p className="text-neutral-500 dark:text-neutral-400">{t("help:done.wrapUp")}</p>
        </div>
      </Card>

      <div className="flex items-center justify-between gap-4 border-t border-neutral-200 pt-5 dark:border-neutral-800">
        <Button
          variant="secondary"
          onClick={startOver}
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
