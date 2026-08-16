import { ArrowRight, ShieldCheck, Sparkles } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { setTelemetryEnabled, trackEvent } from "@/lib/tauri";
import type { ToolId } from "@/lib/types";
import { useAppStore } from "@/stores/app";
import { useWizardStore } from "@/stores/wizard";

const ALL_TOOLS: ToolId[] = ["codex", "claude-code"];

export function WelcomeScreen() {
  const { t } = useTranslation();
  const config = useAppStore((s) => s.config);
  const selected = useWizardStore((s) => s.selectedTools);
  const setSelected = useWizardStore((s) => s.setSelectedTools);
  const telemetryOptIn = useWizardStore((s) => s.telemetryOptIn);
  const setTelemetryOptIn = useWizardStore((s) => s.setTelemetryOptIn);
  const goTo = useWizardStore((s) => s.goTo);

  const telemetryConfigured = Boolean(config?.telemetry.endpoint);

  const toggleTool = (id: ToolId) => {
    const next = selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id];
    setSelected(next);
  };

  const start = async () => {
    if (telemetryConfigured) {
      await setTelemetryEnabled(telemetryOptIn).catch(() => undefined);
      await trackEvent({
        name: "wizard_start",
        step: "welcome",
        status: null,
        durationMs: null,
        errorClass: null,
        ruleId: null,
      }).catch(() => undefined);
    }
    goTo("env_check");
  };

  return (
    <div className="mx-auto max-w-3xl space-y-5">
      <div>
        <h1 className="text-2xl font-semibold">{t("welcome.title")}</h1>
        <p className="mt-2 text-sm leading-6 text-neutral-600 dark:text-neutral-300">
          {t("welcome.intro")}
        </p>
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <Card
          title={
            <span className="inline-flex items-center gap-2">
              <Sparkles className="text-brand-600 size-4" aria-hidden />
              {t("welcome.whatWeDo")}
            </span>
          }
        >
          <ul className="list-disc space-y-1.5 pl-5 text-sm text-neutral-700 dark:text-neutral-300">
            {(["check", "install", "guide", "verify"] as const).map((k) => (
              <li key={k}>{t(`welcome.whatWeDoItems.${k}`)}</li>
            ))}
          </ul>
        </Card>
        <Card
          title={
            <span className="inline-flex items-center gap-2">
              <ShieldCheck className="text-success-500 size-4" aria-hidden />
              {t("welcome.whatWeDont")}
            </span>
          }
        >
          <ul className="list-disc space-y-1.5 pl-5 text-sm text-neutral-700 dark:text-neutral-300">
            {(["noConfigWrite", "noEnvEdit", "noKeyStorage"] as const).map((k) => (
              <li key={k}>{t(`welcome.whatWeDontItems.${k}`)}</li>
            ))}
          </ul>
        </Card>
      </div>

      <Card title={t("welcome.selectTools")}>
        <div className="flex flex-wrap gap-x-8 gap-y-4">
          {ALL_TOOLS.map((id) => (
            <div key={id} className="space-y-1">
              <label className="inline-flex cursor-pointer items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  className="accent-brand-600 size-4"
                  checked={selected.includes(id)}
                  onChange={() => toggleTool(id)}
                />
                {t(`tools.${id}`)}
              </label>
              {id === "codex" && (
                <p className="pl-6 text-xs text-neutral-500">{t("welcome.codexClientHint")}</p>
              )}
            </div>
          ))}
        </div>
      </Card>

      {telemetryConfigured && (
        <Card>
          <p className="text-sm text-neutral-600 dark:text-neutral-300">
            {t("welcome.telemetryNotice")}
          </p>
          <label className="mt-3 inline-flex cursor-pointer items-center gap-2 text-sm">
            <input
              type="checkbox"
              className="accent-brand-600 size-4"
              checked={telemetryOptIn}
              onChange={(e) => setTelemetryOptIn(e.target.checked)}
            />
            {t("welcome.telemetryOptIn")}
          </label>
        </Card>
      )}

      <div className="flex justify-end">
        <Button
          onClick={() => void start()}
          disabled={selected.length === 0}
          leftIcon={<ArrowRight className="size-4" aria-hidden />}
        >
          {t("welcome.start")}
        </Button>
      </div>
    </div>
  );
}
