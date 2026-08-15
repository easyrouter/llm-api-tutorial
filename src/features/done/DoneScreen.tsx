import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { useWizardStore } from "@/stores/wizard";

/** TODO(impl): replace this placeholder — see docs/ARCHITECTURE.md for the screen contract. */
export function DoneScreen() {
  const { t } = useTranslation();
  const next = useWizardStore((s) => s.next);
  const back = useWizardStore((s) => s.back);
  return (
    <div className="mx-auto max-w-3xl space-y-5">
      <h1 className="text-2xl font-semibold">{t("steps.done")}</h1>
      <Card>
        <p className="text-sm text-neutral-500">TODO</p>
      </Card>
      <div className="flex justify-between">
        <Button variant="secondary" onClick={back}>
          {t("actions.back")}
        </Button>
        <Button onClick={next}>{t("actions.next")}</Button>
      </div>
    </div>
  );
}
