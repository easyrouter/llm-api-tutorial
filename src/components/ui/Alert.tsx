import { CircleAlert, CircleCheck, Info, TriangleAlert } from "lucide-react";
import type { HTMLAttributes, ReactNode } from "react";

import { cn } from "@/lib/cn";

export type AlertVariant = "info" | "success" | "warning" | "danger";

export interface AlertProps extends Omit<HTMLAttributes<HTMLDivElement>, "title"> {
  variant?: AlertVariant;
  title?: ReactNode;
  /** Buttons/links rendered under the message. */
  actions?: ReactNode;
  /** Hide the leading icon (e.g. for very short inline notes). */
  hideIcon?: boolean;
}

const styles: Record<AlertVariant, { box: string; icon: string; Icon: typeof Info }> = {
  info: {
    box: "border-brand-500/30 bg-brand-50 text-neutral-800 dark:bg-brand-700/15 dark:text-neutral-100",
    icon: "text-brand-600 dark:text-brand-500",
    Icon: Info,
  },
  success: {
    box: "border-success-500/30 bg-success-500/10 text-neutral-800 dark:text-neutral-100",
    icon: "text-success-500",
    Icon: CircleCheck,
  },
  warning: {
    box: "border-warning-500/30 bg-warning-500/10 text-neutral-800 dark:text-neutral-100",
    icon: "text-warning-500",
    Icon: TriangleAlert,
  },
  danger: {
    box: "border-danger-500/30 bg-danger-500/10 text-neutral-800 dark:text-neutral-100",
    icon: "text-danger-500",
    Icon: CircleAlert,
  },
};

/**
 * Inline callout. Warnings/dangers use `role="alert"` (assertive), info/success `role="status"`
 * (polite); pass `role` explicitly to override.
 */
export function Alert({
  variant = "info",
  title,
  actions,
  hideIcon = false,
  className,
  children,
  role,
  ...rest
}: AlertProps) {
  const { box, icon, Icon } = styles[variant];
  const defaultRole = variant === "danger" || variant === "warning" ? "alert" : "status";
  return (
    <div
      role={role ?? defaultRole}
      data-variant={variant}
      className={cn("flex gap-3 rounded-lg border p-4 text-sm leading-6", box, className)}
      {...rest}
    >
      {!hideIcon && <Icon className={cn("mt-1 size-4 shrink-0", icon)} aria-hidden />}
      <div className="min-w-0 flex-1">
        {title && <div className="font-semibold">{title}</div>}
        {children != null && children !== false && (
          <div className={title ? "mt-1" : undefined}>{children}</div>
        )}
        {actions && <div className="mt-3 flex flex-wrap items-center gap-2">{actions}</div>}
      </div>
    </div>
  );
}
