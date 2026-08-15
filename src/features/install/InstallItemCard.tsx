import { SkipForward, Undo2 } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Alert, Button, Card, StatusBadge } from "@/components/ui";
import type { AppConfig, Platform } from "@/lib/types";

import { CcSwitchItemBody } from "./CcSwitchItemBody";
import type { InstallActions } from "./useInstallController";
import {
  isBusy,
  itemBadge,
  itemOutcome,
  jobIdOf,
  type InstallState,
  type ItemState,
} from "./install-state";
import { installKind } from "./install-targets";
import { NodeItemBody } from "./NodeItemBody";
import { NpmItemBody } from "./NpmItemBody";

export interface InstallItemCardProps {
  item: ItemState;
  state: InstallState;
  actions: InstallActions;
  config: AppConfig | null;
  platform: Platform | null;
}

/** One install target: header (name + phase badge), kind-specific body, skip controls. */
export function InstallItemCard({ item, state, actions, config, platform }: InstallItemCardProps) {
  const { t } = useTranslation();
  const { target } = item;
  const toolName = t(`common:tools.${target}`);
  const badge = itemBadge(item);
  const outcome = itemOutcome(item);
  const busy = isBusy(item);
  const jobId = jobIdOf(item.step);

  const body = (() => {
    switch (installKind(target)) {
      case "npm":
        return (
          <NpmItemBody
            item={item}
            log={jobId ? (state.jobs[jobId] ?? null) : null}
            toolName={toolName}
            platform={platform}
            config={config}
            actions={actions}
          />
        );
      case "node":
        return (
          <NodeItemBody
            item={item}
            toolName={toolName}
            platform={platform}
            config={config}
            actions={actions}
          />
        );
      case "cc-switch":
        return (
          <CcSwitchItemBody
            item={item}
            progress={
              item.step.phase === "downloading" ? (state.downloads[item.step.jobId] ?? null) : null
            }
            platform={platform}
            config={config}
            actions={actions}
          />
        );
    }
  })();

  return (
    <Card
      data-testid={`install-item-${target}`}
      data-phase={item.step.phase}
      title={toolName}
      actions={<StatusBadge status={badge.status} label={t(`install:state.${badge.labelKey}`)} />}
    >
      {outcome === "skipped" ? (
        <Alert
          variant="warning"
          actions={
            <Button
              variant="secondary"
              size="sm"
              onClick={() => actions.unskip(target)}
              leftIcon={<Undo2 className="size-4" aria-hidden />}
            >
              {t("install:actions.undoSkip")}
            </Button>
          }
        >
          {t("install:skip.warning", { tool: toolName })}
        </Alert>
      ) : (
        <div className="space-y-4">
          {body}
          {outcome === "pending" && (
            <div className="flex justify-end border-t border-neutral-200 pt-3 dark:border-neutral-800">
              <Button
                variant="ghost"
                size="sm"
                onClick={() => actions.skip(target)}
                disabled={busy}
                leftIcon={<SkipForward className="size-4" aria-hidden />}
              >
                {t("install:actions.skip")}
              </Button>
            </div>
          )}
        </div>
      )}
    </Card>
  );
}
