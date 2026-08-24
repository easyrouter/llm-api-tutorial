import { describe, expect, it } from "vitest";

import { anthropicRootOf, gatewayDefaults, toolProtocol } from "@/lib/gateway";
import type { AppConfig } from "@/lib/types";

/** Minimal config: only the gateway section matters here. */
function config(gateway: Record<string, unknown>): AppConfig {
  return { gateway } as unknown as AppConfig;
}

const preset = {
  baseUrl: "https://gateway.example.com/v1",
  protocol: "responses",
  presetProviderName: "Service Gateway",
  defaultModel: "gpt-5.6-sol",
  defaultReasoningEffort: "medium",
  claudeCode: { baseUrl: "https://gateway.example.com", defaultModel: "claude-sonnet-5" },
};

describe("toolProtocol", () => {
  it("uses Anthropic Messages for Claude Code and the preset protocol for Codex", () => {
    const chat = config({ protocol: "chat_completions" });
    expect(toolProtocol(chat, "codex")).toBe("chat_completions");
    expect(toolProtocol(chat, "claude-code")).toBe("anthropic_messages");
    expect(toolProtocol(null, "codex")).toBe("responses");
    expect(toolProtocol(null, "claude-code")).toBe("anthropic_messages");
  });
});

describe("gatewayDefaults", () => {
  it("gives each tool its own address and model", () => {
    const cfg = config(preset);
    expect(gatewayDefaults(cfg, "codex")).toEqual({
      baseUrl: "https://gateway.example.com/v1",
      protocol: "responses",
      model: "gpt-5.6-sol",
      reasoningEffort: "medium",
    });
    expect(gatewayDefaults(cfg, "claude-code")).toEqual({
      // The root, not `/v1`: Claude Code appends `/v1/messages` itself.
      baseUrl: "https://gateway.example.com",
      protocol: "anthropic_messages",
      model: "claude-sonnet-5",
      // `model_reasoning_effort` is a Codex config key.
      reasoningEffort: "",
    });
  });

  it("falls back to the shared values when the Claude Code block is empty or missing", () => {
    const empty = config({ ...preset, claudeCode: { baseUrl: "  ", defaultModel: "" } });
    const claude = gatewayDefaults(empty, "claude-code");
    // The address falls back to the shared one *without* the `/v1` Claude Code must not get.
    expect(claude.baseUrl).toBe("https://gateway.example.com");
    expect(claude.model).toBe("gpt-5.6-sol");
    expect(claude.protocol).toBe("anthropic_messages");

    const missing = config({ ...preset, claudeCode: undefined });
    expect(gatewayDefaults(missing, "claude-code").baseUrl).toBe("https://gateway.example.com");
  });

  it("strips only a trailing /v1 when deriving the Anthropic root", () => {
    const cases: Array<[string, string]> = [
      ["https://gw.example/v1", "https://gw.example"],
      ["https://gw.example/v1/", "https://gw.example"],
      ["https://gw.example", "https://gw.example"],
      ["  https://gw.example/v1  ", "https://gw.example"],
      ["https://gw.example/openai/v1x", "https://gw.example/openai/v1x"],
      ["https://gw.example/api", "https://gw.example/api"],
      ["", ""],
    ];
    for (const [input, expected] of cases) expect(anthropicRootOf(input)).toBe(expected);
  });

  it("is empty rather than undefined before the config has loaded", () => {
    for (const tool of ["codex", "claude-code"] as const) {
      const defaults = gatewayDefaults(null, tool);
      expect(defaults.baseUrl).toBe("");
      expect(defaults.model).toBe("");
    }
  });
});
