/**
 * Pure logic for the Verify step (M4): deriving rule-engine symptoms from a `VerifyResult`,
 * the telemetry event for the step, and the "may I finish?" gate. No I/O, no i18n.
 */
import { primaryRuleId } from "@/features/diagnose/diagnoses";
import type {
  AppConfig,
  ErrorClass,
  Symptom,
  TelemetryEvent,
  ToolId,
  VerifyResult,
} from "@/lib/types";

/** Target label used in network symptoms; matches the rule engine's vocabulary. */
export const GATEWAY_TARGET = "gateway";

/**
 * Translates an unsuccessful `VerifyResult` into `Symptom`s for `diagnose`:
 * - CLI: `command_not_found` → CommandNotFound; any other failure → CommandFailed (with the
 *   redacted output tail);
 * - gateway: an HTTP status → HttpStatus; network / TLS → NetworkError; timeout → Timeout;
 *   protocol mismatch without a status → ProtocolMismatch;
 * - running terminals while something failed → NoEffectAfterConfig (fault C is only a hypothesis
 *   when there is an actual failure).
 * A fully successful result yields no symptoms.
 */
export function symptomsFromResult(result: VerifyResult): Symptom[] {
  const { tool, cli, gateway, runningTerminals } = result;
  const symptoms: Symptom[] = [];

  if (!cli.ok) {
    if (cli.errorClass === "command_not_found") {
      symptoms.push({ kind: "command_not_found", tool });
    } else {
      symptoms.push({ kind: "command_failed", tool, outputTail: cli.outputTail ?? "" });
    }
  }

  if (gateway && !gateway.ok) {
    if (gateway.httpStatus !== null) {
      symptoms.push({ kind: "http_status", status: gateway.httpStatus, tool });
    } else if (gateway.errorClass === "timeout") {
      symptoms.push({ kind: "timeout", target: GATEWAY_TARGET });
    } else if (gateway.errorClass === "protocol_mismatch") {
      symptoms.push({ kind: "protocol_mismatch", tool });
    } else if (gateway.errorClass === "network" || gateway.errorClass === "tls") {
      symptoms.push({ kind: "network_error", target: GATEWAY_TARGET });
    }
  }

  if (!result.ok && runningTerminals.length > 0) {
    symptoms.push({ kind: "no_effect_after_config", tool });
  }

  return symptoms;
}

/** The error class worth reporting: CLI first (it blocks everything), then the gateway. */
export function primaryErrorClass(result: VerifyResult): ErrorClass | null {
  if (!result.cli.ok && result.cli.errorClass) return result.cli.errorClass;
  if (result.gateway && !result.gateway.ok && result.gateway.errorClass) {
    return result.gateway.errorClass;
  }
  return null;
}

/** `step_result` telemetry event for one verification run (no URLs, paths or free text). */
export function verifyTelemetryEvent(result: VerifyResult, durationMs: number): TelemetryEvent {
  return {
    name: "step_result",
    step: "verify",
    status: result.ok ? "pass" : "fail",
    durationMs: Math.max(0, Math.round(durationMs)),
    errorClass: primaryErrorClass(result),
    ruleId: primaryRuleId(result.diagnoses),
  };
}

/** Selected tools that have not (yet) verified `ok`. */
export function unverifiedTools(
  selected: readonly ToolId[],
  results: Partial<Record<ToolId, VerifyResult>>,
): ToolId[] {
  return selected.filter((tool) => results[tool]?.ok !== true);
}

/** Binary name for a tool from the company preset (`codex`, `claude`); falls back to the id. */
export function toolBinary(config: AppConfig | null, tool: ToolId): string {
  return config?.tools.find((spec) => spec.id === tool)?.binary ?? tool;
}
