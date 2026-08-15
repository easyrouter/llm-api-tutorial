import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";

import i18n from "@/i18n";
import type { AppConfig, ToolId, VerifyResult } from "@/lib/types";
import { useAppStore } from "@/stores/app";
import { useInstallStore } from "@/stores/install";
import { useWizardStore } from "@/stores/wizard";
import { mockInvoke, mockWriteText } from "@/test/mocks/tauri";

import { DoneScreen } from "./DoneScreen";

const config: AppConfig = {
  schemaVersion: 1,
  company: { name: "Acme", supportContact: "it-support@acme.example" },
  gateway: {
    baseUrl: "https://gateway.acme.example/v1",
    protocol: "responses",
    presetProviderName: "Acme Gateway",
    defaultModel: "",
    defaultReasoningEffort: "",
  },
  tools: [
    {
      id: "codex",
      displayName: "Codex CLI",
      npmPackage: "@openai/codex",
      binary: "codex",
      versionArgs: ["--version"],
      configDir: "~/.codex",
    },
    {
      id: "claude-code",
      displayName: "Claude Code",
      npmPackage: "@anthropic-ai/claude-code",
      binary: "claude",
      versionArgs: ["--version"],
      configDir: "~/.claude",
    },
  ],
  ccSwitch: { githubRepo: "", releasesApi: "", downloadPage: "", intranetMirror: "", dataDir: "" },
  requirements: {
    nodeMinVersion: "18.0.0",
    nodeRecommendedLts: "22",
    os: {
      windows: { minBuild: 19041, label: "Windows 10 2004+" },
      macos: { minVersion: "12.0", label: "macOS 12+" },
    },
  },
  mirrors: { npmRegistries: [], nodeDist: [], probeTimeoutMs: 4000 },
  envVarsToInspect: [],
  docs: { baseUrl: "https://docs.acme.example", indexPath: "index.json", cacheTtlSeconds: 3600 },
  telemetry: { enabled: false, endpoint: "", flushIntervalSeconds: 30 },
};

const verifyResult = (tool: ToolId, ok: boolean, version: string | null): VerifyResult => ({
  tool,
  cli: { ok, version, path: null, outputTail: null, errorClass: ok ? null : "command_not_found" },
  gateway: null,
  runningTerminals: [],
  ok,
  diagnoses: [],
});

const row = (name: string) => screen.getByText(name).closest("li") as HTMLElement;

describe("DoneScreen", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });
  beforeEach(() => {
    useWizardStore.getState().reset();
    useAppStore.setState({ config, info: null, loading: false, error: null });
  });

  it("summarises the verify result of every selected tool", () => {
    useWizardStore.setState({
      step: "done",
      furthestStep: "done",
      selectedTools: ["codex", "claude-code"],
      verifyResults: {
        codex: verifyResult("codex", true, "0.42.0"),
        "claude-code": verifyResult("claude-code", false, null),
      },
    });
    render(<DoneScreen />);

    expect(screen.getByRole("heading", { level: 1, name: "Setup complete" })).toBeInTheDocument();

    const codex = row("Codex CLI");
    expect(within(codex).getByText("Verified · version 0.42.0")).toBeInTheDocument();
    expect(codex).toHaveAttribute("data-outcome", "ok");
    expect(within(codex).queryByRole("button", { name: "Back to Verify" })).toBeNull();

    const claude = row("Claude Code");
    expect(within(claude).getByText("Verification did not pass")).toBeInTheDocument();
    expect(claude).toHaveAttribute("data-outcome", "failed");
    fireEvent.click(within(claude).getByRole("button", { name: "Back to Verify" }));
    expect(useWizardStore.getState().step).toBe("verify");
  });

  it("marks tools that were never verified and links back to Verify", () => {
    useWizardStore.setState({ step: "done", selectedTools: ["codex"], verifyResults: {} });
    render(<DoneScreen />);
    const codex = row("Codex CLI");
    expect(within(codex).getByText("Not verified yet")).toBeInTheDocument();
    expect(within(codex).getByRole("button", { name: "Back to Verify" })).toBeInTheDocument();
  });

  it("shows next steps with the command per tool and the support contact", () => {
    useWizardStore.setState({ step: "done", selectedTools: ["claude-code"], verifyResults: {} });
    render(<DoneScreen />);

    expect(
      screen.getByText("Open a new terminal window and run claude to get started."),
    ).toBeInTheDocument();
    expect(screen.getByTestId("copy-field-value")).toHaveTextContent("claude");
    fireEvent.click(screen.getByRole("button", { name: /Copy/ }));
    expect(mockWriteText).toHaveBeenCalledWith("claude");
    expect(screen.getByText(/contact it-support@acme\.example/)).toBeInTheDocument();
  });

  it("start over resets the wizard and the install store; open help opens the FAQ", () => {
    useWizardStore.setState({
      step: "done",
      furthestStep: "done",
      selectedTools: ["codex"],
      verifyResults: { codex: verifyResult("codex", true, "1.0.0") },
    });
    useInstallStore.getState().skip("cc-switch");
    useInstallStore.getState().request("node");
    render(<DoneScreen />);

    fireEvent.click(screen.getByRole("button", { name: "Open help" }));
    expect(useWizardStore.getState().helpOpen).toBe(true);
    expect(useWizardStore.getState().helpSectionId).toBe("faq");

    fireEvent.click(screen.getByRole("button", { name: "Start over" }));
    expect(useWizardStore.getState().step).toBe("welcome");
    expect(useWizardStore.getState().verifyResults).toEqual({});
    expect(useInstallStore.getState().skipped).toEqual([]);
    expect(useInstallStore.getState().requested).toEqual([]);
  });

  it("explains an empty selection instead of rendering an empty list", () => {
    useWizardStore.setState({ step: "done", selectedTools: [], verifyResults: {} });
    render(<DoneScreen />);
    expect(screen.getByText(/No tools were selected/)).toBeInTheDocument();
    expect(screen.queryByRole("list")).toBeNull();
  });

  it("sends one wizard_done event only when telemetry is configured", async () => {
    useWizardStore.setState({
      step: "done",
      selectedTools: ["codex"],
      verifyResults: { codex: verifyResult("codex", true, "1.0.0") },
    });
    const { unmount } = render(<DoneScreen />);
    expect(mockInvoke).not.toHaveBeenCalledWith("track_event", expect.anything());
    unmount();

    useAppStore.setState({
      config: { ...config, telemetry: { ...config.telemetry, endpoint: "https://t.acme.example" } },
    });
    render(<DoneScreen />);
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("track_event", {
        event: {
          name: "wizard_done",
          step: "done",
          status: "ok",
          durationMs: null,
          errorClass: null,
          ruleId: null,
        },
      }),
    );
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "track_event")).toHaveLength(1);
  });
});
