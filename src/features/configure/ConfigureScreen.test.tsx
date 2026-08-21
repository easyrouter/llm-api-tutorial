import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";

import i18n from "@/i18n";
import type {
  CheckResult,
  CodexConfigStatus,
  ConfigGuide,
  ConnectivityReport,
  GatewayProbeRequest,
  GuideStep,
  KeyValidation,
  ToolId,
  UrlPreview,
} from "@/lib/types";
import { useWizardStore } from "@/stores/wizard";
import { mockInvoke, mockWriteText, setInvokeHandlers } from "@/test/mocks/tauri";

import { ConfigureScreen } from "./ConfigureScreen";

const BASE_URL = "https://gateway.example.com/v1";
const GOOD_KEY = "sk-abcdefghijklmnopqrstuvwxyz";
const MASKED_LINK = `ccswitch://v1/import?resource=provider&app=codex&name=Service+Gateway&apiKey=sk-****wxyz`;

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
    steps:
      tool === "codex"
        ? [
            step("open_cc_switch", null, { verifyCheck: "cc_switch" }),
            step("select_tool_tab", null, { params: { tool } }),
            step("add_official_provider", "chatgpt_login"),
            step("login_chatgpt", "chatgpt_login"),
            step("paste_base_url", "api_key", { copyValue: BASE_URL }),
            step("set_model", "api_key"),
            step("apply_codex_config", null),
          ]
        : [
            step("open_cc_switch", null, { verifyCheck: "cc_switch" }),
            step("select_tool_tab", null, { params: { tool } }),
            step("paste_base_url", null, { copyValue: BASE_URL }),
            step("set_model", null),
          ],
  };
}

function step(
  code: string,
  branch: GuideStep["branch"],
  overrides: Partial<Pick<GuideStep, "params" | "copyValue" | "verifyCheck">> = {},
): GuideStep {
  return { id: code, code, params: {}, copyValue: null, verifyCheck: null, branch, ...overrides };
}

const CONFIG_PATH = "C:\\Users\\me\\.codex\\config.toml";
const BACKUP_PATH = `${CONFIG_PATH}.seedrouter-20260819T120000.bak`;

function configStatus(overrides: Partial<CodexConfigStatus> = {}): CodexConfigStatus {
  return {
    path: CONFIG_PATH,
    exists: false,
    currentRedacted: null,
    backups: [],
    matchesTemplate: null,
    ...overrides,
  };
}

/** Number badges of the visible steps, in order. */
function stepNumbers(): string[] {
  return within(screen.getByTestId("guide-steps"))
    .getAllByRole("listitem")
    .map((li) => li.querySelector("span[aria-hidden]")?.textContent ?? "");
}

function stepCodes(): string[] {
  return within(screen.getByTestId("guide-steps"))
    .getAllByRole("listitem")
    .map((li) => li.getAttribute("data-step-code") ?? "");
}

function keyValidation(key: string): KeyValidation {
  const issues: KeyValidation["issues"] = [];
  if (key.trim().length === 0) issues.push("empty");
  if (key.length > 0 && key !== key.trim()) issues.push("leading_or_trailing_whitespace");
  if (key.trim() !== "" && !key.trim().startsWith("sk-")) issues.push("unexpected_prefix");
  return {
    valid: issues.every((i) => i === "unexpected_prefix"),
    issues,
    length: key.trim().length,
  };
}

function urlPreview(url: string): UrlPreview {
  const trimmed = url.trim().replace(/\/+$/, "");
  if (!trimmed.startsWith("http")) {
    return { input: url, effectiveUrl: "", rule: "invalid", warnings: [] };
  }
  return {
    input: url,
    effectiveUrl: trimmed.endsWith("/v1") ? trimmed : `${trimmed}/v1`,
    rule: trimmed.endsWith("/v1") ? "already_versioned" : "appended_v1",
    warnings: url.trim().endsWith("/") ? ["trailing_slash_removed"] : [],
  };
}

