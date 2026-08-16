import { Gauge, RotateCcw, ShieldCheck } from "lucide-react";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import {
  Alert,
  Button,
  Card,
  ConfirmDialog,
  CopyField,
  ErrorBanner,
  LogView,
  type LogLine,
} from "@/components/ui";
import { useAsync } from "@/hooks";
import { onFastUiDone, onFastUiOutput } from "@/lib/events";
import { codexFastUiStatus, planCodexFastUi, startCodexFastUi } from "@/lib/tauri";
import type { FastUiAction, FastUiPlan, InstallDoneEvent } from "@/lib/types";

const ACTIONS: readonly FastUiAction[] = ["install", "verify", "restore"];

/**
 * Optional Codex "Fast UI" toolkit (Windows + Codex tab only — ADR-0007).
 *
 * Users who cannot sign in with a ChatGPT account do not get the speed / service-tier option in
 * the Codex client. This card offers IT's toolkit, which builds an **independent copy** of the
 * installed Codex client with that one UI gate opened; the official installation is untouched
 * and keeps working next to it. Every action is shown as an exact command and confirmed first
 * (hard rule 4), and `restore` undoes the patch.
 *
 * Deliberately last on the page and framed as optional: the supported route is the ChatGPT
 * sign-in step above, and an unofficial patch is pinned to one Codex build.
 */
export function CodexFastUiCard() {
  const { t } = useTranslation();
  const status = useAsync(codexFastUiStatus);
  const plan = useAsync(planCodexFastUi);
  const launch = useAsync(startCodexFastUi);
  const { run: loadStatus } = status;

  const [pending, setPending] = useState<FastUiAction | null>(null);
  const [lines, setLines] = useState<LogLine[]>([]);
  const [done, setDone] = useState<InstallDoneEvent | null>(null);
  const [running, setRunning] = useState(false);
  /** Job ids started here, so the shared event payload shape cannot pick up foreign jobs. */
  const jobIds = useRef(new Set<string>());

  useEffect(() => {
    void loadStatus();
  }, [loadStatus]);

  const finish = useCallback(
    (event: InstallDoneEvent) => {
      setRunning(false);
      setDone(event);
      void loadStatus();
    },
    [loadStatus],
  );

  useEffect(() => {
    let disposed = false;
    const active: (() => void)[] = [];
    const subscribe = (attach: Promise<() => void>) => {
      attach
        .then((unlisten) => {
          if (disposed) unlisten();
          else active.push(unlisten);
        })
        .catch(() => undefined);
    };
    subscribe(
      onFastUiOutput((event) => {
        if (!jobIds.current.has(event.jobId)) return;
        setLines((current) => [...current, { stream: event.stream, line: event.line }]);
      }),
    );
    subscribe(
      onFastUiDone((event) => {
        if (!jobIds.current.has(event.jobId)) return;
        finish(event);
      }),
    );
    return () => {
      disposed = true;
      for (const unlisten of active.splice(0)) unlisten();
    };
  }, [finish]);

  const info = status.data;
  // Windows-only feature: render nothing at all elsewhere (and while the status is loading).
  if (!info?.supported) return null;

  const requestAction = (action: FastUiAction) => {
    setDone(null);
    setPending(action);
    void plan.run(action);
  };

  const confirm = async (confirmed: FastUiPlan) => {
    const job = await launch.run(confirmed);
    setPending(null);
    if (!job) return;
    jobIds.current.add(job.jobId);
    setLines([]);
    setDone(null);
    setRunning(true);
  };

  const blocked = (action: FastUiAction): boolean => {
    if (running) return true;
    if (!info.toolkitAvailable) return true;
    return action === "install" ? !info.codexAppFound : !info.installed;
  };

  const icons: Record<FastUiAction, ReactNode> = {
    install: <Gauge className="size-4" aria-hidden />,
    verify: <ShieldCheck className="size-4" aria-hidden />,
    restore: <RotateCcw className="size-4" aria-hidden />,
  };

  return (
    <Card
      title={t("guide:fastui.title")}
      description={t("guide:fastui.description")}
      data-testid="codex-fast-ui"
    >
      <div className="space-y-3">
        <Alert variant="warning" title={t("guide:fastui.caveatTitle")}>
          {t("guide:fastui.caveat", {
            build: info.testedCodexBuild,
            version: info.toolkitVersion,
          })}
        </Alert>

        {info.installed && (
          <Alert variant="success" data-testid="fast-ui-installed">
            {t("guide:fastui.installedAt", { path: info.shortcutPath })}
          </Alert>
        )}
        {info.blockedCode && !info.installed && (
          <Alert variant="info" data-testid="fast-ui-blocked">
            {t(info.blockedCode)}
          </Alert>
        )}

        <div className="flex flex-wrap items-center gap-2">
          {ACTIONS.map((action) => (
            <Button
              key={action}
              variant={action === "install" ? "primary" : "secondary"}
              onClick={() => requestAction(action)}
              disabled={blocked(action)}
              loading={plan.loading && pending === action}
              leftIcon={icons[action]}
              data-testid={`fast-ui-${action}`}
            >
              {t(
                action === "install" && info.installed
                  ? "guide:fastui.action.reinstall"
                  : `guide:fastui.action.${action}`,
              )}
            </Button>
          ))}
        </div>

        <p className="text-xs text-neutral-500">{t("guide:fastui.reversible")}</p>

        {plan.error && <ErrorBanner error={plan.error} />}
        {launch.error && <ErrorBanner error={launch.error} />}

        {(running || lines.length > 0) && (
          <LogView lines={lines} maxHeight={200} emptyText={t("guide:fastui.starting")} />
        )}
        {done && (
          <Alert variant={done.success ? "success" : "danger"} data-testid="fast-ui-result">
            {done.success ? t("guide:fastui.succeeded") : t("guide:fastui.failed")}
          </Alert>
        )}
      </div>

      <ConfirmDialog
        open={pending != null && plan.data != null}
        title={t("guide:fastui.confirmTitle")}
        description={t(`guide:fastui.confirm.${pending ?? "install"}`)}
        confirmLabel={t("guide:fastui.confirmAction")}
        confirmLoading={launch.loading}
        danger={pending === "restore"}
        onConfirm={() => {
          if (plan.data) void confirm(plan.data);
        }}
        onCancel={() => setPending(null)}
      >
        {plan.data && (
          <div className="space-y-3">
            <CopyField
              label={t("guide:fastui.commandLabel")}
              value={plan.data.displayCommand}
              hint={t("guide:fastui.integrity", { sha256: plan.data.toolkitSha256 })}
            />
            {plan.data.reinstall && (
              <Alert variant="warning" data-testid="fast-ui-reinstall">
                {t("guide:fastui.reinstallNote")}
              </Alert>
            )}
          </div>
        )}
      </ConfirmDialog>
    </Card>
  );
}
