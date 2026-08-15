import { Check, Copy, TriangleAlert } from "lucide-react";
import { useId, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { useCopy } from "@/hooks/useCopy";
import { cn } from "@/lib/cn";
import { maskSecret } from "@/lib/format";

import { Button } from "./Button";

export interface CopyFieldProps {
  label?: ReactNode;
  /** The real value; always what gets copied. */
  value: string;
  /** Monospace rendering (default true — values are URLs, commands, keys). */
  mono?: boolean;
  /** Mask the value on screen (`sk-****abcd`); the clipboard still receives the real value. */
  secret?: boolean;
  /** Extra hint rendered under the value (e.g. where to paste it). */
  hint?: ReactNode;
  className?: string;
}

/**
 * Read-only value with a Copy button. Shows "Copied" for 1.5 s after a successful copy and a
 * short failure note if the clipboard is unavailable. Secrets are masked visually only.
 */
export function CopyField({
  label,
  value,
  mono = true,
  secret = false,
  hint,
  className,
}: CopyFieldProps) {
  const { t } = useTranslation();
  const id = useId();
  const { copied, failed, copy } = useCopy();
  const shown = secret ? maskSecret(value) : value;

  return (
    <div className={cn("space-y-1.5", className)}>
      {label && (
        <div id={id} className="text-sm font-medium text-neutral-700 dark:text-neutral-200">
          {label}
        </div>
      )}
      <div className="flex items-stretch gap-2">
        <div
          aria-labelledby={label ? id : undefined}
          aria-describedby={secret ? `${id}-secret` : undefined}
          data-selectable
          data-testid="copy-field-value"
          className={cn(
            "flex min-w-0 flex-1 items-center rounded-md border border-neutral-300 bg-neutral-50 px-3 py-2 text-sm break-all text-neutral-900 dark:border-neutral-700 dark:bg-neutral-950 dark:text-neutral-100",
            mono && "font-mono",
          )}
        >
          {shown || <span className="text-neutral-400">—</span>}
        </div>
        <Button
          variant="secondary"
          onClick={() => void copy(value)}
          leftIcon={
            copied ? (
              <Check className="text-success-500 size-4" aria-hidden />
            ) : (
              <Copy className="size-4" aria-hidden />
            )
          }
          className="shrink-0"
        >
          {copied ? t("actions.copied") : t("actions.copy")}
        </Button>
      </div>
      {secret && (
        <p id={`${id}-secret`} className="text-xs text-neutral-500">
          {t("ui.secretMasked")}
        </p>
      )}
      {hint && <p className="text-xs text-neutral-500">{hint}</p>}
      <p role="status" aria-live="polite" className={cn(!failed && "sr-only")}>
        {copied && <span className="sr-only">{t("actions.copied")}</span>}
        {failed && (
          <span className="text-danger-500 inline-flex items-center gap-1 text-xs">
            <TriangleAlert className="size-3" aria-hidden />
            {t("ui.copyFailed")}
          </span>
        )}
      </p>
    </div>
  );
}
