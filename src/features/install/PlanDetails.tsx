import { useTranslation } from "react-i18next";

import { Alert, CopyField, ExternalLink } from "@/components/ui";
import type { InstallPlan, Platform } from "@/lib/types";

import { NPM_PERMISSIONS_DOCS_URL } from "./install-targets";

export interface PlanDetailsProps {
  plan: InstallPlan;
  /** Localised tool name for the explanation sentence. */
  toolName: string;
  platform: Platform | null;
}

/**
 * The "show before run" surface (CLAUDE.md hard rule 4): the plan's explanation
 * (`install:plan.<explanationCode>`), the exact command that will be executed, the registry
 * mirror Rust chose, and — when the npm prefix is not writable — a warning with the manual
 * alternatives. For installer plans (`installerPath` set) that elevate, the note says the OS
 * will ask for authorisation (UAC / password) — the tool never elevates silently.
 */
export function PlanDetails({ plan, toolName, platform }: PlanDetailsProps) {
  const { t } = useTranslation();
  const hasCommand = plan.displayCommand.trim().length > 0;
  const installer = plan.installerPath !== null;
  const prefixCommand =
    platform === "windows"
      ? t("install:common.requiresAdmin.prefixCommandWindows")
      : t("install:common.requiresAdmin.prefixCommandUnix");

  return (
    <div className="space-y-3">
      <p className="text-sm text-neutral-700 dark:text-neutral-300">
        {t(`install:plan.${plan.explanationCode}`, {
          tool: toolName,
          defaultValue: t("install:plan.generic", { tool: toolName }),
        })}
      </p>

      {hasCommand && (
        <CopyField
          label={t("install:common.command")}
          value={plan.displayCommand}
          hint={
            installer ? t("install:common.commandHintInstaller") : t("install:common.commandHint")
          }
        />
      )}

      {plan.registry && (
        <p className="text-xs text-neutral-500" data-testid="plan-mirror">
          {t("install:common.mirror")}:{" "}
          <span className="font-medium text-neutral-700 dark:text-neutral-300">
            {t(`install:mirror.${plan.registry.id}`, { defaultValue: plan.registry.id })}
          </span>{" "}
          <span data-selectable className="font-mono">
            ({plan.registry.url})
          </span>{" "}
          · {t("install:common.mirrorHint")}
        </p>
      )}

      {plan.requiresAdmin && installer && (
        <Alert variant="info" data-testid="admin-note">
          {t("install:adminNote")}
        </Alert>
      )}

      {plan.requiresAdmin && !installer && (
        <Alert variant="warning" title={t("install:common.requiresAdmin.title")}>
          <p>{t("install:common.requiresAdmin.body")}</p>
          <ol className="mt-2 list-decimal space-y-2 pl-5">
            <li>{t("install:common.requiresAdmin.optionElevated")}</li>
            <li>
              <p>{t("install:common.requiresAdmin.optionPrefix")}</p>
              <CopyField value={prefixCommand} className="mt-2" />
              <p className="mt-2">
                {t("install:common.requiresAdmin.prefixHint")}{" "}
                <ExternalLink href={NPM_PERMISSIONS_DOCS_URL}>
                  {t("install:common.requiresAdmin.docsLabel")}
                </ExternalLink>
              </p>
            </li>
          </ol>
        </Alert>
      )}
    </div>
  );
}