function connectivity(args: Record<string, unknown> | undefined): ConnectivityReport {
  const req = args?.request as GatewayProbeRequest;
  const url = urlPreview(req.baseUrl);
  const key = keyValidation(req.apiKey);
  const sent = url.rule !== "invalid" && key.valid;
  const gateway = sent
    ? { ok: true, httpStatus: 200, latencyMs: 42, errorClass: null, message: null }
    : null;
  const models = sent
    ? {
        gateway: { ok: true, httpStatus: 200, latencyMs: 18, errorClass: null, message: null },
        models: ["gpt-5", "gpt-5-codex"],
      }
    : null;
  return { url, key, models, gateway };
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
      test_connectivity: connectivity,
      list_gateway_models: () => ({
        gateway: { ok: true, httpStatus: 200, latencyMs: 20, errorClass: null, message: null },
        models: ["gpt-5", "gpt-5-codex"],
      }),
      get_codex_config_template: (args) => {
        const req = args?.request as { model: string; autoCompactScope: string };
        return [
          `model = "${req.model}"`,
          `model_auto_compact_token_limit_scope = "${req.autoCompactScope}"`,
          `experimental_bearer_token = "<API-KEY>"`,
        ].join("\n");
      },
      preview_cc_switch_import: () => ({ displayUrl: MASKED_LINK, app: "codex" }),
      open_cc_switch_import: () => undefined,
      run_env_check: () => passResult,
      codex_config_status: () => configStatus(),
      apply_codex_config: () => ({ path: CONFIG_PATH, backupPath: null, bytes: 100 }),
      restore_codex_config: () => ({ path: CONFIG_PATH, backupPath: null, bytes: 90 }),
    });
  });

  const keyInput = async () => await screen.findByTestId("key-input");

  it("renders the editable values, the codex client note and the numbered steps", async () => {
    render(<ConfigureScreen />);
    expect(await screen.findByText("Open CC Switch")).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledWith("get_config_guide", { tool: "codex" });

    // values card: provider name / base URL / protocol / model, all copyable
    const values = screen.getAllByTestId("copy-field-value").map((el) => el.textContent);
    expect(values).toContain("Service Gateway");
    expect(values).toContain(BASE_URL);
    expect(values).toContain("Responses");
    expect(values).toContain("gpt-5-codex");

    // the configuration covers Codex CLI + the Codex client (shared ~/.codex)
    expect(screen.getByTestId("codex-client-note")).toBeInTheDocument();

    // steps in order (API-key path by default: the sign-in steps are hidden), with params
    // interpolated and copy values rendered
    const steps = within(screen.getByTestId("guide-steps")).getAllByRole("listitem");
    expect(steps).toHaveLength(5);
    expect(stepCodes()).toEqual([
      "open_cc_switch",
      "select_tool_tab",
      "paste_base_url",
      "set_model",
      "apply_codex_config",
    ]);
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

  it("tests connectivity in place: key issues block the probe, a clean key reaches the gateway", async () => {
    render(<ConfigureScreen />);
    const input = await keyInput();
    expect(input).toHaveAttribute("type", "password");
    const testButton = screen.getByTestId("test-connectivity");
    expect(testButton).toBeDisabled();

    // a key with a blocking issue: format verdict shown, no gateway request sent
    fireEvent.change(input, { target: { value: ` ${GOOD_KEY}` } });
    fireEvent.click(testButton);
    const verdict = await screen.findByTestId("key-verdict");
    expect(verdict).toHaveAttribute("data-valid", "false");
    expect(within(verdict).getByText(/space \/ tab at the beginning or end/)).toBeInTheDocument();
    expect(screen.getByTestId("connectivity-not-sent")).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledWith("test_connectivity", {
      request: {
        baseUrl: BASE_URL,
        apiKey: ` ${GOOD_KEY}`,
        model: "gpt-5-codex",
        protocol: "responses",
      },
    });

    // a clean key: URL verdict + gateway result
    fireEvent.change(input, { target: { value: GOOD_KEY } });
    fireEvent.click(testButton);
    await waitFor(() =>
      expect(screen.getByTestId("key-verdict")).toHaveAttribute("data-valid", "true"),
    );
    expect(screen.getByTestId("url-verdict")).toHaveAttribute("data-rule", "already_versioned");
    const gateway = screen.getByTestId("gateway-check");
    expect(gateway).toHaveAttribute("data-status", "pass");
    const modelList = screen.getByTestId("model-list-check");
    expect(modelList).toHaveAttribute("data-status", "pass");
    expect(within(modelList).getByText("2")).toBeInTheDocument();
    expect(screen.queryByTestId("connectivity-not-sent")).toBeNull();
  });

  it("edits the base URL in place and uses it for the test and the steps", async () => {
    render(<ConfigureScreen />);
    await screen.findByText("Open CC Switch");

    fireEvent.click(screen.getByTestId("edit-base-url"));
    fireEvent.change(screen.getByTestId("input-base-url"), {
      target: { value: "https://gateway.example.com/" },
    });
    fireEvent.click(screen.getByTestId("done-base-url"));

    // the paste_base_url step copies the edited value
    const steps = within(screen.getByTestId("guide-steps")).getAllByRole("listitem");
    expect(within(steps[2]!).getByTestId("copy-field-value")).toHaveTextContent(
      "https://gateway.example.com/",
    );

    // and the connectivity test is run against it
    fireEvent.change(await keyInput(), { target: { value: GOOD_KEY } });
    fireEvent.click(screen.getByTestId("test-connectivity"));
    const verdict = await screen.findByTestId("url-verdict");
    expect(verdict).toHaveAttribute("data-rule", "appended_v1");
    expect(screen.getByText(/trailing slash is removed/)).toBeInTheDocument();
    expect(screen.getByText(/\/v1 is appended/)).toBeInTheDocument();
  });

  it("fetches the model list on the set_model step and picks a model with one click", async () => {
    render(<ConfigureScreen />);
    await screen.findByText("Open CC Switch");
    const fetchButton = screen.getByTestId("fetch-models");
    expect(fetchButton).toBeDisabled();

    fireEvent.change(await keyInput(), { target: { value: GOOD_KEY } });
    fireEvent.click(fetchButton);
    const list = await screen.findByTestId("models-list");
    expect(mockInvoke).toHaveBeenCalledWith("list_gateway_models", {
      request: { baseUrl: BASE_URL, apiKey: GOOD_KEY, model: "", protocol: "responses" },
    });

    fireEvent.click(within(list).getByRole("button", { name: "gpt-5" }));
    expect(screen.getByTestId("model-picked")).toHaveTextContent("gpt-5");
    // the values card now shows the picked model
    const modelRow = screen.getByTestId("row-model");
    expect(within(modelRow).getByTestId("copy-field-value")).toHaveTextContent(/^gpt-5$/);
  });

  it("imports into CC Switch only after showing the masked link and confirming", async () => {
    render(<ConfigureScreen />);
    await screen.findByText("Open CC Switch");
    const importButton = screen.getByTestId("cc-switch-import");
    expect(importButton).toBeDisabled();

    fireEvent.change(await keyInput(), { target: { value: GOOD_KEY } });
    fireEvent.click(importButton);

    // the dialog previews the masked deep link — the raw key never shows up
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByTestId("copy-field-value")).toHaveTextContent("apiKey=sk-****wxyz");
    expect(dialog).not.toHaveTextContent(GOOD_KEY);
    expect(mockInvoke).not.toHaveBeenCalledWith("open_cc_switch_import", expect.anything());

    fireEvent.click(within(dialog).getByRole("button", { name: "Open CC Switch" }));
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("open_cc_switch_import", {
        request: {
          tool: "codex",
          providerName: "Service Gateway",
          baseUrl: BASE_URL,
          apiKey: GOOD_KEY,
          model: "gpt-5-codex",
        },
      }),
    );
    expect(await screen.findByTestId("import-sent")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("offers the editable codex config template with scope choice and key substitution", async () => {
    render(<ConfigureScreen />);
    await screen.findByText("Open CC Switch");
    const textarea = screen.getByTestId<HTMLTextAreaElement>("config-toml-input");
    await waitFor(() => expect(textarea.value).toContain("body_after_prefix"));
    expect(textarea.value).toContain('model = "gpt-5-codex"');
    expect(textarea.value).toContain("<API-KEY>");

    // switching the accounting scope regenerates the template
    fireEvent.click(screen.getByTestId("config-scope-total"));
    await waitFor(() => expect(textarea.value).toContain('"total"'));

    // copying substitutes the real key for the placeholder; the screen never shows it
    fireEvent.change(await keyInput(), { target: { value: GOOD_KEY } });
    fireEvent.click(screen.getByTestId("config-toml-copy"));
    await waitFor(() => expect(mockWriteText).toHaveBeenCalled());
    const copiedText = mockWriteText.mock.calls.at(-1)?.[0] as string;
    expect(copiedText).toContain(`experimental_bearer_token = "${GOOD_KEY}"`);
    expect(copiedText).not.toContain("<API-KEY>");
    expect(textarea.value).toContain("<API-KEY>");
    expect(textarea.value).not.toContain(GOOD_KEY);

    // manual edits freeze regeneration until the template is restored
    fireEvent.change(textarea, {
      target: { value: `${textarea.value}\nmodel_auto_compact_token_limit = 100000` },
    });
    fireEvent.click(screen.getByTestId("config-scope-body_after_prefix"));
    await new Promise((resolve) => setTimeout(resolve, 400));
    expect(textarea.value).toContain("model_auto_compact_token_limit = 100000");
    expect(screen.getByTestId("config-toml-edited")).toBeInTheDocument();

    fireEvent.click(screen.getByTestId("config-toml-restore"));
    await waitFor(() =>
      expect(textarea.value).not.toContain("model_auto_compact_token_limit = 100000"),
    );
    expect(textarea.value).toContain("body_after_prefix");
  });

  it("switches tabs per tool and discards the typed key when leaving a tab", async () => {
    render(<ConfigureScreen />);
    const tabs = screen.getAllByRole("tab");
    expect(tabs.map((tab) => tab.textContent)).toEqual(["Codex CLI", "Claude Code"]);

    fireEvent.change(await keyInput(), { target: { value: "sk-secret" } });
    fireEvent.click(tabs[1]!);

    expect(await screen.findByTestId("tool-guide-claude-code")).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledWith("get_config_guide", { tool: "claude-code" });
    expect(screen.getByTestId("key-input")).toHaveValue("");
    // Claude Code shows the protocol note instead of a protocol value, and no codex extras
    expect(screen.getByText(/Claude Code has no protocol setting/)).toBeInTheDocument();
    expect(screen.queryByTestId("codex-client-note")).toBeNull();
    expect(screen.queryByTestId("config-toml-input")).toBeNull();
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

  it("switches the Codex account path: filters + renumbers steps and hides the import card", async () => {
    render(<ConfigureScreen />);
    await screen.findByText("Open CC Switch");
    // default: API-key path — custom provider + one-click import + the Windows patch card
    expect(screen.getByTestId("account-path-api_key")).toHaveAttribute("data-selected", "true");
    expect(screen.getByTestId("cc-switch-import")).toBeInTheDocument();
    expect(stepNumbers()).toEqual(["1", "2", "3", "4", "5"]);

    fireEvent.click(within(screen.getByTestId("account-path-chatgpt_login")).getByRole("radio"));
    expect(screen.getByTestId("account-path-chatgpt_login")).toHaveAttribute(
      "data-selected",
      "true",
    );
    expect(stepCodes()).toEqual([
      "open_cc_switch",
      "select_tool_tab",
      "add_official_provider",
      "login_chatgpt",
      "apply_codex_config",
    ]);
    expect(stepNumbers()).toEqual(["1", "2", "3", "4", "5"]);
    expect(screen.getByText("Add the “OpenAI Official” preset provider")).toBeInTheDocument();
    // no custom provider on the sign-in path: import card and Windows patch card are gone
    expect(screen.queryByTestId("cc-switch-import")).toBeNull();
    expect(screen.queryByTestId("codex-fast-ui")).toBeNull();
    // the values card explains the key only feeds config.toml; the template card stays
    expect(screen.getByText(/only goes into the config.toml template/)).toBeInTheDocument();
    expect(screen.getByTestId("config-toml-input")).toBeInTheDocument();

    // no account-path selector on the Claude Code tab
    fireEvent.click(screen.getAllByRole("tab")[1]!);
    await screen.findByTestId("tool-guide-claude-code");
    expect(screen.queryByTestId("account-path")).toBeNull();
    expect(screen.getByTestId("cc-switch-import")).toBeInTheDocument();
    expect(stepCodes()).toHaveLength(4);
  });

  it("applies the codex config only after a masked preview and confirmation", async () => {
    setInvokeHandlers({
      codex_config_status: () =>
        configStatus({ exists: true, matchesTemplate: false, backups: [BACKUP_PATH] }),
      apply_codex_config: () => ({ path: CONFIG_PATH, backupPath: BACKUP_PATH, bytes: 120 }),
    });
    render(<ConfigureScreen />);
    await screen.findByText("Open CC Switch");
    const textarea = screen.getByTestId<HTMLTextAreaElement>("config-toml-input");
    await waitFor(() => expect(textarea.value).toContain("<API-KEY>"));
    const status = await screen.findByTestId("config-status");
    expect(status).toHaveTextContent("differs from the template (1 backups)");

    // an empty key blocks the apply: no dialog, nothing written
    fireEvent.click(screen.getByTestId("config-apply"));
    expect(await screen.findByTestId("config-apply-needs-key")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(mockInvoke).not.toHaveBeenCalledWith("apply_codex_config", expect.anything());

    // with a key: the dialog shows path, backup note and a masked preview — never the real key
    fireEvent.change(await keyInput(), { target: { value: GOOD_KEY } });
    fireEvent.click(screen.getByTestId("config-apply"));
    const dialog = await screen.findByRole("dialog");
    expect(screen.queryByTestId("config-apply-needs-key")).toBeNull();
    expect(dialog).toHaveTextContent(CONFIG_PATH);
    expect(dialog).toHaveTextContent(/backed up first/);
    const preview = within(dialog).getByTestId("config-apply-preview");
    expect(preview).toHaveTextContent('experimental_bearer_token = "sk-****wxyz"');
    expect(preview).not.toHaveTextContent("<API-KEY>");
    expect(dialog).not.toHaveTextContent(GOOD_KEY);
    expect(within(dialog).getAllByRole("button", { name: /What does this do/ })).not.toHaveLength(
      0,
    );
    expect(mockInvoke).not.toHaveBeenCalledWith("apply_codex_config", expect.anything());

    // confirm → the real key is written (memory only), success shows path + backup
    fireEvent.click(within(dialog).getByRole("button", { name: "Write file" }));
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("apply_codex_config", {
        request: {
          content: expect.stringContaining(`experimental_bearer_token = "${GOOD_KEY}"`) as string,
        },
      }),
    );
    expect(mockInvoke).not.toHaveBeenCalledWith("apply_codex_config", {
      request: { content: expect.stringContaining("<API-KEY>") as string },
    });
    const applied = await screen.findByTestId("config-applied");
    expect(applied).toHaveTextContent(CONFIG_PATH);
    expect(applied).toHaveTextContent(BACKUP_PATH);
    expect(screen.queryByRole("dialog")).toBeNull();
    // the screen still never shows the real key
    expect(document.body).not.toHaveTextContent(GOOD_KEY);

    // restore: confirm first (destructive), then the newest backup is put back
    fireEvent.click(screen.getByTestId("config-restore"));
    const restoreDialog = await screen.findByRole("dialog");
    expect(restoreDialog).toHaveTextContent(BACKUP_PATH);
    expect(mockInvoke).not.toHaveBeenCalledWith("restore_codex_config");
    fireEvent.click(within(restoreDialog).getByRole("button", { name: "Restore backup" }));
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("restore_codex_config"));
    expect(await screen.findByTestId("config-restored")).toHaveTextContent(CONFIG_PATH);
  });
});
