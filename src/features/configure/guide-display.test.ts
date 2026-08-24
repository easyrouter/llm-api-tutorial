import { beforeAll, describe, expect, it } from "vitest";

import i18n from "@/i18n";
import type { ConfigGuide, GuideStep } from "@/lib/types";

import {
  guideStepParams,
  liveCopyValue,
  modelHintText,
  protocolLabel,
  toolLabel,
  visibleSteps,
} from "./guide-display";

const t = (key: string, options?: Record<string, unknown>) => i18n.t(key, options);

const guide = (overrides: Partial<ConfigGuide["preset"]> = {}): ConfigGuide => ({
  tool: "codex",
  preset: {
    providerName: "Service Gateway",
    baseUrl: "https://gateway.example.com/v1",
    protocol: "responses",
    modelHint: "",
    reasoningEffortHint: "",
    ...overrides,
  },
  steps: [],
});

const step = (params: Record<string, string> = {}): GuideStep => ({
  id: "s",
  code: "select_tool_tab",
  params,
  copyValue: null,
  verifyCheck: null,
  branch: null,
});

describe("guide-display", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  it("labels protocols and tools", () => {
    expect(protocolLabel(t, "responses")).toBe("Responses");
    expect(protocolLabel(t, "chat_completions")).toBe("Chat Completions");
    expect(protocolLabel(t, "anthropic_messages")).toBe("Anthropic Messages");
    expect(toolLabel(t, "codex")).toBe("Codex CLI");
    expect(toolLabel(t, "claude-code")).toBe("Claude Code");
  });

  it("falls back to the 'use your assigned model' text when the hint is blank", () => {
    expect(modelHintText(t, "  gpt-5 ")).toBe("gpt-5");
    expect(modelHintText(t, "   ")).toMatch(/model you were assigned/);
  });

  it("always supplies tool / protocol / model_hint params", () => {
    const params = guideStepParams(t, guide(), step());
    expect(params).toEqual({
      tool: "Codex CLI",
      protocol: "Responses",
      model_hint: expect.stringMatching(/assigned/) as string,
    });
  });

  it("translates known enum values in step params and keeps the rest verbatim", () => {
    const params = guideStepParams(
      t,
      guide({ modelHint: "gpt-5" }),
      step({ tool: "claude-code", protocol: "chat_completions", extra: "raw" }),
    );
    expect(params).toEqual({
      tool: "Claude Code",
      protocol: "Chat Completions",
      model_hint: "gpt-5",
      extra: "raw",
    });
  });

  it("does not translate unknown values of known keys", () => {
    expect(guideStepParams(t, guide(), step({ tool: "something-else" })).tool).toBe(
      "something-else",
    );
  });

  it("live values win over the step params and the preset", () => {
    const live = { providerName: " My Gateway ", baseUrl: "https://x.example/v1", model: "gpt-5" };
    const params = guideStepParams(
      t,
      guide(),
      step({ provider_name: "Service Gateway", base_url: "https://preset.example/v1" }),
      live,
    );
    expect(params.provider_name).toBe("My Gateway");
    expect(params.base_url).toBe("https://x.example/v1");
    expect(params.model_hint).toBe("gpt-5");
    // an emptied live model falls back to the "use your assigned model" text
    const cleared = guideStepParams(t, guide({ modelHint: "gpt-5" }), step(), {
      providerName: "",
      baseUrl: "",
      model: "  ",
    });
    expect(cleared.model_hint).toMatch(/assigned/);
  });

  it("liveCopyValue overrides the copyable value per step code", () => {
    const at = (code: string, copyValue: string | null): GuideStep => ({
      id: code,
      code,
      params: {},
      copyValue,
      verifyCheck: null,
      branch: null,
    });
    const live = { providerName: "My Gateway", baseUrl: "https://x.example/v1", model: "gpt-5" };
    expect(liveCopyValue(at("add_provider", "Service Gateway"), live)).toBe("My Gateway");
    expect(liveCopyValue(at("paste_base_url", "https://preset.example/v1"), live)).toBe(
      "https://x.example/v1",
    );
    expect(liveCopyValue(at("set_model", null), live)).toBe("gpt-5");
    expect(
      liveCopyValue(at("set_model", null), { providerName: "", baseUrl: "", model: " " }),
    ).toBeNull();
    // untouched steps and the no-live case keep the Rust value
    expect(liveCopyValue(at("open_cc_switch", null), live)).toBeNull();
    expect(liveCopyValue(at("paste_base_url", "keep"), undefined)).toBe("keep");
    // an emptied live field falls back to the preset copy value
    expect(
      liveCopyValue(at("add_provider", "Service Gateway"), {
        providerName: " ",
        baseUrl: "",
        model: "",
      }),
    ).toBe("Service Gateway");
  });

  it("visibleSteps keeps shared steps, filters by branch and preserves order", () => {
    const at = (code: string, branch: GuideStep["branch"]): GuideStep => ({
      id: code,
      code,
      params: {},
      copyValue: null,
      verifyCheck: null,
      branch,
    });
    const steps = [
      at("open_cc_switch", null),
      at("add_official_provider", "chatgpt_login"),
      at("login_chatgpt", "chatgpt_login"),
      at("add_provider", "api_key"),
      at("paste_api_key", "api_key"),
      at("apply_codex_config", null),
    ];
    expect(visibleSteps(steps, "chatgpt_login").map((s) => s.code)).toEqual([
      "open_cc_switch",
      "add_official_provider",
      "login_chatgpt",
      "apply_codex_config",
    ]);
    expect(visibleSteps(steps, "api_key").map((s) => s.code)).toEqual([
      "open_cc_switch",
      "add_provider",
      "paste_api_key",
      "apply_codex_config",
    ]);
    // no branch (Claude Code): everything is shown, in order
    expect(visibleSteps(steps, null)).toHaveLength(6);
  });
});
