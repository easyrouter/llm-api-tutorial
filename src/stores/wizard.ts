/**
 * Wizard state — the single source of truth for "where the user is" and what has been
 * established so far (environment snapshot, chosen tools, verification outcome).
 *
 * Rules
 * - Never store an API key here (or anywhere). The key lives only in the local state of the
 *   Verify screen for the duration of one request.
 * - Steps are linear but the user may jump back; `furthestStep` gates forward navigation.
 */
import { create } from "zustand";

import type { EnvSnapshot, ToolId, VerifyResult, WizardStep } from "@/lib/types";

export const WIZARD_STEPS: readonly WizardStep[] = [
  "welcome",
  "env_check",
  "install",
  "configure",
  "verify",
  "done",
] as const;

export function stepIndex(step: WizardStep): number {
  return WIZARD_STEPS.indexOf(step);
}

export interface WizardState {
  step: WizardStep;
  furthestStep: WizardStep;
  selectedTools: ToolId[];
  telemetryOptIn: boolean;
  snapshot: EnvSnapshot | null;
  verifyResults: Partial<Record<ToolId, VerifyResult>>;
  helpOpen: boolean;
  helpSectionId: string | null;

  goTo: (step: WizardStep) => void;
  next: () => void;
  back: () => void;
  setSelectedTools: (tools: ToolId[]) => void;
  setTelemetryOptIn: (on: boolean) => void;
  setSnapshot: (snapshot: EnvSnapshot | null) => void;
  setVerifyResult: (tool: ToolId, result: VerifyResult) => void;
  openHelp: (sectionId?: string | null) => void;
  closeHelp: () => void;
  reset: () => void;
}

const initial = {
  step: "welcome" as WizardStep,
  furthestStep: "welcome" as WizardStep,
  selectedTools: ["codex", "claude-code"] as ToolId[],
  telemetryOptIn: false,
  snapshot: null,
  verifyResults: {},
  helpOpen: false,
  helpSectionId: null,
};

export const useWizardStore = create<WizardState>()((set, get) => ({
  ...initial,

  goTo: (step) =>
    set((s) => ({
      step,
      furthestStep: stepIndex(step) > stepIndex(s.furthestStep) ? step : s.furthestStep,
    })),

  next: () => {
    const i = stepIndex(get().step);
    const target = WIZARD_STEPS[Math.min(i + 1, WIZARD_STEPS.length - 1)];
    if (target) get().goTo(target);
  },

  back: () => {
    const i = stepIndex(get().step);
    const target = WIZARD_STEPS[Math.max(i - 1, 0)];
    if (target) set({ step: target });
  },

  setSelectedTools: (tools) => set({ selectedTools: tools }),
  setTelemetryOptIn: (on) => set({ telemetryOptIn: on }),
  setSnapshot: (snapshot) => set({ snapshot }),
  setVerifyResult: (tool, result) =>
    set((s) => ({ verifyResults: { ...s.verifyResults, [tool]: result } })),
  openHelp: (sectionId = null) => set({ helpOpen: true, helpSectionId: sectionId }),
  closeHelp: () => set({ helpOpen: false }),
  reset: () => set({ ...initial }),
}));
