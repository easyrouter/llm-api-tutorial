/**
 * Vitest setup (runs before every test file).
 *
 * - jest-dom matchers
 * - i18n initialised with the real locale files (default language zh-CN; tests may call
 *   `i18n.changeLanguage("en")`)
 * - the Tauri runtime is replaced by the doubles in `./mocks/tauri` (invoke, clipboard, events)
 *   and reset before each test. See that file for how to script responses in a screen test.
 */
import "@testing-library/jest-dom/vitest";
import { beforeEach, vi } from "vitest";

import "../i18n";
import { resetTauriMocks } from "./mocks/tauri";

vi.mock("@tauri-apps/api/core", async () => (await import("./mocks/tauri")).tauriCoreMock());
vi.mock("@tauri-apps/plugin-clipboard-manager", async () =>
  (await import("./mocks/tauri")).clipboardMock(),
);
vi.mock("@tauri-apps/api/event", async () => (await import("./mocks/tauri")).eventMock());

beforeEach(() => {
  resetTauriMocks();
});
