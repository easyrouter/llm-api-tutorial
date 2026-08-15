import type { CSSProperties } from "react";
import { useTranslation } from "react-i18next";

import { useStickToBottom } from "@/hooks/useStickToBottom";
import { cn } from "@/lib/cn";
import type { OutputStream } from "@/lib/types";

/** Only the newest lines are rendered; older ones are summarised in a header note. */
export const MAX_RENDERED_LOG_LINES = 2000;

export interface LogLine {
  stream: OutputStream;
  line: string;
}

export interface LogViewProps {
  lines: LogLine[];
  /** CSS max-height of the scroll area (number = px). Default 240. */
  maxHeight?: number | string;
  /** Follow new output unless the user scrolled up. Default true. */
  autoScroll?: boolean;
  /** Text shown when there are no lines yet (defaults to `common:ui.log.empty`). */
  emptyText?: string;
  className?: string;
}

const streamStyles: Record<OutputStream, string> = {
  stdout: "text-neutral-100",
  stderr: "text-warning-500",
  system: "text-brand-100 italic",
};

/**
 * Terminal-style output pane for streamed install/verify output. stderr lines are tinted
 * warning, `system` lines (messages from this app, not the child process) brand + italic.
 * Renders at most the last `MAX_RENDERED_LOG_LINES` lines. Text is selectable.
 */
export function LogView({
  lines,
  maxHeight = 240,
  autoScroll = true,
  emptyText,
  className,
}: LogViewProps) {
  const { t } = useTranslation();
  const hidden = Math.max(0, lines.length - MAX_RENDERED_LOG_LINES);
  const visible = hidden > 0 ? lines.slice(hidden) : lines;
  const { ref, onScroll } = useStickToBottom<HTMLDivElement>(lines.length, autoScroll);
  const style: CSSProperties = {
    maxHeight: typeof maxHeight === "number" ? `${maxHeight}px` : maxHeight,
  };

  return (
    <div
      ref={ref}
      onScroll={onScroll}
      role="log"
      tabIndex={0}
      data-selectable
      style={style}
      className={cn(
        "focus-visible:ring-brand-500 overflow-y-auto rounded-md bg-neutral-900 p-3 font-mono text-xs leading-5 text-neutral-100 focus-visible:ring-2 focus-visible:outline-none dark:bg-neutral-950",
        className,
      )}
    >
      {hidden > 0 && (
        <div className="mb-1 text-neutral-400 italic" data-testid="log-truncated">
          {t("ui.log.truncated", { count: hidden })}
        </div>
      )}
      {visible.length === 0 ? (
        <div className="text-neutral-400 italic">{emptyText ?? t("ui.log.empty")}</div>
      ) : (
        <pre className="m-0 font-mono whitespace-pre-wrap">
          {visible.map((l, i) => (
            <span
              key={hidden + i}
              data-stream={l.stream}
              className={cn("block", streamStyles[l.stream])}
            >
              {l.line || " "}
            </span>
          ))}
        </pre>
      )}
    </div>
  );
}
