/**
 * Display helpers for the configuration walkthrough (M3): human labels for wire enums and the
 * interpolation params handed to `guide:<step code>.title|body`.
 *
 * Rust sends raw values (`tool: "codex"`, `protocol: "responses"`); the UI turns them into
 * display names before interpolation and always supplies the params the step texts rely on
 * (`tool`, `protocol`, `model_hint`) so a missing param can never leak as `{{model_hint}}`.
 */
import type { Translate } from "@/lib/errors";
import type { ConfigGuide, GuideBranch, GuideStep, Params, Protocol, ToolId } from "@/lib/types";

const TOOL_IDS: readonly string[] = ["codex", "claude-code"];
const PROTOCOLS: readonly string[] = ["responses", "chat_completions", "anthropic_messages"];

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

/** Values the user may have edited in the provider card; they override the preset in steps. */
export interface LiveProviderValues {
  providerName: string;
  baseUrl: string;
  model: string;
}

/**
 * Params for `t("guide:<code>.title|body", params)`: UI defaults derived from the guide (tool
 * display name, protocol label, model hint) overridden by the step's own params, with known
 * enum values translated to display names. `live` values (edited in the provider card) win
 * over both so the steps always mirror what the user is actually going to paste.
 */
export function guideStepParams(
  t: Translate,
  guide: ConfigGuide,
  step: GuideStep,
  live?: LiveProviderValues,
): Params {
  const params: Params = {
    tool: toolLabel(t, guide.tool),
    protocol: protocolLabel(t, guide.preset.protocol),
    model_hint: modelHintText(t, guide.preset.modelHint),
  };
  for (const [key, value] of Object.entries(step.params)) {
    params[key] = displayValue(t, key, value);
  }
  if (live) {
    const name = live.providerName.trim();
    if (name) params.provider_name = name;
    const url = live.baseUrl.trim();
    if (url) params.base_url = url;
    params.model_hint = modelHintText(t, live.model);
  }
  return params;
}

/** The step's copyable value with live edits applied (`null` hides the copy field). */
export function liveCopyValue(step: GuideStep, live?: LiveProviderValues): string | null {
  if (!live) return step.copyValue;
  switch (step.code) {
    case "add_provider":
      return live.providerName.trim() || step.copyValue;
    case "paste_base_url":
      return live.baseUrl.trim() || step.copyValue;
    case "set_model":
      return live.model.trim() || null;
    default:
      return step.copyValue;
  }
}

/**
 * Steps visible for the chosen Codex account path: steps without a branch are shared, the rest
 * show only for their own branch. Without a branch (Claude Code) every step is shown.
 */
export function visibleSteps(steps: readonly GuideStep[], branch: GuideBranch | null): GuideStep[] {
  if (!branch) return [...steps];
  return steps.filter((step) => step.branch === null || step.branch === branch);
}
