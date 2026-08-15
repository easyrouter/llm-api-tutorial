import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import i18n from "@/i18n";
import type { FixAction } from "@/lib/types";
import { useWizardStore } from "@/stores/wizard";
import { mockInvoke, rejectWith, setInvokeHandlers, wireError } from "@/test/mocks/tauri";

import { FixActionButtons } from "./FixActionButtons";

describe("FixActionButtons", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
    // Rust sends fully-qualified keys ("<ns>:<path>"); use a throwaway namespace here.
    i18n.addResourceBundle("en", "uikit_test", {
      env_vars: { instructions: "Remove {{name}} from your shell profile." },
    });
  });
  beforeEach(() => {
    useWizardStore.getState().reset();
  });

  it("renders nothing for an empty list", () => {
    const { container } = render(<FixActionButtons fixes={[]} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("open_url: labelled from fixes.labels and opened via open_external", () => {
    const fixes: FixAction[] = [
      { kind: "open_url", url: "https://nodejs.org/", labelCode: "node_download" },
      { kind: "open_url", url: "https://x.example", labelCode: "unknown_code" },
    ];
    render(<FixActionButtons fixes={fixes} />);
    fireEvent.click(screen.getByRole("button", { name: "Open Node.js download page" }));
    expect(mockInvoke).toHaveBeenCalledWith("open_external", { url: "https://nodejs.org/" });
    // unknown label codes fall back to the raw code so a missing translation is visible
    expect(screen.getByRole("button", { name: "Open unknown_code" })).toBeInTheDocument();
  });

  it("open_url: surfaces a failure to open", async () => {
    setInvokeHandlers({ open_external: rejectWith(wireError("invalid_input", "bad")) });
    render(<FixActionButtons fixes={[{ kind: "open_url", url: "x", labelCode: "docs" }]} />);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Open Help documentation" }));
      await Promise.resolve();
    });
    expect(screen.getByRole("alert")).toHaveTextContent("Invalid input.");
  });

  it("go_to_step: navigates the wizard store", () => {
    render(<FixActionButtons fixes={[{ kind: "go_to_step", step: "install" }]} />);
    fireEvent.click(screen.getByRole("button", { name: "Go to “Install”" }));
    expect(useWizardStore.getState().step).toBe("install");
  });

  it("install: calls onInstall with the target, or falls back to the Install step", () => {
    const onInstall = vi.fn();
    const fixes: FixAction[] = [{ kind: "install", tool: "cc-switch" }];
    const { unmount } = render(<FixActionButtons fixes={fixes} onInstall={onInstall} />);
    fireEvent.click(screen.getByRole("button", { name: "Install CC Switch" }));
    expect(onInstall).toHaveBeenCalledWith("cc-switch");
    expect(useWizardStore.getState().step).toBe("welcome");
    unmount();

    render(<FixActionButtons fixes={fixes} />);
    fireEvent.click(screen.getByRole("button", { name: "Install CC Switch" }));
    expect(useWizardStore.getState().step).toBe("install");
  });

  it("instructions: toggles an inline alert rendered from a fully-qualified key", () => {
    render(
      <FixActionButtons
        fixes={[
          { kind: "instructions", code: "uikit_test:env_vars.instructions", params: { name: "X" } },
        ]}
      />,
    );
    const toggle = screen.getByRole("button", { name: "Show instructions" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("status")).toBeNull();

    fireEvent.click(toggle);
    expect(screen.getByRole("status")).toHaveTextContent("Remove X from your shell profile.");
    const hide = screen.getByRole("button", { name: "Hide instructions" });
    expect(hide).toHaveAttribute("aria-expanded", "true");
    expect(hide).toHaveAttribute("aria-controls", screen.getByRole("status").id);

    fireEvent.click(hide);
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("rerun: calls onRerun, hidden without a handler", () => {
    const onRerun = vi.fn();
    const { unmount } = render(<FixActionButtons fixes={[{ kind: "rerun" }]} onRerun={onRerun} />);
    fireEvent.click(screen.getByRole("button", { name: "Re-check" }));
    expect(onRerun).toHaveBeenCalledTimes(1);
    unmount();
    const { container } = render(<FixActionButtons fixes={[{ kind: "rerun" }]} />);
    expect(container.querySelector("button")).toBeNull();
  });
});
