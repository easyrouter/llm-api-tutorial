/**
 * Resolving the company gateway preset for one tool — the frontend mirror of
 * `config::gateway_defaults` in Rust.
 *
 * `AppConfig.gateway` carries the Codex defaults; `gateway.claudeCode` overrides the ones that
 * differ for Claude Code. Never read `config.gateway.baseUrl` / `defaultModel` for a specific
 * tool: go through `gatewayDefaults` so both tools get their own address and model.
 */
import type { AppConfig, Protocol, ToolId } from "@/lib/types";

/** Fallback protocol when the config has not loaded yet (the company preset's own default). */
const DEFAULT_PROTOCOL: Protocol = "responses";

/** Gateway values that apply to one tool. */
export interface GatewayDefaults {
  baseUrl: string;
  protocol: Protocol;
  model: string;
  /** Codex only — `model_reasoning_effort` is a Codex config key; empty for Claude Code. */
  reasoningEffort: string;
}

/**
 * Wire protocol of `tool`: Claude Code always speaks Anthropic Messages (it has no protocol
 * setting); Codex follows the company preset (Responses by default).
 */
export function toolProtocol(config: AppConfig | null, tool: ToolId): Protocol {
  if (tool === "claude-code") return "anthropic_messages";
  return config?.gateway.protocol ?? DEFAULT_PROTOCOL;
}

/**
 * Gateway defaults for `tool`. Claude Code takes `gateway.claudeCode`, falling back field by
 * field to the shared values when an entry is empty or missing (an IT override may replace the
 * whole `gateway` object) — the address falls back to the shared one with a trailing `/v1`
 * removed (`anthropicRootOf`).
 */
export function gatewayDefaults(config: AppConfig | null, tool: ToolId): GatewayDefaults {
  const gateway = config?.gateway;
  const protocol = toolProtocol(config, tool);
  if (tool === "codex") {
    return {
      baseUrl: gateway?.baseUrl ?? "",
      protocol,
      model: gateway?.defaultModel ?? "",
      reasoningEffort: gateway?.defaultReasoningEffort ?? "",
    };
  }
  return {
    baseUrl: gateway?.claudeCode?.baseUrl?.trim()
      ? gateway.claudeCode.baseUrl
      : anthropicRootOf(gateway?.baseUrl ?? ""),
    protocol,
    model: orShared(gateway?.claudeCode?.defaultModel, gateway?.defaultModel),
    reasoningEffort: "",
  };
}

/**
 * The Anthropic base URL derived from an OpenAI-shaped one: the same address without the
 * trailing `/v1` segment (mirrors `config::anthropic_root_of` in Rust). Only used as the
 * fallback when `gateway.claudeCode.baseUrl` is unset — a `/v1` base would make Claude Code
 * request `…/v1/v1/messages` and 404.
 */
export function anthropicRootOf(baseUrl: string): string {
  const trimmed = baseUrl.trim();
  const withoutSlash = trimmed.replace(/\/+$/, "");
  return withoutSlash.endsWith("/v1") ? withoutSlash.slice(0, -"/v1".length) : trimmed;
}

function orShared(value: string | undefined, shared: string | undefined): string {
  return value?.trim() ? value : (shared ?? "");
}
