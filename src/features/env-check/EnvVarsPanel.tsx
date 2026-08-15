import { useTranslation } from "react-i18next";

import { Card } from "@/components/ui";
import { cn } from "@/lib/cn";
import type { EnvVarFinding } from "@/lib/types";

export interface EnvVarsPanelProps {
  findings: EnvVarFinding[];
  className?: string;
}

/**
 * Lists the environment variables the checks found (masked values, whether they are set in
 * this session, and where they come from). Renders nothing when there is nothing to show.
 * The tool never edits these variables (PRD #17) — the copy says so.
 */
export function EnvVarsPanel({ findings, className }: EnvVarsPanelProps) {
  const { t } = useTranslation();
  if (findings.length === 0) return null;

  return (
    <Card
      title={t("checks:envVars.title")}
      description={t("checks:envVars.intro")}
      className={className}
    >
      <ul className="divide-y divide-neutral-200 dark:divide-neutral-800">
        {findings.map((f) => (
          <li key={f.name} className="space-y-1.5 py-3 text-sm first:pt-0 last:pb-0">
            <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
              <code data-selectable className="font-mono font-semibold">
                {f.name}
              </code>
              <span
                className={cn(
                  "rounded-full border px-2 py-0.5 text-xs",
                  f.presentInSession
                    ? "border-warning-500/30 bg-warning-500/10 text-warning-500"
                    : "border-neutral-300 text-neutral-500 dark:border-neutral-700",
                )}
              >
                {t("checks:envVars.session")}:{" "}
                {f.presentInSession ? t("checks:envVars.present") : t("checks:envVars.absent")}
              </span>
            </div>
            <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-xs">
              <dt className="text-neutral-500">{t("checks:envVars.value")}</dt>
              <dd data-selectable className="font-mono break-all">
                {f.valueMasked ?? "—"}
              </dd>
              <dt className="text-neutral-500">{t("checks:envVars.sources")}</dt>
              <dd>
                {f.sources.length === 0 ? (
                  <span className="text-neutral-500">{t("checks:envVars.noSources")}</span>
                ) : (
                  <ul className="space-y-0.5">
                    {f.sources.map((s, i) => (
                      <li key={`${s.kind}-${i}`}>
                        <span className="text-neutral-700 dark:text-neutral-300">
                          {t(`checks:envVars.source.${s.kind}`, { defaultValue: s.kind })}
                        </span>
                        {s.location && (
                          <>
                            {" · "}
                            <code data-selectable className="font-mono break-all">
                              {s.location}
                            </code>
                          </>
                        )}
                      </li>
                    ))}
                  </ul>
                )}
              </dd>
            </dl>
          </li>
        ))}
      </ul>
    </Card>
  );
}
