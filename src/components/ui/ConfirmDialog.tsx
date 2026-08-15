import { useEffect, useId, useRef, type KeyboardEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";

import { Button } from "./Button";

export interface ConfirmDialogProps {
  open: boolean;
  title: ReactNode;
  description?: ReactNode;
  /** Extra content between the description and the buttons (e.g. the command about to run). */
  children?: ReactNode;
  confirmLabel?: ReactNode;
  cancelLabel?: ReactNode;
  /** Red confirm button for destructive / irreversible actions. */
  danger?: boolean;
  /** Disables Confirm and shows a spinner (e.g. while the confirmed action is starting). */
  confirmLoading?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

function focusables(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE));
}

/**
 * Accessible confirmation modal rendered into `document.body`.
 * - `role="dialog"`, `aria-modal`, labelled by the title and described by the description
 * - focuses the first button on open, keeps Tab inside, restores focus on close
 * - Escape and a click on the backdrop both cancel
 *
 * This is the "show before run" surface (CLAUDE.md hard rule 4): put the exact command in
 * `children` (e.g. `<CopyField value={plan.displayCommand} />`) so the user sees what will
 * happen before confirming.
 */
export function ConfirmDialog({
  open,
  title,
  description,
  children,
  confirmLabel,
  cancelLabel,
  danger = false,
  confirmLoading = false,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const { t } = useTranslation();
  const id = useId();
  const panelRef = useRef<HTMLDivElement>(null);
  const restoreRef = useRef<HTMLElement | null>(null);

  // Focus management: remember the opener, focus the first button, restore on close.
  useEffect(() => {
    if (!open) return;
    restoreRef.current = document.activeElement as HTMLElement | null;
    const panel = panelRef.current;
    const first = panel?.querySelector<HTMLElement>("button:not([disabled])");
    (first ?? panel)?.focus();
    return () => {
      restoreRef.current?.focus?.();
      restoreRef.current = null;
    };
  }, [open]);

  if (!open) return null;

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.stopPropagation();
      onCancel();
      return;
    }
    if (event.key !== "Tab" || !panelRef.current) return;
    const items = focusables(panelRef.current);
    if (items.length === 0) {
      event.preventDefault();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement;
    if (event.shiftKey && (active === first || active === panelRef.current)) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && active === last) {
      event.preventDefault();
      first?.focus();
    }
  };

  const descriptionId = description ? `${id}-desc` : undefined;

  return createPortal(
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-neutral-900/50 p-4 backdrop-blur-[2px]"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onCancel();
      }}
      data-testid="dialog-backdrop"
    >
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={`${id}-title`}
        aria-describedby={descriptionId}
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className="w-full max-w-lg rounded-xl border border-neutral-200 bg-white p-6 shadow-xl outline-none dark:border-neutral-800 dark:bg-neutral-900"
      >
        <h2 id={`${id}-title`} className="text-lg font-semibold">
          {title}
        </h2>
        {description && (
          <p
            id={descriptionId}
            className="mt-2 text-sm leading-6 text-neutral-600 dark:text-neutral-300"
          >
            {description}
          </p>
        )}
        {children && <div className="mt-4">{children}</div>}
        <div className="mt-6 flex justify-end gap-3">
          <Button variant="secondary" onClick={onCancel}>
            {cancelLabel ?? t("actions.cancel")}
          </Button>
          <Button
            variant={danger ? "danger" : "primary"}
            onClick={onConfirm}
            loading={confirmLoading}
          >
            {confirmLabel ?? t("actions.confirm")}
          </Button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
