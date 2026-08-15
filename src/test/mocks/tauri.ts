/**
 * Test doubles for the Tauri runtime (there is no `window.__TAURI_INTERNALS__` in jsdom).
 *
 * `src/test/setup.ts` installs these globally for every test file, so a screen test usually
 * needs no `vi.mock` at all — just import the handles and script them:
 *
 * ```ts
 * import {
 *   emitMockEvent, mockInvoke, mockWriteText, rejectWith, setInvokeHandlers, wireError,
 * } from "@/test/mocks/tauri";
 *
 * setInvokeHandlers({
 *   run_env_checks: () => snapshot,                              // by command name; value or promise
 *   plan_install: (args) => planFor(args?.target as InstallTarget),
 *   verify_setup: rejectWith(wireError("network")),               // reject like Rust does
 * });
 * render(<EnvCheckScreen />);
 * expect(mockInvoke).toHaveBeenCalledWith("run_env_checks", undefined);
 *
 * emitMockEvent("checks://progress", checkResult);          // drive listeners registered via lib/events.ts
 * expect(mockWriteText).toHaveBeenCalledWith("sk-…");     // clipboard writes (lib/clipboard.ts)
 * ```
 *
 * All handles are reset before each test (`resetTauriMocks`, wired in setup.ts): handlers are
 * cleared, `mockInvoke` resolves `undefined` for unknown commands, `mockWriteText` resolves,
 * `mockListen` records listeners so `emitMockEvent` can call them.
 *
 * If a test needs a completely different implementation of one of these modules it can still
 * call `vi.mock("@tauri-apps/api/core", () => ({ invoke: myFn }))` — a test-file `vi.mock`
 * takes precedence over the setup-file one for that file.
 *
 * The factories (`tauriCoreMock`, `clipboardMock`, `eventMock`) are exported for that case and
 * for suites with a custom setup file:
 * `vi.mock("@tauri-apps/api/core", async () => (await import("@/test/mocks/tauri")).tauriCoreMock())`
 * (the async import form is required because `vi.mock` is hoisted above imports).
 */
import { vi } from "vitest";

import type { Params, WireError } from "@/lib/types";

/** Handler for one command: receives the invoke args object and returns the response. */
export type InvokeHandler = (args: Record<string, unknown> | undefined) => unknown;

const invokeHandlers = new Map<string, InvokeHandler>();
const eventListeners = new Map<string, Set<(event: { payload: unknown }) => void>>();

/** Mock for `invoke` from `@tauri-apps/api/core`. */
export const mockInvoke = vi.fn((cmd: string, args?: Record<string, unknown>): Promise<unknown> => {
  const handler = invokeHandlers.get(cmd);
  return Promise.resolve(handler ? handler(args) : undefined);
});

/** Mock for `writeText` from `@tauri-apps/plugin-clipboard-manager`. */
export const mockWriteText = vi.fn((_text: string): Promise<void> => Promise.resolve());

/** Mock for `readText` from `@tauri-apps/plugin-clipboard-manager` (empty clipboard). */
export const mockReadText = vi.fn((): Promise<string> => Promise.resolve(""));

/** Mock for `listen` from `@tauri-apps/api/event`; resolves to an unlisten function. */
export const mockListen = vi.fn(
  (channel: string, cb: (event: { payload: unknown }) => void): Promise<() => void> => {
    let set = eventListeners.get(channel);
    if (!set) {
      set = new Set();
      eventListeners.set(channel, set);
    }
    set.add(cb);
    return Promise.resolve(() => {
      set.delete(cb);
    });
  },
);

/** Mock for `emit` from `@tauri-apps/api/event` (webview → Rust; nothing listens in tests). */
export const mockEmit = vi.fn((_channel: string, _payload?: unknown): Promise<void> =>
  Promise.resolve(),
);

/**
 * Registers per-command responses for `mockInvoke`. Handlers may return a value or a promise
 * (rejected promises reject the invoke, e.g. `() => Promise.reject(wireError)`).
 * Calling it again merges; use `resetTauriMocks()` to clear.
 */
export function setInvokeHandlers(handlers: Record<string, InvokeHandler>): void {
  for (const [cmd, handler] of Object.entries(handlers)) invokeHandlers.set(cmd, handler);
}

/** Builds the `WireError` shape Rust rejects with (`AppError` → `{ code, message, params }`). */
export function wireError(code: string, message = code, params: Params = {}): WireError {
  return { code, message, params };
}

/**
 * Invoke handler that rejects the way the Rust core does — with a plain `WireError` object,
 * not an `Error` instance: `setInvokeHandlers({ plan_install: rejectWith(wireError("io")) })`.
 */
export function rejectWith(error: WireError | Error): InvokeHandler {
  // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors -- mirrors Tauri, which rejects with the serialised AppError object
  return () => Promise.reject(error);
}

/** Delivers `payload` to every listener registered on `channel` (as Rust `emit` would). */
export function emitMockEvent(channel: string, payload: unknown): void {
  for (const cb of eventListeners.get(channel) ?? []) cb({ payload });
}

/** Number of live listeners on a channel — handy to assert that unlisten ran on unmount. */
export function listenerCount(channel: string): number {
  return eventListeners.get(channel)?.size ?? 0;
}

/**
 * Clears handlers, listeners, call history and any `mockResolvedValueOnce`-style overrides
 * (restoring the default implementations above). Called automatically before each test.
 */
export function resetTauriMocks(): void {
  invokeHandlers.clear();
  eventListeners.clear();
  mockInvoke.mockReset();
  mockWriteText.mockReset();
  mockReadText.mockReset();
  mockListen.mockReset();
  mockEmit.mockReset();
}

/** `vi.mock` factory for `@tauri-apps/api/core`. */
export const tauriCoreMock = () => ({ invoke: mockInvoke });

/** `vi.mock` factory for `@tauri-apps/plugin-clipboard-manager`. */
export const clipboardMock = () => ({ writeText: mockWriteText, readText: mockReadText });

/** `vi.mock` factory for `@tauri-apps/api/event`. */
export const eventMock = () => ({ listen: mockListen, once: mockListen, emit: mockEmit });
