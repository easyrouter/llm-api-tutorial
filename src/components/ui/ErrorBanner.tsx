import { RefreshCw } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { describeError, OTHER_ERROR_CODE, toWireError } from "@/lib/errors";

import { Alert } from "./Alert";
import { Button } from "./Button";

export interface ErrorBannerProps {
  /** Anything thrown/rejected. `null`/`undefined` renders nothing. */
  error: unknown;
  onRetry?: () => void;
  retryLabel?: ReactNode;
  /** Defaults to `common:errors.title`. */
  title?: ReactNode;
  /** Extra actions rendered next to Retry (e.g. "Open help"). */
  actions?: ReactNode;
  className?: string;
}

/**
 * Localised error callout: `errors.<code>` message (see `lib/errors.ts`), the machine code in
 * small print for support tickets, and an optional Retry button.
 */
export function ErrorBanner({
  error,
  onRetry,
  retryLabel,
  title,
  actions,
  className,
}: ErrorBannerProps) {
  const { t } = useTranslation();
  if (error === null || error === undefined) return null;
  const wire = toWireError(error);
  const showCode = wire.code !== OTHER_ERROR_CODE;
  const hasActions = Boolean(onRetry) || Boolean(actions);

  return (
    <Alert
      variant="danger"
      title={title ?? t("errors.title")}
      className={className}
      data-error-code={wire.code}
      actions={
        hasActions ? (
          <>
            {onRetry && (
              <Button
                variant="secondary"
                size="sm"
                onClick={onRetry}
                leftIcon={<RefreshCw className="size-4" aria-hidden />}
              >
                {retryLabel ?? t("actions.retry")}
              </Button>
            )}
            {actions}
          </>
        ) : undefined
      }
    >
      <p>{describeError(t, wire)}</p>
      {showCode && (
        <p className="mt-1 font-mono text-xs text-neutral-500 dark:text-neutral-400">
          {t("ui.errorCode", { code: wire.code })}
        </p>
      )}
    </Alert>
  );
}
