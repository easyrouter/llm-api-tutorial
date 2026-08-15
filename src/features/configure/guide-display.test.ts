import { beforeAll, describe, expect, it } from "vitest";

import i18n from "@/i18n";
import type { ConfigGuide, GuideStep } from "@/lib/types";

import { guideStepParams, modelHintText, protocolLabel, toolLabel } from "./guide-display";

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
});

describe("guide-display", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  it("labels protocols and tools", () => {
    expect(protocolLabel(t, "responses")).toBe("Responses");
    expect(protocolLabel(t, "chat_completions")).toBe("Chat Completions");
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
});
