/**
 * Error normalisation for the UI.
 *
 * Rust rejects `invoke` promises with a serialised `AppError` (`{ code, message, params }`,
 * see `src-tauri/src/error.rs`). Anything else that can be thrown in the webview (an `Error`,
 * a string, `undefined`…) is coerced into the same shape with `code: "other"` so screens only
 * ever deal with one type. `describeError` turns it into a localised sentence via
 * `common:errors.<code>`; unknown codes fall back to `common:errors.generic`.
 */
import { isWireError, type WireError } from "./types";

/** Code used for anything that did not come from the Rust core as an `AppError`. */
export const OTHER_ERROR_CODE = "other";

/** Minimal shape of an i18next `t` function — keeps this module free of i18next generics. */
export type Translate = (key: string, options?: Record<string, unknown>) => string;

function messageOf(e: unknown): string {
  if (e instanceof Error) return e.message || e.name;
  if (typeof e === "string") return e;
  if (typeof e === "number" || typeof e === "boolean" || typeof e === "bigint") return String(e);
  if (typeof e === "object" && e !== null) {
    const message = (e as { message?: unknown }).message;
    if (typeof message === "string") return message;
    try {
      return JSON.stringify(e);
    } catch {
      return Object.prototype.toString.call(e);
    }
  }
  // null, undefined, symbols, functions: nothing meaningful to show.
  return "";
}

/** Coerces anything thrown/rejected into a `WireError`. Idempotent for real wire errors. */
export function toWireError(e: unknown): WireError {
  if (isWireError(e)) {
    return { code: e.code, message: e.message, params: e.params ?? {} };
  }
  return { code: OTHER_ERROR_CODE, message: messageOf(e), params: {} };
}

/** Convenience accessor: the machine code of any error (`"other"` when not a wire error). */
export function errorCode(e: unknown): string {
  return toWireError(e).code;
}

/**
 * Localised, user-facing description of an error.
 *
 * Looks up `errors.<code>` in the `common` namespace with the error params (plus `message`)
 * available for interpolation, and falls back to `errors.generic` (which shows the raw
 * message) when no translation exists for the code. Errors with the catch-all code `other`
 * also use `errors.generic` whenever they carry a message, so the detail is not lost.
 */
export function describeError(t: Translate, e: unknown): string {
  const wire = toWireError(e);
  const generic = t("errors.generic", { message: wire.message });
  if (wire.code === OTHER_ERROR_CODE && wire.message) return generic;
  const options: Record<string, unknown> = {
    ...wire.params,
    message: wire.message,
    defaultValue: generic,
  };
  const byCode = t(`errors.${wire.code}`, options);
  // Rust may name a more specific message than `errors.<code>` in the `reason` param — a fully
  // qualified key such as `guide:fastui.blocked.codex_app_missing`. Without this the blocking
  // reasons the core takes care to distinguish all render as one generic sentence.
  const reason = wire.params?.reason;
  if (typeof reason === "string" && reason.includes(":")) {
    return t(reason, { ...options, defaultValue: byCode });
  }
  return byCode;
}
