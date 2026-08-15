/**
 * App-level state loaded once at startup: app info (version, platform, locale hint) and the
 * company preset config. Both come from the Rust core.
 */
import { create } from "zustand";

import { getAppConfig, getAppInfo } from "@/lib/tauri";
import type { AppConfig, AppInfo } from "@/lib/types";

export interface AppState {
  info: AppInfo | null;
  config: AppConfig | null;
  loading: boolean;
  error: string | null;
  bootstrap: () => Promise<void>;
}

export const useAppStore = create<AppState>()((set) => ({
  info: null,
  config: null,
  loading: false,
  error: null,

  bootstrap: async () => {
    set({ loading: true, error: null });
    try {
      const [info, config] = await Promise.all([getAppInfo(), getAppConfig()]);
      set({ info, config, loading: false });
    } catch (e) {
      set({ loading: false, error: e instanceof Error ? e.message : String(e) });
    }
  },
}));
