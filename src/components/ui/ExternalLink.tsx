import { ExternalLink as ExternalLinkIcon } from "lucide-react";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { cn } from "@/lib/cn";
import { openExternal } from "@/lib/tauri";

import { Button, type ButtonProps } from "./Button";

export interface ExternalLinkProps {
  /** http(s) URL; opened in the system browser by the Rust `open_external` command. */
  href: string;
  children: ReactNode;
  /** `inline` looks like a text link (default); `button` renders a secondary `Button`. */
  variant?: "inline" | "button";
  size?: ButtonProps["size"];
  className?: string;
}

/**
 * Opens a URL in the user's default browser. Rendered as a `<button>` (never `<a href>`): the
 * webview must not navigate, and only the Rust side may open URLs (validated to http/https).
 */
export function ExternalLink({
  href,
  children,
  variant = "inline",
  size = "sm",
  className,
}: ExternalLinkProps) {
  const { t } = useTranslation();
  const [failed, setFailed] = useState(false);
  const open = () => {
    setFailed(false);
    openExternal(href).catch(() => setFailed(true));
  };
  const title = failed ? t("ui.openLinkFailed", { url: href }) : href;

  if (variant === "button") {
    return (
      <Button
        variant="secondary"
        size={size}
        onClick={open}
        title={title}
        className={className}
        leftIcon={<ExternalLinkIcon className="size-4" aria-hidden />}
      >
        {children}
      </Button>
    );
  }
  return (
    <button
      type="button"
      onClick={open}
      title={title}
      className={cn(
        "text-brand-700 hover:text-brand-600 focus-visible:ring-brand-500 dark:text-brand-100 inline-flex items-center gap-1 rounded-sm underline underline-offset-2 focus-visible:ring-2 focus-visible:outline-none",
        failed && "text-danger-500",
        className,
      )}
    >
      {children}
      <ExternalLinkIcon className="size-3.5" aria-hidden />
      <span className="sr-only">{t("ui.opensInBrowser")}</span>
    </button>
  );
}
