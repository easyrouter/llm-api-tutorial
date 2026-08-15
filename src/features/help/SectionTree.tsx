import { useTranslation } from "react-i18next";

import { cn } from "@/lib/cn";
import type { DocSection } from "@/lib/types";

export interface SectionTreeProps {
  sections: readonly DocSection[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  className?: string;
}

interface BranchProps {
  sections: readonly DocSection[];
  depth: number;
  selectedId: string | null;
  onSelect: (id: string) => void;
}

function Branch({ sections, depth, selectedId, onSelect }: BranchProps) {
  return (
    <ul
      className={cn(
        "space-y-0.5",
        depth > 0 && "mt-0.5 ml-3 border-l border-neutral-200 pl-2 dark:border-neutral-800",
      )}
    >
      {sections.map((s) => {
        const isSelected = s.id === selectedId;
        return (
          <li key={s.id}>
            <button
              type="button"
              onClick={() => onSelect(s.id)}
              aria-current={isSelected ? "page" : undefined}
              data-section-id={s.id}
              className={cn(
                "w-full rounded-md px-2 py-1 text-left text-sm transition-colors",
                isSelected
                  ? "bg-brand-50 text-brand-700 dark:bg-brand-700/20 dark:text-brand-100 font-medium"
                  : "text-neutral-700 hover:bg-neutral-100 dark:text-neutral-200 dark:hover:bg-neutral-800",
              )}
            >
              {s.title}
            </button>
            {s.children.length > 0 && (
              <Branch
                sections={s.children}
                depth={depth + 1}
                selectedId={selectedId}
                onSelect={onSelect}
              />
            )}
          </li>
        );
      })}
    </ul>
  );
}

/** Nested list of help sections (always expanded); the selected one carries `aria-current`. */
export function SectionTree({ sections, selectedId, onSelect, className }: SectionTreeProps) {
  const { t } = useTranslation("help");
  return (
    <nav aria-label={t("tree.label")} className={className}>
      <Branch sections={sections} depth={0} selectedId={selectedId} onSelect={onSelect} />
    </nav>
  );
}
