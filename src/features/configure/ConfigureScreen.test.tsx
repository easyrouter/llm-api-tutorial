import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";

import i18n from "@/i18n";
import type { CheckResult, ConfigGuide, KeyValidation, ToolId, UrlPreview } from "@/lib/types";
import { useWizardStore } from "@/stores/wizard";
import { mockInvoke, setInvokeHandlers } from "@/test/mocks/tauri";

import { ConfigureScreen } from "./ConfigureScreen";

const BASE_URL = "https://gateway.example.com/v1";

function guideFor(tool: ToolId): ConfigGuide {
  return {
    tool,
    preset: {
      providerName: "Service Gateway",
      baseUrl: BASE_URL,
      protocol: "responses",
      modelHint: tool === "codex" ? "gpt-5-codex" : "",
      reasoningEffortHint: "",
    },
    steps: [
      { id: "1", code: "open_cc_switch", params: {}, copyValue: null, verifyCheck: "cc_switch" },
      { id: "2", code: "select_tool_tab", params: { tool }, copyValue: null, verifyCheck: null },
      { id: "3", code: "paste_base_url", params: {}, copyValue: BASE_URL, verifyCheck: null },
      { id: "4", code: "set_model", params: {}, copyValue: null, verifyCheck: null },
    ],
  };
}

function keyValidation(key: string): KeyValidation {
  const issues: KeyValidation["issues"] = [];
  if (key !== key.trim()) issues.push("leading_or_trailing_whitespace");
  if (!key.trim().startsWith("sk-")) issues.push("unexpected_prefix");
  return {
    valid: issues.length === 0 || issues.every((i) => i === "unexpected_prefix"),
    issues,
    length: key.trim().length,
  };
}

function urlPreview(url: string): UrlPreview {
  const trimmed = url.trim().replace(/\/+$/, "");
  return {
    input: url,
    effectiveUrl: trimmed.endsWith("/v1") ? trimmed : `${trimmed}/v1`,
    rule: trimmed.endsWith("/v1") ? "already_versioned" : "appended_v1",
    warnings: url.endsWith("/") ? ["trailing_slash_removed"] : [],
  };
}

const passResult: CheckResult = {
  id: "cc_switch",
  status: "pass",
  code: "cc_switch.installed",
  params: {},
  details: [],
  fixes: [],
  durationMs: 5,
};

