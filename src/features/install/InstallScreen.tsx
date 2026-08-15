import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { Alert, StepFooter } from "@/components/ui";
import type { InstallTarget } from "@/lib/types";
import { useAppStore } from "@/stores/app";
import { useInstallStore } from "@/stores/install";
import { useWizardStore } from "@/stores/wizard";

import { InstallItemCard } from "./InstallItemCard";
import { allHandled, countHandled } from "./install-state";
import { deriveInstallTargets, installKind } from "./install-targets";
import { useInstallController } from "./useInstallController";

/**
 * M2 — Install. The list of targets is derived once when the screen mounts (from the
 * environment snapshot, the selected tools and explicit requests) and stays stable while the
 * user works through it — a target that becomes installed does not vanish mid-flow. Next is
 * enabled once every item is done or skipped.
 */
export function InstallScreen() {
  const { t } = useTranslation();
  const config = useAppStore((s) => s.config);
  const platform = useAppStore((s) => s.info?.platform ?? null);
  const snapshot = useWizardStore((s) => s.snapshot);
  const selectedTools = useWizardStore((s) => s.selectedTools);
  const next = useWizardStore((s) => s.next);
  const back = useWizardStore((s) => s.back);
  const requested = useInstallStore((s) => s.requested);
  const skipped = useInstallStore((s) => s.skipped);

  const [targets] = useState<InstallTarget[]>(() =>
    deriveInstallTargets(snapshot, selectedTools, requested),
  );
  const [initialSkipped] = useState<InstallTarget[]>(() =>
    skipped.filter((s) => targets.includes(s)),
  );
  const { state, actions } = useInstallController(targets, initialSkipped);

  // Prepare every pending item once (plan / fetch release) so the user sees what will happen
  // without an extra click. Guarded so React StrictMode's double-invocation does not double it.
  const prepared = useRef(false);
  useEffect(() => {
    if (prepared.current) return;
    prepared.current = true;
    for (const target of targets) {
      if (initialSkipped.includes(target)) continue;
      if (installKind(target) === "cc-switch") void actions.fetchRelease(target);
      else void actions.plan(target);
    }
  }, [actions, initialSkipped, targets]);

  const done = allHandled(state, targets);
  const handled = countHandled(state, targets);

  return (
    <div className="mx-auto max-w-3xl space-y-5">
      <div>
        <h1 className="text-2xl font-semibold">{t("install:screen.title")}</h1>
        <p className="mt-2 text-sm leading-6 text-neutral-600 dark:text-neutral-300">
          {t("install:screen.intro")}
        </p>
      </div>

      {targets.length === 0 ? (
        snapshot ? (
          <Alert variant="success" title={t("install:screen.nothingNeeded")}>
            {t("install:screen.nothingNeededHint")}
          </Alert>
        ) : (
          <Alert variant="info">{t("install:screen.noSnapshot")}</Alert>
        )
      ) : (
        <>
          <Alert variant={done ? "success" : "info"} data-testid="install-progress">
            {done
              ? t("install:screen.allDone")
              : `${t("install:screen.progress", { done: handled, total: targets.length })} · ${t("install:screen.pendingHint")}`}
          </Alert>
          <div className="space-y-4">
            {targets.map((target) => {
              const item = state.items[target];
              return item ? (
                <InstallItemCard
                  key={target}
                  item={item}
                  state={state}
                  actions={actions}
                  config={config}
                  platform={platform}
                />
              ) : null;
            })}
          </div>
        </>
      )}

      <StepFooter onBack={back} onNext={next} nextDisabled={!done} />
    </div>
  );
}
