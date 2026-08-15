import { useEffect } from "react";
import { useTranslation } from "react-i18next";

import { AppShell } from "@/components/layout/AppShell";
import { ConfigureScreen } from "@/features/configure/ConfigureScreen";
import { DoneScreen } from "@/features/done/DoneScreen";
import { EnvCheckScreen } from "@/features/env-check/EnvCheckScreen";
import { InstallScreen } from "@/features/install/InstallScreen";
import { VerifyScreen } from "@/features/verify/VerifyScreen";
import { WelcomeScreen } from "@/features/welcome/WelcomeScreen";
import { detectInitialLang, setLang } from "@/i18n";
import { useAppStore } from "@/stores/app";
import { useWizardStore } from "@/stores/wizard";

/** Maps the current wizard step to its screen. `diagnose` is a panel inside Verify, not a step. */
function CurrentScreen() {
  const step = useWizardStore((s) => s.step);
  switch (step) {
    case "welcome":
      return <WelcomeScreen />;
    case "env_check":
      return <EnvCheckScreen />;
    case "install":
      return <InstallScreen />;
    case "configure":
      return <ConfigureScreen />;
    case "verify":
    case "diagnose":
      return <VerifyScreen />;
    case "done":
      return <DoneScreen />;
    default:
      return <WelcomeScreen />;
  }
}

export default function App() {
  const { t } = useTranslation();
  const bootstrap = useAppStore((s) => s.bootstrap);
  const info = useAppStore((s) => s.info);
  const error = useAppStore((s) => s.error);

  useEffect(() => {
    void bootstrap();
  }, [bootstrap]);

  // Once the OS locale hint arrives, settle the initial language (user choice still wins).
  useEffect(() => {
    if (info) void setLang(detectInitialLang(info.localeHint));
  }, [info]);

  if (error) {
    return (
      <div className="flex h-full items-center justify-center p-8">
        <div className="border-danger-500/40 max-w-md rounded-lg border bg-white p-6 shadow-sm dark:bg-neutral-900">
          <h1 className="text-danger-500 text-lg font-semibold">{t("errors.title")}</h1>
          <p className="mt-2 text-sm text-neutral-600 dark:text-neutral-300">
            {t("errors.generic", { message: error })}
          </p>
        </div>
      </div>
    );
  }

  return (
    <AppShell>
      <CurrentScreen />
    </AppShell>
  );
}