describe("ConfigureScreen", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
    i18n.addResourceBundle("en", "checks", { cc_switch: { installed: "CC Switch is installed" } });
  });

  beforeEach(() => {
    useWizardStore.getState().reset();
    useWizardStore.getState().goTo("configure");
    setInvokeHandlers({
      get_config_guide: (args) => guideFor(args?.tool as ToolId),
      validate_api_key: (args) => keyValidation(args?.key as string),
      preview_effective_url: (args) => urlPreview(args?.url as string),
      run_env_check: () => passResult,
    });
  });

  it("renders the preset values and the numbered steps from get_config_guide", async () => {
    render(<ConfigureScreen />);
    expect(await screen.findByText("Open CC Switch")).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledWith("get_config_guide", { tool: "codex" });

    // values card
    const values = screen.getAllByTestId("copy-field-value").map((el) => el.textContent);
    expect(values).toContain("Service Gateway");
    expect(values).toContain(BASE_URL);
    expect(values).toContain("Responses");
    expect(values).toContain("gpt-5-codex");

    // steps in order, with params interpolated and copy values rendered
    const steps = within(screen.getByTestId("guide-steps")).getAllByRole("listitem");
    expect(steps).toHaveLength(4);
    expect(steps[1]).toHaveTextContent("Switch to the Codex CLI tab");
    expect(within(steps[2]!).getByTestId("copy-field-value")).toHaveTextContent(BASE_URL);
    expect(steps[3]).toHaveTextContent("enter: gpt-5-codex");
  });

  it("re-runs the linked check when the user clicks 'I did this'", async () => {
    render(<ConfigureScreen />);
    const button = await screen.findByRole("button", { name: /I did this/ });
    fireEvent.click(button);
    expect(await screen.findByText("CC Switch is installed")).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledWith("run_env_check", { id: "cc_switch" });
    expect(screen.getByText("Pass")).toBeInTheDocument();
  });

  it("validates the key format and shows blocking issues vs hints", async () => {
    render(<ConfigureScreen />);
    const input = await screen.findByTestId("key-input");
    expect(input).toHaveAttribute("type", "password");

    fireEvent.change(input, { target: { value: " sk-abcdefghijklmnopqrstuvwxyz" } });
    const verdict = await screen.findByTestId("key-verdict");
    expect(verdict).toHaveAttribute("data-valid", "false");
    expect(mockInvoke).toHaveBeenCalledWith("validate_api_key", {
      key: " sk-abcdefghijklmnopqrstuvwxyz",
    });
    expect(within(verdict).getByText(/space \/ tab at the beginning or end/)).toBeInTheDocument();

    fireEvent.change(input, { target: { value: "gw-abcdefghijklmnopqrstuvwxyz" } });
    await waitFor(() =>
      expect(screen.getByTestId("key-verdict")).toHaveAttribute("data-valid", "true"),
    );
    expect(screen.getByText(/format looks fine \(29 characters\)/)).toBeInTheDocument();
    expect(screen.getByText(/Does not start with the usual sk-/)).toBeInTheDocument();
    // hints are not styled as blocking
    expect(screen.getByText(/Does not start/).closest("li")).toHaveAttribute("data-kind", "hint");
  });

  it("previews the effective URL for the preset and for edits", async () => {
    render(<ConfigureScreen />);
    const verdict = await screen.findByTestId("url-verdict");
    expect(mockInvoke).toHaveBeenCalledWith("preview_effective_url", { url: BASE_URL });
    expect(verdict).toHaveAttribute("data-rule", "already_versioned");
    expect(within(verdict).getByTestId("copy-field-value")).toHaveTextContent(BASE_URL);

    fireEvent.change(screen.getByTestId("url-input"), {
      target: { value: "https://gateway.example.com/" },
    });
    await waitFor(() =>
      expect(screen.getByTestId("url-verdict")).toHaveAttribute("data-rule", "appended_v1"),
    );
    expect(screen.getByText(/trailing slash is removed/)).toBeInTheDocument();
    expect(screen.getByText(/\/v1 is appended/)).toBeInTheDocument();
  });

  it("switches tabs per tool and discards the typed key when leaving a tab", async () => {
    render(<ConfigureScreen />);
    const tabs = screen.getAllByRole("tab");
    expect(tabs.map((tab) => tab.textContent)).toEqual(["Codex CLI", "Claude Code"]);

    const input = await screen.findByTestId("key-input");
    fireEvent.change(input, { target: { value: "sk-secret" } });
    fireEvent.click(tabs[1]!);

    expect(await screen.findByTestId("tool-guide-claude-code")).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledWith("get_config_guide", { tool: "claude-code" });
    expect(screen.getByTestId("key-input")).toHaveValue("");
    // Claude Code shows the protocol note instead of a protocol value
    expect(screen.getByText(/Claude Code has no protocol setting/)).toBeInTheDocument();
  });

  it("links the terminal reminder to the help section and advances to Verify", async () => {
    render(<ConfigureScreen />);
    await screen.findByText("Open CC Switch");
    fireEvent.click(screen.getByRole("button", { name: /Why is this needed/ }));
    expect(useWizardStore.getState().helpOpen).toBe(true);
    expect(useWizardStore.getState().helpSectionId).toBe("verify");

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /Go to verification/ }));
      await Promise.resolve();
    });
    expect(useWizardStore.getState().step).toBe("verify");
  });
});
