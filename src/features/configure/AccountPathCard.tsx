import { KeyRound, UserRound } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";

import { Card } from "@/components/ui";
import { cn } from "@/lib/cn";
import type { GuideBranch } from "@/lib/types";

const BRANCHES: readonly GuideBranch[] = ["chatgpt_login", "api_key"];

export interface AccountPathCardProps {
  branch: GuideBranch;
  onChange: (branch: GuideBranch) => void;
}

/**
 * Codex account-path selector (codex tab only). CC Switch ships a built-in "OpenAI Official"
 * preset for users who sign in with a ChatGPT account; only users without an account add a
 * custom provider with the gateway URL + API key. The choice filters the walkthrough steps and
 * the one-click cards below; it is kept in the parent's state and never persisted.
 */
export function AccountPathCard({ branch, onChange }: AccountPathCardProps) {
  const { t } = useTranslation();
  const name = useId();
  return (
    <Card title={t("guide:branch.title")} description={t("guide:branch.description")}>
      <div
        role="radiogroup"
        aria-label={t("guide:branch.title")}
        className="grid gap-3 sm:grid-cols-2"
        data-testid="account-path"
      >
        {BRANCHES.map((value) => {
          const selected = branch === value;
          const Icon = value === "chatgpt_login" ? UserRound : KeyRound;
          return (
            <label
              key={value}
              className={cn(
                "flex cursor-pointer items-start gap-3 rounded-md border p-3 text-sm transition-colors",
                selected
                  ? "border-brand-500 bg-brand-50 dark:bg-brand-700/15"
                  : "border-neutral-200 hover:border-neutral-400 dark:border-neutral-800 dark:hover:border-neutral-600",
              )}
              data-testid={`account-path-${value}`}
              data-selected={selected}
            >
              <input
                type="radio"
                name={name}
                value={value}
                checked={selected}
                onChange={() => onChange(value)}
                className="accent-brand-600 mt-1 size-4 shrink-0"
              />
              <span className="min-w-0">
                <span className="flex items-center gap-1.5 font-medium">
                  <Icon className="size-4 shrink-0" aria-hidden />
                  {t(`guide:branch.${value}.label`)}
                </span>
                <span className="mt-1 block leading-5 text-neutral-600 dark:text-neutral-400">
                  {t(`guide:branch.${value}.description`)}
                </span>
              </span>
            </label>
          );
        })}
      </div>
    </Card>
  );
}
