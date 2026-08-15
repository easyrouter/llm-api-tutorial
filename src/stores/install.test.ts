import { beforeEach, describe, expect, it } from "vitest";

import { useInstallStore } from "./install";
import { startOver } from "./start-over";
import { useWizardStore } from "./wizard";

const state = () => useInstallStore.getState();

describe("install store", () => {
  beforeEach(() => {
    state().reset();
    useWizardStore.getState().reset();
  });

  it("request / unrequest are idempotent and order-preserving", () => {
    state().request("node");
    state().request("codex");
    state().request("node");
    expect(state().requested).toEqual(["node", "codex"]);

    state().unrequest("node");
    state().unrequest("node");
    expect(state().requested).toEqual(["codex"]);
  });

  it("skip / unskip are idempotent", () => {
    state().skip("cc-switch");
    state().skip("cc-switch");
    state().skip("claude-code");
    expect(state().skipped).toEqual(["cc-switch", "claude-code"]);

    state().unskip("cc-switch");
    expect(state().skipped).toEqual(["claude-code"]);
    state().unskip("node");
    expect(state().skipped).toEqual(["claude-code"]);
  });

  it("startOver clears the install store together with the wizard", () => {
    state().request("node");
    state().skip("cc-switch");
    useWizardStore.getState().goTo("done");

    startOver();
    expect(state().requested).toEqual([]);
    expect(state().skipped).toEqual([]);
    expect(useWizardStore.getState().step).toBe("welcome");
  });
});
