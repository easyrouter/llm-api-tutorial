import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";

import i18n from "@/i18n";
import type {
  AppConfig,
  DiagnoseRequest,
  Diagnosis,
  TelemetryEvent,
  TerminalProcess,
  VerifyRequest,
  VerifyResult,
} from "@/lib/types";
import { useAppStore } from "@/stores/app";
import { useWizardStore } from "@/stores/wizard";
import { mockInvoke, rejectWith, setInvokeHandlers, wireError } from "@/test/mocks/tauri";

import { VerifyScreen } from "./VerifyScreen";

const BASE_URL = "https://gateway.example.com/v1";

const config: AppConfig = {
  schemaVersion: 1,
  company: { name: "Company", supportContact: "it@example.com" },
  gateway: {
    baseUrl: BASE_URL,
    protocol: "responses",
    presetProviderName: "Service Gateway",
    defaultModel: "gpt-5-codex",
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
  ccSwitch: {
    githubRepo: "x/y",
    releasesApi: "",
    downloadPage: "",
    intranetMirror: "",
    dataDir: "~/.cc-switch",
  },
  codexApp: {
    storeProductId: "",
    windowsMsixX64: "",
    windowsMsixArm64: "",
    macosDmg: "",
    downloadPage: "",
  },
  requirements: {
    nodeMinVersion: "18.0.0",
    nodeRecommendedLts: "22",
    os: {
      windows: { minBuild: 19041, label: "Win10" },
      macos: { minVersion: "12.0", label: "macOS 12" },
    },
  },
  mirrors: { npmRegistries: [], nodeDist: [], probeTimeoutMs: 1000 },
  envVarsToInspect: [],
  docs: { baseUrl: "", indexPath: "index.json", cacheTtlSeconds: 60 },
  telemetry: { enabled: false, endpoint: "", flushIntervalSeconds: 30 },
};

const authDiagnosis: Diagnosis = {
  ruleId: "A",
  severity: "blocking",
  code: "auth.key_checklist",
  params: {},
  actions: [{ kind: "rerun" }],
  checklist: ["whitespace"],
};

const envDiagnosis: Diagnosis = {
  ruleId: "D",
  severity: "warning",
  code: "env.conflict",
  params: { name: "OPENAI_API_KEY" },
  actions: [],
  checklist: ["inspect"],
};

const terminals: TerminalProcess[] = [
  { pid: 100, name: "WindowsTerminal.exe" },
  { pid: 200, name: "Code.exe" },
];

function okResult(request: VerifyRequest): VerifyResult {
  return {
    tool: request.tool,
    cli: {
      ok: true,
      version: "0.42.0",
      path: "C:\\npm\\codex.cmd",
      outputTail: null,
      errorClass: null,
    },
    gateway: request.gateway
      ? { ok: true, httpStatus: 200, latencyMs: 250, errorClass: null, message: null }
      : null,
    runningTerminals: [],
    ok: true,
    diagnoses: [],
  };
}

function authFailure(request: VerifyRequest): VerifyResult {
  return {
    tool: request.tool,
    cli: {
      ok: true,
      version: "0.42.0",
      path: "C:\\npm\\codex.cmd",
      outputTail: null,
      errorClass: null,
    },
    gateway: {
      ok: false,
      httpStatus: 401,
      latencyMs: 90,
      errorClass: "auth",
      message: "invalid api key ****",
    },
    runningTerminals: terminals,
    ok: false,
    diagnoses: [authDiagnosis],
  };
}

async function clickVerify() {
  await act(async () => {
    fireEvent.click(screen.getByTestId("verify-button"));
    await Promise.resolve();
    await Promise.resolve();
  });
}

describe("VerifyScreen", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  beforeEach(() => {
    useWizardStore.getState().reset();
    useWizardStore.getState().setSelectedTools(["codex"]);
    useWizardStore.getState().goTo("verify");
    useAppStore.setState({ config, info: null, loading: false, error: null });
    setInvokeHandlers({
      verify_setup: (args) => okResult(args?.request as VerifyRequest),
      list_running_terminals: () => [],
      diagnose: () => [],
    });
  });

  it("verifies the CLI only by default and unlocks Next when every tool is ok", async () => {
    render(<VerifyScreen />);
    const nextButton = screen.getByRole("button", { name: "Finish" });
    expect(nextButton).toBeDisabled();
    expect(screen.getByText(/Runs codex --version/)).toBeInTheDocument();

    await clickVerify();
    expect(mockInvoke).toHaveBeenCalledWith("verify_setup", {
      request: { tool: "codex", gateway: null },
    });
    const result = await screen.findByTestId("verify-result");
    expect(result).toHaveAttribute("data-ok", "true");
    expect(within(result).getByTestId("cli-check")).toHaveTextContent("0.42.0");
    expect(within(result).getByTestId("gateway-check")).toHaveTextContent(
      "Gateway not tested this time",
    );
    expect(screen.queryByTestId("diagnose-panel")).toBeNull();
    expect(useWizardStore.getState().verifyResults.codex?.ok).toBe(true);
    const trackCall = mockInvoke.mock.calls.find(([cmd]) => cmd === "track_event");
    const tracked = (trackCall?.[1] as { event: TelemetryEvent }).event;
    expect(tracked).toMatchObject({ name: "step_result", step: "verify", status: "pass" });
    expect(screen.getByText("All tools verified successfully.")).toBeInTheDocument();
    expect(nextButton).toBeEnabled();
  });

  it("probes the gateway with a key held only for the request, then clears it and opens diagnosis on failure", async () => {
    setInvokeHandlers({ verify_setup: (args) => authFailure(args?.request as VerifyRequest) });
    render(<VerifyScreen />);

    fireEvent.click(screen.getByTestId("gateway-toggle"));
    expect(screen.getByTestId("gateway-url")).toHaveValue(BASE_URL);
    expect(screen.getByTestId("gateway-model")).toHaveValue("gpt-5-codex");
    const keyInput = screen.getByTestId("gateway-key");
    expect(keyInput).toHaveAttribute("type", "password");
    expect(screen.getByText(/held in memory only for this one request/)).toBeInTheDocument();

    // missing key → no request, inline hint
    await clickVerify();
    expect(mockInvoke).not.toHaveBeenCalledWith("verify_setup", expect.anything());
    expect(screen.getByText(/Enter the gateway address, model and API key/)).toBeInTheDocument();

    fireEvent.change(keyInput, { target: { value: "sk-test-key" } });
    await clickVerify();
    expect(mockInvoke).toHaveBeenCalledWith("verify_setup", {
      request: {
        tool: "codex",
        gateway: {
          baseUrl: BASE_URL,
          apiKey: "sk-test-key",
          model: "gpt-5-codex",
          protocol: "responses",
        },
      },
    });
    expect(screen.getByTestId("gateway-key")).toHaveValue("");

    const result = await screen.findByTestId("verify-result");
    expect(result).toHaveAttribute("data-ok", "false");
    const gateway = within(result).getByTestId("gateway-check");
    expect(gateway).toHaveAttribute("data-status", "fail");
    expect(gateway).toHaveTextContent("401");
    expect(gateway).toHaveTextContent("Authentication failed (key rejected)");
    expect(gateway).toHaveTextContent("invalid api key ****");

    // running terminals warning
    const terminalsAlert = within(result).getByTestId("terminals-alert");
    expect(terminalsAlert).toHaveTextContent("2 terminal window(s) still running");
    expect(terminalsAlert).toHaveTextContent("WindowsTerminal.exe");
    expect(terminalsAlert).toHaveTextContent("Code.exe");

    // diagnosis panel auto-expanded with the engine's findings
    const panel = screen.getByTestId("diagnose-panel");
    expect(panel).toHaveTextContent(/Authentication failed: the gateway rejects your API key/);
    expect(panel).toHaveTextContent(/analysed the cause automatically/);

    // telemetry carries class + rule, never the key
    const trackCall = mockInvoke.mock.calls.find(([cmd]) => cmd === "track_event");
    const tracked = (trackCall?.[1] as { event: TelemetryEvent }).event;
    expect(tracked).toMatchObject({ status: "fail", errorClass: "auth", ruleId: "A" });
    expect(JSON.stringify(trackCall)).not.toContain("sk-test-key");

    // Next stays locked until "finish anyway"
    const nextButton = screen.getByRole("button", { name: "Finish" });
    expect(nextButton).toBeDisabled();
    fireEvent.click(screen.getByTestId("finish-anyway"));
    expect(nextButton).toBeEnabled();
  });

  it("re-checks running terminals on demand", async () => {
    setInvokeHandlers({
      verify_setup: (args) => authFailure(args?.request as VerifyRequest),
      list_running_terminals: () => [terminals[0]!],
    });
    render(<VerifyScreen />);
    await clickVerify();
    await screen.findByTestId("verify-result");

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Re-check terminals" }));
      await Promise.resolve();
    });
    expect(mockInvoke).toHaveBeenCalledWith("list_running_terminals");
    await waitFor(() =>
      expect(screen.getByTestId("terminals-alert")).toHaveTextContent(
        "1 terminal window(s) still running",
      ),
    );
    expect(screen.getByTestId("terminals-alert")).not.toHaveTextContent("Code.exe");
  });

  it("runs the rule engine with the full snapshot and merges the findings", async () => {
    const snapshot = { generatedAt: "now" } as never;
    useWizardStore.getState().setSnapshot(snapshot);
    setInvokeHandlers({
      verify_setup: (args) => authFailure(args?.request as VerifyRequest),
      diagnose: () => [authDiagnosis, envDiagnosis],
    });
    render(<VerifyScreen />);
    await clickVerify();
    await screen.findByTestId("diagnose-panel");

    await act(async () => {
      fireEvent.click(screen.getByTestId("run-diagnosis"));
      await Promise.resolve();
    });
    const call = mockInvoke.mock.calls.find(([cmd]) => cmd === "diagnose");
    const request = (call?.[1] as { request: DiagnoseRequest }).request;
    expect(request.snapshot).toBe(snapshot);
    expect(request.symptoms).toEqual([
      { kind: "http_status", status: 401, tool: "codex" },
      { kind: "no_effect_after_config", tool: "codex" },
    ]);

    const panel = await screen.findByTestId("diagnose-panel");
    await waitFor(() => expect(panel).toHaveTextContent("Environment variable conflict"));
    // de-duplicated: the auth finding appears once
    expect(within(panel).getAllByRole("article")).toHaveLength(2);
    expect(panel).toHaveTextContent(/have been merged in/);
  });

  it("shows a Rust error from verify_setup with a retry", async () => {
    setInvokeHandlers({
      verify_setup: rejectWith(
        wireError("command_timeout", "", { program: "codex", timeoutMs: "10000" }),
      ),
    });
    render(<VerifyScreen />);
    await clickVerify();
    expect(await screen.findByText(/Command codex timed out/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
  });

  it("renders one card per selected tool", () => {
    useWizardStore.getState().setSelectedTools(["codex", "claude-code"]);
    render(<VerifyScreen />);
    expect(screen.getByTestId("verify-card-codex")).toBeInTheDocument();
    expect(screen.getByTestId("verify-card-claude-code")).toHaveTextContent(
      /Runs claude --version/,
    );
  });

  it("probes the gateway for Claude Code with the Anthropic Messages protocol", async () => {
    useWizardStore.getState().setSelectedTools(["claude-code"]);
    render(<VerifyScreen />);
    const card = screen.getByTestId("verify-card-claude-code");
    fireEvent.click(within(card).getByTestId("gateway-toggle"));
    expect(card).toHaveTextContent("Protocol: Anthropic Messages");
    fireEvent.change(within(card).getByTestId("gateway-model"), {
      target: { value: "claude-sonnet-4-5" },
    });
    fireEvent.change(within(card).getByTestId("gateway-key"), { target: { value: "sk-ant-key" } });
    await clickVerify();
    expect(mockInvoke).toHaveBeenCalledWith("verify_setup", {
      request: {
        tool: "claude-code",
        gateway: {
          baseUrl: BASE_URL,
          apiKey: "sk-ant-key",
          model: "claude-sonnet-4-5",
          protocol: "anthropic_messages",
        },
      },
    });
  });

  it("warns inline when the gateway address is not https", () => {
    render(<VerifyScreen />);
    fireEvent.click(screen.getByTestId("gateway-toggle"));
    expect(screen.queryByTestId("gateway-url-not-https")).toBeNull();
    fireEvent.change(screen.getByTestId("gateway-url"), {
      target: { value: "http://gateway.example.com/v1" },
    });
    expect(screen.getByTestId("gateway-url-not-https")).toHaveTextContent(/not https/);
    fireEvent.change(screen.getByTestId("gateway-url"), {
      target: { value: "http://127.0.0.1:8080/v1" },
    });
    expect(screen.queryByTestId("gateway-url-not-https")).toBeNull();
  });
});
