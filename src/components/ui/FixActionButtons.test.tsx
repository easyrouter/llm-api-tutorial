import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import i18n from "@/i18n";
import type { EnvCleanupPlan, FixAction, PathRepairPlan } from "@/lib/types";
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

  describe("open_system_uri", () => {
    it("labels by uri, shows the region hint as a tooltip and calls open_system_uri", () => {
      render(
        <FixActionButtons
          fixes={[
            { kind: "open_system_uri", uri: "ms_store_codex_app" },
            { kind: "open_system_uri", uri: "windows_region_settings" },
          ]}
        />,
      );
      fireEvent.click(screen.getByRole("button", { name: "Open Microsoft Store" }));
      expect(mockInvoke).toHaveBeenCalledWith("open_system_uri", { uri: "ms_store_codex_app" });
      const region = screen.getByRole("button", { name: "Open region settings" });
      expect(region).toHaveAttribute(
        "title",
        expect.stringContaining("not available in your region"),
      );
      fireEvent.click(region);
      expect(mockInvoke).toHaveBeenCalledWith("open_system_uri", {
        uri: "windows_region_settings",
      });
    });

    it("surfaces a failure to open", async () => {
      setInvokeHandlers({ open_system_uri: rejectWith(wireError("unsupported", "nope")) });
      render(<FixActionButtons fixes={[{ kind: "open_system_uri", uri: "ms_store_codex_app" }]} />);
      await act(async () => {
        fireEvent.click(screen.getByRole("button", { name: "Open Microsoft Store" }));
        await Promise.resolve();
      });
      expect(screen.getByRole("alert")).toHaveTextContent("Not supported on this platform.");
    });
  });

  describe("repair_path", () => {
    const plan: PathRepairPlan = {
      dir: "C:\\Program Files\\nodejs",
      platform: "windows",
      location: "HKCU\\Environment\\Path",
      displayCommand: 'setx Path "%Path%;C:\\Program Files\\nodejs"',
      alreadyPresent: false,
      createsBackup: false,
    };

    it("plans on click, shows the plan in a confirm dialog, applies on confirm, then re-runs", async () => {
      const onRerun = vi.fn();
      setInvokeHandlers({
        plan_path_repair: () => plan,
        apply_path_repair: () => ({ changed: true, location: plan.location, backupPath: null }),
      });
      render(
        <FixActionButtons fixes={[{ kind: "repair_path", dir: plan.dir }]} onRerun={onRerun} />,
      );
      fireEvent.click(screen.getByRole("button", { name: "Fix PATH now" }));
      expect(mockInvoke).toHaveBeenCalledWith("plan_path_repair", { dir: plan.dir });

      const dialog = await screen.findByRole("dialog");
      await waitFor(() =>
        expect(within(dialog).getByTestId("copy-field-value")).toHaveTextContent(
          plan.displayCommand,
        ),
      );
      expect(dialog).toHaveTextContent("HKCU\\Environment\\Path");
      expect(dialog.querySelector('[data-variant="info"]')).toBeNull(); // no "already present" note
      // help link to the one-click doc section
      expect(dialog.querySelector('[data-help-section="one-click"]')).not.toBeNull();

      fireEvent.click(within(dialog).getByRole("button", { name: "Fix PATH" }));
      await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("apply_path_repair", { plan }));
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      const outcome = screen.getByTestId("remediation-outcome");
      expect(outcome).toHaveTextContent("Added C:\\Program Files\\nodejs to PATH");
      expect(outcome).toHaveTextContent("Close every terminal window");
      expect(onRerun).toHaveBeenCalledTimes(1);
    });

    it("explains an already-present dir and the backup, and reports a failed apply", async () => {
      setInvokeHandlers({
        plan_path_repair: () => ({
          ...plan,
          platform: "macos",
          location: "/Users/alice/.zshrc",
          alreadyPresent: true,
          createsBackup: true,
        }),
        apply_path_repair: rejectWith(wireError("io", "disk")),
      });
      render(<FixActionButtons fixes={[{ kind: "repair_path", dir: plan.dir }]} />);
      fireEvent.click(screen.getByRole("button", { name: "Fix PATH now" }));
      const dialog = await screen.findByRole("dialog");
      await waitFor(() =>
        expect(dialog.querySelector('[data-variant="info"]')).toHaveTextContent(
          "PATH already contains",
        ),
      );
      expect(dialog).toHaveTextContent("backed up first");

      fireEvent.click(within(dialog).getByRole("button", { name: "Fix PATH" }));
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      expect(screen.getByRole("alert")).toHaveTextContent("File read/write failed.");
      expect(screen.queryByTestId("remediation-outcome")).toBeNull();
    });

    it("cancel closes the dialog without applying", async () => {
      setInvokeHandlers({ plan_path_repair: () => plan });
      render(<FixActionButtons fixes={[{ kind: "repair_path", dir: plan.dir }]} />);
      fireEvent.click(screen.getByRole("button", { name: "Fix PATH now" }));
      const dialog = await screen.findByRole("dialog");
      fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
      expect(screen.queryByRole("dialog")).toBeNull();
      expect(mockInvoke).not.toHaveBeenCalledWith("apply_path_repair", expect.anything());
    });
  });

  describe("clean_env_vars", () => {
    const plan: EnvCleanupPlan = {
      platform: "windows",
      requiresAdmin: true,
      items: [
        {
          name: "ANTHROPIC_BASE_URL",
          source: { kind: "user_registry", location: "HKCU\\Environment" },
          value: "https://gw.example.com/v1",
          line: null,
          action: "delete_user_registry",
          displayCommand: "reg delete HKCU\\Environment /v ANTHROPIC_BASE_URL /f",
        },
        {
          name: "ANTHROPIC_AUTH_TOKEN",
          source: { kind: "machine_registry", location: "HKLM\\...\\Environment" },
          value: "sk-ant-full-token-value",
          line: null,
          action: "delete_machine_registry",
          displayCommand: "reg delete HKLM\\...\\Environment /v ANTHROPIC_AUTH_TOKEN /f",
        },
        {
          name: "ANTHROPIC_AUTH_TOKEN",
          source: { kind: "process", location: "" },
          value: "sk-ant-full-token-value",
          line: null,
          action: "none",
          displayCommand: "",
        },
      ],
    };

    it("lists every item with the FULL value (toggle hides), warns about admin, applies, re-runs", async () => {
      const onRerun = vi.fn();
      setInvokeHandlers({
        plan_env_cleanup: () => plan,
        apply_env_cleanup: () => ({
          removed: ["ANTHROPIC_BASE_URL"],
          failed: ["ANTHROPIC_AUTH_TOKEN"],
          backups: [],
        }),
      });
      const names = ["ANTHROPIC_BASE_URL", "ANTHROPIC_AUTH_TOKEN"];
      render(<FixActionButtons fixes={[{ kind: "clean_env_vars", names }]} onRerun={onRerun} />);
      fireEvent.click(screen.getByRole("button", { name: "Clean up environment variables" }));
      expect(mockInvoke).toHaveBeenCalledWith("plan_env_cleanup", { names });

      const dialog = await screen.findByRole("dialog");
      await waitFor(() =>
        expect(within(dialog).getAllByTestId("env-cleanup-item")).toHaveLength(3),
      );
      // full values shown by default
      const values = within(dialog).getAllByTestId("env-cleanup-value");
      expect(values[0]).toHaveTextContent("https://gw.example.com/v1");
      expect(values[1]).toHaveTextContent("sk-ant-full-token-value");
      // per-item action + command
      expect(dialog).toHaveTextContent("Delete from the Windows user variables");
      expect(dialog).toHaveTextContent("needs administrator rights, a UAC prompt will appear");
      expect(dialog).toHaveTextContent("Cannot remove: it is only set in the current process");
      expect(dialog).toHaveTextContent("reg delete HKCU\\Environment /v ANTHROPIC_BASE_URL /f");
      // admin warning + confirm label
      expect(within(dialog).getByRole("alert")).toHaveTextContent("administrator rights");
      expect(dialog.querySelector('[data-help-section="one-click"]')).not.toBeNull();

      fireEvent.click(within(dialog).getByRole("button", { name: "Hide full values" }));
      expect(within(dialog).getAllByTestId("env-cleanup-value")[0]).toHaveTextContent("(hidden)");
      fireEvent.click(within(dialog).getByRole("button", { name: "Show full values" }));
      expect(within(dialog).getAllByTestId("env-cleanup-value")[0]).toHaveTextContent(
        "https://gw.example.com/v1",
      );

      fireEvent.click(
        within(dialog).getByRole("button", { name: "Remove them (needs administrator rights)" }),
      );
      await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("apply_env_cleanup", { plan }));
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      const outcome = screen.getByTestId("remediation-outcome");
      expect(outcome).toHaveTextContent("Removed: ANTHROPIC_BASE_URL");
      expect(outcome).toHaveTextContent("Could not remove: ANTHROPIC_AUTH_TOKEN");
      expect(outcome).toHaveTextContent("Close every terminal window");
      expect(onRerun).toHaveBeenCalledTimes(1);
    });

    it("reports a planning failure inline and closes the dialog", async () => {
      setInvokeHandlers({ plan_env_cleanup: rejectWith(wireError("unsupported")) });
      render(<FixActionButtons fixes={[{ kind: "clean_env_vars", names: ["X"] }]} />);
      fireEvent.click(screen.getByRole("button", { name: "Clean up environment variables" }));
      await waitFor(() =>
        expect(screen.getByRole("alert")).toHaveTextContent("Not supported on this platform."),
      );
      expect(screen.queryByRole("dialog")).toBeNull();
    });
  });
});
