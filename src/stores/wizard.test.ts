import { beforeEach, describe, expect, it } from "vitest";

import type { WizardStep } from "@/lib/types";

import { normalizeStep, stepIndex, useWizardStore, WIZARD_STEPS } from "./wizard";

const state = () => useWizardStore.getState();

describe("wizard store", () => {
  beforeEach(() => {
    state().reset();
  });

  it("normalises the diagnose pseudo-step to verify", () => {
    expect(normalizeStep("diagnose")).toBe("verify");
    expect(normalizeStep("verify")).toBe("verify");
    expect(stepIndex("diagnose")).toBe(stepIndex("verify"));
    expect(stepIndex("welcome")).toBe(0);
    expect(WIZARD_STEPS).not.toContain("diagnose");
  });

  it("goTo advances furthestStep only forwards", () => {
    state().goTo("configure");
    expect(state().step).toBe("configure");
    expect(state().furthestStep).toBe("configure");

    state().goTo("env_check");
    expect(state().step).toBe("env_check");
    expect(state().furthestStep).toBe("configure");
  });

  it("goTo(diagnose) lands on verify and next/back move relative to verify", () => {
    state().goTo("verify");
    state().goTo("diagnose");
    expect(state().step).toBe("verify");
    expect(state().furthestStep).toBe("verify");

    state().next();
    expect(state().step).toBe("done");

    state().goTo("diagnose");
    state().back();
    expect(state().step).toBe("configure");
  });

  it("next/back walk the linear flow and clamp at both ends", () => {
    const forward: WizardStep[] = [];
    for (let i = 0; i < WIZARD_STEPS.length; i += 1) {
      state().next();
      forward.push(state().step);
    }
    expect(forward).toEqual(["env_check", "install", "configure", "verify", "done", "done"]);
    expect(state().furthestStep).toBe("done");

    for (let i = 0; i < WIZARD_STEPS.length + 1; i += 1) state().back();
    expect(state().step).toBe("welcome");
    expect(state().furthestStep).toBe("done");
  });

  it("openHelp bumps the request counter even for the same section", () => {
    state().openHelp("faq");
    const first = state().helpRequestId;
    expect(state().helpOpen).toBe(true);
    expect(state().helpSectionId).toBe("faq");

    state().closeHelp();
    expect(state().helpOpen).toBe(false);

    state().openHelp("faq");
    expect(state().helpRequestId).toBe(first + 1);
    expect(state().helpOpen).toBe(true);
  });

  it("navigation lock and reset", () => {
    state().setNavigationLocked(true);
    state().goTo("install");
    state().setSelectedTools(["codex"]);
    expect(state().navigationLocked).toBe(true);

    state().reset();
    expect(state().navigationLocked).toBe(false);
    expect(state().step).toBe("welcome");
    expect(state().furthestStep).toBe("welcome");
    expect(state().selectedTools).toEqual(["codex", "claude-code"]);
    expect(state().helpRequestId).toBe(0);
  });
});
