import {
  ArrowRight,
  BookOpen,
  ChevronUp,
  Download,
  ExternalLink as ExternalLinkIcon,
  RefreshCw,
} from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";

import { cn } from "@/lib/cn";
import { openExternal } from "@/lib/tauri";
import type { FixAction, InstallTarget } from "@/lib/types";
import { useWizardStore } from "@/stores/wizard";

import { Alert } from "./Alert";
import { Button, type ButtonProps } from "./Button";
import { ErrorBanner } from "./ErrorBanner";

export interface FixActionButtonsProps {
  fixes: FixAction[];
  /** Handler for `rerun` fixes; the button is hidden when absent. */
  onRerun?: () => void;
  /**
   * Handler for `install` fixes. When absent the button navigates to the Install step, which
   * lists everything that is missing.
   */
  onInstall?: (target: InstallTarget) => void;
  size?: ButtonProps["size"];
  className?: string;
}

/**
 * Renders one button per `FixAction` attached to a check result or diagnosis:
 *
 * | kind           | label (common ns)                                | click                                   |
 * | -------------- | ------------------------------------------------ | --------------------------------------- |
 * | `open_url`     | `fixes.open_url` + `fixes.labels.<labelCode>`    | `openExternal(url)` (Rust validates)    |
 * | `go_to_step`   | `fixes.go_to_step` + `steps.<step>`              | wizard store `goTo(step)`               |
 * | `install`      | `fixes.install` + `tools.<tool>`                 | `onInstall(tool)` or `goTo("install")`  |
 * | `instructions` | `fixes.instructions` / `ui.hideInstructions`     | toggles an inline Alert                 |
 * | `rerun`        | `fixes.rerun`                                    | `onRerun()`                             |
 *
 * `instructions.code` must be a **fully-qualified** i18n key (`"checks:env_vars.instructions"`,
 * i.e. `<namespace>:<path>`), because this component cannot know which namespace the Rust
 * module that produced the action belongs to. It is rendered as-is with `t(code, params)`.
 * Unknown `labelCode`s fall back to the raw code so a missing translation is visible, not
 * silent.
 */
export function FixActionButtons({
  fixes,
  onRerun,
  onInstall,
  size = "sm",
  className,
}: FixActionButtonsProps) {
  const { t } = useTranslation();
  const goTo = useWizardStore((s) => s.goTo);
  const idBase = useId();
  const [openInstructions, setOpenInstructions] = useState<Set<number>>(() => new Set());
  const [error, setError] = useState<unknown>(null);

  if (fixes.length === 0) return null;

  const toggleInstructions = (index: number) =>
    setOpenInstructions((prev) => {
      const next = new Set(prev);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return next;
    });

  const openUrl = (url: string) => {
    setError(null);
    openExternal(url).catch((e: unknown) => setError(e));
  };

  const renderButton = (fix: FixAction, index: number) => {
    switch (fix.kind) {
      case "open_url":
        return (
          <Button
            key={index}
            variant="secondary"
            size={size}
            onClick={() => openUrl(fix.url)}
            title={fix.url}
            leftIcon={<ExternalLinkIcon className="size-4" aria-hidden />}
          >
            {t("fixes.open_url", {
              label: t(`fixes.labels.${fix.labelCode}`, { defaultValue: fix.labelCode }),
            })}
          </Button>
        );
      case "go_to_step":
        return (
          <Button
            key={index}
            variant="secondary"
            size={size}
            onClick={() => goTo(fix.step)}
            leftIcon={<ArrowRight className="size-4" aria-hidden />}
          >
            {t("fixes.go_to_step", { step: t(`steps.${fix.step}`) })}
          </Button>
        );
      case "install":
        return (
          <Button
            key={index}
            variant="primary"
            size={size}
            onClick={() => (onInstall ? onInstall(fix.tool) : goTo("install"))}
            leftIcon={<Download className="size-4" aria-hidden />}
          >
            {t("fixes.install", { tool: t(`tools.${fix.tool}`) })}
          </Button>
        );
      case "instructions": {
        const expanded = openInstructions.has(index);
        return (
          <Button
            key={index}
            variant="secondary"
            size={size}
            onClick={() => toggleInstructions(index)}
            aria-expanded={expanded}
            aria-controls={`${idBase}-instructions-${index}`}
            leftIcon={
              expanded ? (
                <ChevronUp className="size-4" aria-hidden />
              ) : (
                <BookOpen className="size-4" aria-hidden />
              )
            }
          >
            {expanded ? t("ui.hideInstructions") : t("fixes.instructions")}
          </Button>
        );
      }
      case "rerun":
        if (!onRerun) return null;
        return (
          <Button
            key={index}
            variant="secondary"
            size={size}
            onClick={onRerun}
            leftIcon={<RefreshCw className="size-4" aria-hidden />}
          >
            {t("fixes.rerun")}
          </Button>
        );
    }
  };

  return (
    <div className={cn("space-y-3", className)}>
      <div className="flex flex-wrap items-center gap-2">{fixes.map(renderButton)}</div>
      {fixes.map((fix, index) =>
        fix.kind === "instructions" && openInstructions.has(index) ? (
          <Alert key={index} variant="info" id={`${idBase}-instructions-${index}`}>
            <p className="whitespace-pre-line">{t(fix.code, fix.params)}</p>
          </Alert>
        ) : null,
      )}
      {error !== null && <ErrorBanner error={error} />}
    </div>
  );
}
