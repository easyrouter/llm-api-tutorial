import type { ReactNode } from "react";

import { cn } from "@/lib/cn";

export interface KeyValueItem {
  label: ReactNode;
  value: ReactNode;
  /** Monospace value (paths, versions, URLs). */
  mono?: boolean;
  /** Stable key when labels are not unique; defaults to the index. */
  key?: string;
}

export interface KeyValueListProps {
  items: KeyValueItem[];
  /** Two columns on wide layouts. Default false (single column, label left / value right). */
  columns?: 1 | 2;
  className?: string;
}

/** Definition list for facts about the machine (OS, versions, paths…). Values are selectable. */
export function KeyValueList({ items, columns = 1, className }: KeyValueListProps) {
  if (items.length === 0) return null;
  return (
    <dl
      className={cn(
        "grid gap-x-6 gap-y-2 text-sm",
        columns === 2 ? "sm:grid-cols-2" : "grid-cols-1",
        className,
      )}
    >
      {items.map((item, i) => (
        <div key={item.key ?? i} className="flex items-baseline justify-between gap-4">
          <dt className="shrink-0 text-neutral-500 dark:text-neutral-400">{item.label}</dt>
          <dd
            data-selectable
            className={cn(
              "min-w-0 text-right break-all text-neutral-900 dark:text-neutral-100",
              item.mono && "font-mono text-xs",
            )}
          >
            {item.value}
          </dd>
        </div>
      ))}
    </dl>
  );
}
