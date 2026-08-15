/**
 * Display helpers for the configuration walkthrough (M3): human labels for wire enums and the
 * interpolation params handed to `guide:<step code>.title|body`.
 *
 * Rust sends raw values (`tool: "codex"`, `protocol: "responses"`); the UI turns them into
 * display names before interpolation and always supplies the params the step texts rely on
 * (`tool`, `protocol`, `model_hint`) so a missing param can never leak as `{{model_hint}}`.
 */
import type { Translate } from "@/lib/errors";
import type { ConfigGuide, GuideStep, Params, Protocol, ToolId } from "@/lib/types";

const TOOL_IDS: readonly string[] = ["codex", "claude-code"];
const PROTOCOLS: readonly string[] = ["responses", "chat_completions"];

/** `Responses` / `Chat Completions`. */
export function protocolLabel(t: Translate, protocol: Protocol): string {
  return t(`guide:values.protocolValue.${protocol}`);
}

/** Display name of a tool from the common namespace (`Codex CLI`, `Claude Code`). */
export function toolLabel(t: Translate, tool: ToolId): string {
  return t(`common:tools.${tool}`);
}

/** The value shown for the model field: preset hint or the "use your assigned model" text. */
export function modelHintText(t: Translate, modelHint: string): string {
  return modelHint.trim() || t("guide:values.modelHintEmpty");
}

/** Maps a raw wire value to its display form when it is a known enum value; otherwise as-is. */
function displayValue(t: Translate, key: string, value: string): string {
  if (key === "tool" && TOOL_IDS.includes(value)) return toolLabel(t, value as ToolId);
  if (key === "protocol" && PROTOCOLS.includes(value)) {
    return protocolLabel(t, value as Protocol);
  }
  return value;
}

/**
 * Params for `t("guide:<code>.title|body", params)`: UI defaults derived from the guide (tool
 * display name, protocol label, model hint) overridden by the step's own params, with known
 * enum values translated to display names.
 */
export function guideStepParams(t: Translate, guide: ConfigGuide, step: GuideStep): Params {
  const params: Params = {
    tool: toolLabel(t, guide.tool),
    protocol: protocolLabel(t, guide.preset.protocol),
    model_hint: modelHintText(t, guide.preset.modelHint),
  };
  for (const [key, value] of Object.entries(step.params)) {
    params[key] = displayValue(t, key, value);
  }
  return params;
}
