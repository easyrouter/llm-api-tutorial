/**
 * Help-docs state (M6): a per-language cache of the section index and of the pages the user
 * has opened, plus the last selected section per language.
 *
 * - The Rust side already runs the remote → cache → bundled cascade with its own TTL, so this
 *   store only avoids re-invoking within one app session; `force: true` refetches (Retry).
 * - Every slot is an `AsyncSlot`: `data` survives while a reload is in flight so the panel does
 *   not flicker, `error` is a normalised `WireError` ready for `ErrorBanner`.
 * - Late results (a `loadIndex` for a language the user already switched away from, or a
 *   superseded forced reload) are ignored so a slow response can never overwrite a newer one.
 * - Nothing here is persisted; the selection memory lives for the app session only.
 */
import { create } from "zustand";

import type { Lang } from "@/i18n";
import { toWireError } from "@/lib/errors";
import { fetchDocPage, fetchDocsIndex } from "@/lib/tauri";
import type { DocPage, DocsIndex, WireError } from "@/lib/types";

/** State of one cached document (index or page). */
export interface AsyncSlot<T> {
  data: T | null;
  loading: boolean;
  error: WireError | null;
}

const EMPTY_SLOT: AsyncSlot<never> = { data: null, loading: false, error: null };

/** Returns `slot` or an idle empty slot, so callers never deal with `undefined`. */
export function slotOf<T>(slot: AsyncSlot<T> | undefined): AsyncSlot<T> {
  return slot ?? EMPTY_SLOT;
}

/** Cache key of a page: `<lang>/<sectionId>`. */
export function pageKey(lang: Lang, id: string): string {
  return `${lang}/${id}`;
}

export interface LoadOptions {
  /** Refetch even when the slot already holds data (Retry / explicit reload). */
  force?: boolean;
}

export interface DocsState {
  /** Section index per language. */
  index: Partial<Record<Lang, AsyncSlot<DocsIndex>>>;
  /** Pages keyed by `pageKey(lang, id)`. */
  pages: Record<string, AsyncSlot<DocPage>>;
  /** Last selected section id per language (also the current selection while the panel is open). */
  selected: Partial<Record<Lang, string>>;

  /** Loads the index for `lang` (cached unless `force`). Resolves `undefined` on error. */
  loadIndex: (lang: Lang, options?: LoadOptions) => Promise<DocsIndex | undefined>;
  /** Loads one page (cached unless `force`). Resolves `undefined` on error. */
  loadPage: (lang: Lang, id: string, options?: LoadOptions) => Promise<DocPage | undefined>;
  /** Records the selected section for `lang`. */
  select: (lang: Lang, id: string) => void;
  /** Drops every cache and selection (tests, "start over"). */
  reset: () => void;
}

const initial = { index: {}, pages: {}, selected: {} };

export const useDocsStore = create<DocsState>()((set, get) => {
  // Monotonic request ids per cache key: a response only lands if it is still the latest
  // request for that key (see module doc).
  const latest = new Map<string, number>();
  let counter = 0;
  const begin = (key: string): (() => boolean) => {
    const id = ++counter;
    latest.set(key, id);
    return () => latest.get(key) === id;
  };

  const patchIndex = (lang: Lang, patch: Partial<AsyncSlot<DocsIndex>>) =>
    set((s) => ({ index: { ...s.index, [lang]: { ...slotOf(s.index[lang]), ...patch } } }));
  const patchPage = (key: string, patch: Partial<AsyncSlot<DocPage>>) =>
    set((s) => ({ pages: { ...s.pages, [key]: { ...slotOf(s.pages[key]), ...patch } } }));

  return {
    ...initial,

    loadIndex: async (lang, { force = false } = {}) => {
      const current = slotOf(get().index[lang]);
      if (!force && current.data) return current.data;
      if (!force && current.loading) return undefined;
      const isCurrent = begin(`index/${lang}`);
      patchIndex(lang, { loading: true, error: null });
      try {
        const data = await fetchDocsIndex(lang);
        if (!isCurrent()) return undefined;
        patchIndex(lang, { data, loading: false, error: null });
        return data;
      } catch (e) {
        if (!isCurrent()) return undefined;
        patchIndex(lang, { loading: false, error: toWireError(e) });
        return undefined;
      }
    },

    loadPage: async (lang, id, { force = false } = {}) => {
      const key = pageKey(lang, id);
      const current = slotOf(get().pages[key]);
      if (!force && current.data) return current.data;
      if (!force && current.loading) return undefined;
      const isCurrent = begin(`page/${key}`);
      patchPage(key, { loading: true, error: null });
      try {
        const data = await fetchDocPage(id, lang);
        if (!isCurrent()) return undefined;
        patchPage(key, { data, loading: false, error: null });
        return data;
      } catch (e) {
        if (!isCurrent()) return undefined;
        patchPage(key, { loading: false, error: toWireError(e) });
        return undefined;
      }
    },

    select: (lang, id) =>
      set((s) => (s.selected[lang] === id ? s : { selected: { ...s.selected, [lang]: id } })),

    reset: () => {
      latest.clear();
      set({ ...initial });
    },
  };
});
