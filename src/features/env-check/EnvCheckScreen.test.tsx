import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";

import i18n from "@/i18n";
import { EVENTS } from "@/lib/events";
import type { Diagnosis, EnvSnapshot } from "@/lib/types";
import { useInstallStore } from "@/stores/install";
import { useWizardStore } from "@/stores/wizard";
import { checkResult, envSnapshot, envVarFinding } from "@/test/fixtures/env";
import {
  emitMockEvent,
  listenerCount,
  mockInvoke,
  rejectWith,
  setInvokeHandlers,
  wireError,
} from "@/test/mocks/tauri";

import { EnvCheckScreen } from "./EnvCheckScreen";

/** A `run_env_checks` handler whose promise the test resolves by hand. */
function deferredChecks() {
  let resolve!: (snapshot: EnvSnapshot) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<EnvSnapshot>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  setInvokeHandlers({ run_env_checks: () => promise });
  return { resolve, reject };
}

function row(id: string) {
  const el = document.querySelector(`[data-check-id="${id}"]`);
  if (!el) throw new Error(`row ${id} not rendered`);
  return el as HTMLElement;
}

describe("EnvCheckScreen", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });
  beforeEach(() => {
    useWizardStore.getState().reset();
    useInstallStore.getState().reset();
    useWizardStore.getState().goTo("env_check");
  });

  it("runs the checks on mount, streams rows from events and stores the snapshot", async () => {
    const checks = deferredChecks();
    render(<EnvCheckScreen />);

    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("run_env_checks"));
    // listener registered before the run started
    expect(listenerCount(EVENTS.checkProgress)).toBe(1);
    expect(row("node")).toHaveAttribute("data-status", "running");
    expect(screen.getByRole("button", { name: "Next" })).toBeDisabled();

    act(() => {
      emitMockEvent(
        EVENTS.checkProgress,
        checkResult("node", "warn", {
          code: "node.too_old",
          params: { version: "16.20.0" },
          details: ["C:\\Program Files\\nodejs\\node.exe"],
        }),
      );
    });
    expect(row("node")).toHaveAttribute("data-status", "warn");
    expect(within(row("node")).getByText(/Node\.js 16\.20\.0 is too old/)).toBeInTheDocument();
    expect(row("npm")).toHaveAttribute("data-status", "running");

    // details block is collapsed until toggled
    const toggle = within(row("node")).getByRole("button", { name: "Show details" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(toggle);
    expect(within(row("node")).getByText(/nodejs\\node\.exe/)).toBeVisible();

    const snapshot = envSnapshot(
      { node: checkResult("node", "warn", { code: "node.too_old", params: { version: "16" } }) },
      { envVars: [envVarFinding()] },
    );
    await act(async () => {
      checks.resolve(snapshot);
      await Promise.resolve();
    });

    expect(useWizardStore.getState().snapshot).toEqual(snapshot);
    expect(screen.getByTestId("env-summary")).toHaveTextContent("9 passed, 1 warning");
    expect(screen.getByRole("button", { name: "Next" })).toBeEnabled();
    // env vars panel shows masked values and sources
    expect(screen.getByText("OPENAI_API_KEY")).toBeInTheDocument();
    expect(screen.getByText("sk-****abcd")).toBeInTheDocument();
    expect(screen.getByText("Windows user variables")).toBeInTheDocument();
    // telemetry (best effort)
    const tracked = mockInvoke.mock.calls.find((c) => c[0] === "track_event");
    expect(tracked?.[1]).toMatchObject({
      event: { name: "step_result", step: "env_check", status: "warn" },
    });
  });

  it("hides the rows of unselected tools and ignores their failures", async () => {
    useWizardStore.getState().setSelectedTools(["codex"]);
    setInvokeHandlers({
      run_env_checks: () =>
        envSnapshot({ claude_code: checkResult("claude_code", "fail", { code: "tool.missing" }) }),
    });
    render(<EnvCheckScreen />);
    await waitFor(() => expect(screen.getByRole("button", { name: "Next" })).toBeEnabled());
    expect(document.querySelector('[data-check-id="claude_code"]')).toBeNull();
    expect(row("codex")).toHaveAttribute("data-status", "pass");
    expect(screen.getByTestId("env-summary")).toHaveTextContent("All 9 checks passed");
  });

  it("blocks Next on failures unless they are install-fixable and acknowledged", async () => {
    setInvokeHandlers({
      run_env_checks: () =>
        envSnapshot({
          codex: checkResult("codex", "fail", {
            code: "tool.missing",
            params: { tool: "codex" },
            fixes: [{ kind: "install", tool: "codex" }, { kind: "rerun" }],
          }),
        }),
    });
    render(<EnvCheckScreen />);
    await waitFor(() => expect(screen.getByTestId("env-summary")).toHaveTextContent("1 blocked"));
    expect(
      within(row("codex")).getByText(
        "Codex CLI was not found; the next step can install it for you",
      ),
    ).toBeInTheDocument();

    const next = screen.getByRole("button", { name: "Next" });
    expect(next).toBeDisabled();
    const ack = screen.getByRole("checkbox");
    fireEvent.click(ack);
    expect(next).toBeEnabled();
    fireEvent.click(next);
    expect(useWizardStore.getState().step).toBe("install");
  });

  it("does not offer continue-anyway when a failure has no install fix", async () => {
    setInvokeHandlers({
      run_env_checks: () => envSnapshot({ os: checkResult("os", "fail", { code: "os.too_old" }) }),
    });
    render(<EnvCheckScreen />);
    await waitFor(() => expect(screen.getByTestId("env-summary")).toHaveTextContent("1 blocked"));
    expect(screen.queryByRole("checkbox")).toBeNull();
    expect(screen.getByRole("button", { name: "Next" })).toBeDisabled();
  });

  it("re-runs a single check and patches the stored snapshot", async () => {
    setInvokeHandlers({
      run_env_checks: () =>
        envSnapshot({
          codex: checkResult("codex", "fail", { code: "tool.missing", fixes: [{ kind: "rerun" }] }),
        }),
      run_env_check: () =>
        checkResult("codex", "pass", { code: "tool.ok", params: { version: "1.0" } }),
    });
    render(<EnvCheckScreen />);
    await waitFor(() => expect(row("codex")).toHaveAttribute("data-status", "fail"));

    fireEvent.click(within(row("codex")).getByRole("button", { name: "Re-check" }));
    expect(mockInvoke).toHaveBeenCalledWith("run_env_check", { id: "codex" });
    await waitFor(() => expect(row("codex")).toHaveAttribute("data-status", "pass"));
    const stored = useWizardStore.getState().snapshot;
    expect(stored?.checks.find((c) => c.id === "codex")?.status).toBe("pass");
    expect(stored?.overall).toBe("pass");
    expect(screen.getByRole("button", { name: "Next" })).toBeEnabled();
  });

  it("install fix: remembers the request and jumps to the Install step", async () => {
    setInvokeHandlers({
      run_env_checks: () =>
        envSnapshot({
          cc_switch: checkResult("cc_switch", "fail", {
            code: "cc_switch.missing",
            fixes: [{ kind: "install", tool: "cc-switch" }],
          }),
        }),
    });
    render(<EnvCheckScreen />);
    await waitFor(() => expect(row("cc_switch")).toHaveAttribute("data-status", "fail"));
    fireEvent.click(within(row("cc_switch")).getByRole("button", { name: "Install CC Switch" }));
    expect(useInstallStore.getState().requested).toEqual(["cc-switch"]);
    expect(useWizardStore.getState().step).toBe("install");
  });

  it("shows an error with retry when the run fails, and re-checks everything on demand", async () => {
    setInvokeHandlers({ run_env_checks: rejectWith(wireError("command_timeout")) });
    render(<EnvCheckScreen />);
    await waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());
    expect(screen.getByRole("alert")).toHaveTextContent("The environment check did not complete");
    expect(screen.getByRole("button", { name: "Next" })).toBeDisabled();

    setInvokeHandlers({ run_env_checks: () => envSnapshot() });
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Next" })).toBeEnabled());
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "run_env_checks")).toHaveLength(2);

    fireEvent.click(screen.getByRole("button", { name: "Re-check everything" }));
    await waitFor(() =>
      expect(mockInvoke.mock.calls.filter((c) => c[0] === "run_env_checks")).toHaveLength(3),
    );
  });

  it("unlistens on unmount", async () => {
    setInvokeHandlers({ run_env_checks: () => envSnapshot() });
    const { unmount } = render(<EnvCheckScreen />);
    await waitFor(() => expect(listenerCount(EVENTS.checkProgress)).toBe(1));
    unmount();
    expect(listenerCount(EVENTS.checkProgress)).toBe(0);
  });

  it("offers a snapshot-based diagnosis when a check is blocked", async () => {
    const snapshot = envSnapshot({
      codex: checkResult("codex", "fail", { code: "tool.not_on_path", fixes: [{ kind: "rerun" }] }),
    });
    const finding: Diagnosis = {
      ruleId: "E",
      severity: "blocking",
      code: "path.not_refreshed",
      params: { tool: "codex", binary: "codex", dir: "C:\\npm" },
      actions: [{ kind: "rerun" }],
      checklist: ["restart_terminal"],
    };
    setInvokeHandlers({ run_env_checks: () => snapshot, diagnose: () => [finding] });
    render(<EnvCheckScreen />);
    await waitFor(() => expect(screen.getByTestId("env-summary")).toHaveTextContent("1 blocked"));

    fireEvent.click(screen.getByTestId("env-diagnose"));
    await waitFor(() => expect(screen.getByTestId("diagnose-panel")).toBeInTheDocument());
    expect(mockInvoke).toHaveBeenCalledWith("diagnose", {
      request: { symptoms: [], snapshot },
    });
    expect(screen.getByText(i18n.t("diagnose:path.not_refreshed.title"))).toBeInTheDocument();

    // a re-run drops the stale findings; the offer comes back once the new run is blocked again
    fireEvent.click(screen.getByRole("button", { name: "Re-check everything" }));
    await waitFor(() =>
      expect(mockInvoke.mock.calls.filter((c) => c[0] === "run_env_checks")).toHaveLength(2),
    );
    await waitFor(() => expect(screen.getByTestId("env-summary")).toHaveTextContent("1 blocked"));
    expect(screen.queryByTestId("diagnose-panel")).toBeNull();
    expect(screen.getByTestId("env-diagnose")).toBeInTheDocument();
  });

  it("does not offer diagnosis when nothing is blocked, and links to the help section", async () => {
    setInvokeHandlers({ run_env_checks: () => envSnapshot() });
    render(<EnvCheckScreen />);
    await waitFor(() => expect(screen.getByRole("button", { name: "Next" })).toBeEnabled());
    expect(screen.queryByTestId("env-diagnose")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Learn more" }));
    expect(useWizardStore.getState().helpSectionId).toBe("env-check");
  });
});
