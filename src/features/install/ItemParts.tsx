import { ExternalLink as ExternalLinkIcon, RefreshCw } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Alert, Button, ErrorBanner, Spinner } from "@/components/ui";
import { checkMessage } from "@/features/env-check/snapshot-utils";
import { openExternal } from "@/lib/tauri";

import type { RecheckState } from "./install-state";

/** Spinner + phase label, used while an IPC call is in flight. */
export function PhaseSpinner({ labelKey }: { labelKey: string }) {
  const { t } = useTranslation();
  return (
    <div className="flex items-center gap-2 text-sm text-neutral-600 dark:text-neutral-300">
      <Spinner size="sm" decorative />
      <span>{t(`install:state.${labelKey}`)}</span>
    </div>
  );
}

export interface RecheckFeedbackProps {
  recheck: RecheckState;
  /** Key under `install:` for a passing result (receives `message`). */
  passedKey: string;
  /** Key under `install:` for a non-passing result (receives `message`). */
  notPassedKey: string;
}

/** Outcome of the "I installed it — re-check" action (or the automatic re-check after npm). */
export function RecheckFeedback({ recheck, passedKey, notPassedKey }: RecheckFeedbackProps) {
  const { t } = useTranslation();
  switch (recheck.status) {
    case "idle":
      return null;
    case "running":
      return <PhaseSpinner labelKey="rechecking" />;
    case "failed":
      return <ErrorBanner error={recheck.error} title={t("install:recheck.failed")} />;
    case "done": {
      if (!recheck.result) return null;
      const message = checkMessage(t, recheck.result);
      const passed = recheck.result.status === "pass";
      return (
        <Alert variant={passed ? "success" : "warning"} data-testid="recheck-feedback">
          {t(passed ? passedKey : notPassedKey, { message })}
        </Alert>
      );
    }
  }
}

export interface RecheckButtonProps {
  onClick: () => void;
  disabled?: boolean;
}

export function RecheckButton({ onClick, disabled }: RecheckButtonProps) {
  const { t } = useTranslation();
  return (
    <Button
      variant="primary"
      size="sm"
      onClick={onClick}
      disabled={disabled}
      leftIcon={<RefreshCw className="size-4" aria-hidden />}
    >
      {t("install:actions.installedRecheck")}
    </Button>
  );
}

export interface OpenPageButtonProps {
  url: string;
  label: string;
  disabled?: boolean;
}

/** Secondary button that opens `url` in the system browser and shows failures inline. */
export function OpenPageButton({ url, label, disabled }: OpenPageButtonProps) {
  const [error, setError] = useState<unknown>(null);
  const open = () => {
    setError(null);
    openExternal(url).catch((e: unknown) => setError(e));
  };
  return (
    <>
      <Button
        variant="secondary"
        size="sm"
        onClick={open}
        disabled={disabled}
        title={url}
        leftIcon={<ExternalLinkIcon className="size-4" aria-hidden />}
      >
        {label}
      </Button>
      {error !== null && <ErrorBanner error={error} className="basis-full" />}
    </>
  );
}
