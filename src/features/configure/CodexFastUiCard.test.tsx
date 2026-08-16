import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";

import i18n from "@/i18n";
import type { FastUiPlan, FastUiStatus } from "@/lib/types";
import { emitMockEvent, mockInvoke, setInvokeHandlers } from "@/test/mocks/tauri";

import { CodexFastUiCard } from "./CodexFastUiCard";

const INSTALL_ROOT = String.raw`C:\Users\me\AppData\Local\SeedRouter\CodexFastUI`;
const SHORTCUT = String.raw`${INSTALL_ROOT}\Codex Fast UI.lnk`;
const SHA256 = "3f1e6ae7c46ad0dbb1849f4f3ebab6798becb485bb1f10e73085cd702e48dc2a";
const COMMAND =
  String.raw`powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File ` +
  String.raw`"C:\cache\codex-fast-ui\v\CodexFastUI-Minimal-2026.07.30\install.ps1" ` +
  `-OutputRoot "${INSTALL_ROOT}"`;

function status(overrides: Partial<FastUiStatus> = {}): FastUiStatus {
  return {
    supported: true,
    toolkitVersion: "2026.07.30-minimal",
    testedCodexBuild: "OpenAI.Codex 26.721.11231.0",
    toolkitAvailable: true,
    codexAppFound: true,
    codexAppPath: String.raw`C:\Program Files\WindowsApps\OpenAI.Codex_26.721.11231.0_x64`,
    installed: false,
    installRoot: INSTALL_ROOT,
    shortcutPath: SHORTCUT,
    blockedCode: null,
    ...overrides,
  };
}

const installPlan: FastUiPlan = {
  action: "install",
  displayCommand: COMMAND,
  installRoot: INSTALL_ROOT,
  shortcutPath: SHORTCUT,
  toolkitVersion: "2026.07.30-minimal",
  toolkitSha256: SHA256,
  reinstall: false,
};

describe("CodexFastUiCard", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  beforeEach(() => {
    setInvokeHandlers({
      codex_fast_ui_status: () => status(),
      plan_codex_fast_ui: () => installPlan,
      start_codex_fast_ui: () => ({ jobId: "job-1", action: "install" }),
    });
  });

  it("renders nothing when the platform does not support the toolkit", async () => {
    setInvokeHandlers({ codex_fast_ui_status: () => status({ supported: false }) });
    const { container } = render(<CodexFastUiCard />);
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("codex_fast_ui_status"));
    expect(container).toBeEmptyDOMElement();
  });

  it("shows the exact command before running it and streams the job output", async () => {
    render(<CodexFastUiCard />);
    const card = await screen.findByTestId("codex-fast-ui");

    // The unofficial-patch caveat names the tested build, and nothing has run yet.
    expect(card).toHaveTextContent("OpenAI.Codex 26.721.11231.0");
    expect(mockInvoke).not.toHaveBeenCalledWith("start_codex_fast_ui", expect.anything());

    fireEvent.click(screen.getByTestId("fast-ui-install"));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent(COMMAND);
    expect(dialog).toHaveTextContent(SHA256);

    fireEvent.click(screen.getByRole("button", { name: "Run" }));
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("start_codex_fast_ui", { plan: installPlan }),
    );

    emitMockEvent("fastui://output", {
      jobId: "job-1",
      stream: "stdout",
      line: "Copying official Codex app",
    });
    // A foreign job must never leak into this card's log.
    emitMockEvent("fastui://output", { jobId: "other", stream: "stdout", line: "not mine" });
    const log = await screen.findByRole("log");
    expect(log).toHaveTextContent("Copying official Codex app");
    expect(log).not.toHaveTextContent("not mine");

    emitMockEvent("fastui://done", {
      jobId: "job-1",
      success: true,
      exitCode: 0,
      durationMs: 1000,
      cancelled: false,
      timedOut: false,
    });
    expect(await screen.findByTestId("fast-ui-result")).toHaveAttribute("data-variant", "success");
  });

  it("keeps verify and restore disabled until a copy exists", async () => {
    render(<CodexFastUiCard />);
    await screen.findByTestId("codex-fast-ui");
    expect(screen.getByTestId("fast-ui-install")).toBeEnabled();
    expect(screen.getByTestId("fast-ui-verify")).toBeDisabled();
    expect(screen.getByTestId("fast-ui-restore")).toBeDisabled();
  });

  it("offers verify and restore once installed, and rebuilds instead of installing", async () => {
    setInvokeHandlers({ codex_fast_ui_status: () => status({ installed: true }) });
    render(<CodexFastUiCard />);
    await screen.findByTestId("codex-fast-ui");
    expect(screen.getByTestId("fast-ui-verify")).toBeEnabled();
    expect(screen.getByTestId("fast-ui-restore")).toBeEnabled();
    expect(screen.getByTestId("fast-ui-install")).toHaveTextContent("Rebuild the patched copy");
    expect(screen.getByTestId("fast-ui-installed")).toHaveTextContent(SHORTCUT);
  });

  it("explains why installing is impossible and blocks it", async () => {
    setInvokeHandlers({
      codex_fast_ui_status: () =>
        status({ codexAppFound: false, blockedCode: "guide:fastui.blocked.codex_app_missing" }),
    });
    render(<CodexFastUiCard />);
    await screen.findByTestId("codex-fast-ui");
    expect(screen.getByTestId("fast-ui-blocked")).toHaveTextContent(
      "The official Codex client was not found",
    );
    expect(screen.getByTestId("fast-ui-install")).toBeDisabled();
  });
});
