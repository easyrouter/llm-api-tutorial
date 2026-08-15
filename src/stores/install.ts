/**
 * Install-step state that must survive navigating away from the Install screen.
 *
 * - `requested`: targets the user explicitly asked to install from the Environment screen
 *   ("Install X" fix button). They are shown on the Install screen even when the snapshot alone
 *   would not derive them.
 * - `skipped`: targets the user chose to skip. Persisted so the Next gate on the Install screen
 *   does not reset when the user goes back and forth.
 *
 * Everything else about an install item (plan, running job, log lines, download progress) is
 * transient and lives in the Install screen's local reducer.
 */
import { create } from "zustand";

import type { InstallTarget } from "@/lib/types";

export interface InstallStoreState {
  requested: InstallTarget[];
  skipped: InstallTarget[];

  /** Remember that the user asked for `target` (idempotent). */
  request: (target: InstallTarget) => void;
  /** Forget a request, e.g. once the target is installed. */
  unrequest: (target: InstallTarget) => void;
  skip: (target: InstallTarget) => void;
  unskip: (target: InstallTarget) => void;
  reset: () => void;
}

const initial = { requested: [] as InstallTarget[], skipped: [] as InstallTarget[] };

function withTarget(list: InstallTarget[], target: InstallTarget): InstallTarget[] {
  return list.includes(target) ? list : [...list, target];
}

function withoutTarget(list: InstallTarget[], target: InstallTarget): InstallTarget[] {
  return list.filter((t) => t !== target);
}

export const useInstallStore = create<InstallStoreState>()((set) => ({
  ...initial,
  request: (target) => set((s) => ({ requested: withTarget(s.requested, target) })),
  unrequest: (target) => set((s) => ({ requested: withoutTarget(s.requested, target) })),
  skip: (target) => set((s) => ({ skipped: withTarget(s.skipped, target) })),
  unskip: (target) => set((s) => ({ skipped: withoutTarget(s.skipped, target) })),
  reset: () => set({ ...initial }),
}));
