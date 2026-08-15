/**
 * Clipboard access. The only place that talks to `@tauri-apps/plugin-clipboard-manager`, so
 * tests can mock one module (`src/test/mocks/tauri.ts`) and the UI kit stays decoupled from the
 * plugin API. Falls back to the browser clipboard when the Tauri runtime is absent
 * (plain `vite dev` in a browser), which keeps the UI usable while developing screens.
 *
 * Never log the text: callers may copy secrets (`CopyField secret`).
 */
import { writeText } from "@tauri-apps/plugin-clipboard-manager";

/** Writes `text` to the system clipboard. Rejects when neither backend is available. */
export async function copyText(text: string): Promise<void> {
  try {
    await writeText(text);
  } catch (tauriError) {
    const browserClipboard = globalThis.navigator?.clipboard;
    if (!browserClipboard?.writeText) throw tauriError;
    await browserClipboard.writeText(text);
  }
}
