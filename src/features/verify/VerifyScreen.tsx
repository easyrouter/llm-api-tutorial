import { SquareTerminal } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Alert, StepFooter } from "@/components/ui";
import { useWizardStore } from "@/stores/wizard";

import { VerifyCard } from "./VerifyCard";
import { unverifiedTools } from "./verify-logic";

/**
 * Verify step (M4): one `VerifyCard` per selected tool. "Next" (→ Done) unlocks when every
 * selected tool verified ok, or when the user explicitly ticks "finish anyway".
 */
export function VerifyScreen() {
  const { t } = useTranslation();
  const selectedTools = useWizardStore((s) => s.selectedTools);
  const verifyResults = useWizardStore((s) => s.verifyResults);
  const next = useWizardStore((s) => s.next);
  const back = useWizardStore((s) => s.back);
  const [finishAnyway, setFinishAnyway] = useState(false);

  const pending = unverifiedTools(selectedTools, verifyResults);
  const allOk = pending.length === 0;

  return (
    <div className="mx-auto max-w-3xl space-y-5">
      <div>
        <h1 className="text-2xl font-semibold">{t("verify:title")}</h1>
        <p className="mt-2 text-sm leading-6 text-neutral-600 dark:text-neutral-300">
          {t("verify:intro")}
        </p>
      </div>

      <Alert
        variant="warning"
        title={
          <span className="inline-flex items-center gap-2">
            <SquareTerminal className="size-4" aria-hidden />
            {t("verify:reminder.title")}
          </span>
        }
      >
        {t("verify:reminder.body")}
      </Alert>

      {selectedTools.map((tool) => (
        <VerifyCard key={tool} tool={tool} />
      ))}

      <StepFooter
        onBack={back}
        onNext={next}
        nextLabel={t("verify:footer.next")}
        nextDisabled={!allOk && !finishAnyway}
        extra={
          allOk ? (
            <span className="text-success-500 text-sm">{t("verify:footer.allOk")}</span>
          ) : (
            <label className="inline-flex cursor-pointer items-center gap-2 text-sm text-neutral-600 dark:text-neutral-300">
              <input
                type="checkbox"
                className="accent-brand-600 size-4"
                checked={finishAnyway}
                onChange={(e) => setFinishAnyway(e.target.checked)}
                data-testid="finish-anyway"
              />
              {t("verify:actions.finishAnyway")}
            </label>
          )
        }
      />
    </div>
  );
}
