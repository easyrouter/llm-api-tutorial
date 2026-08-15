import { beforeEach, describe, expect, it } from "vitest";

import type { DocPage, DocsIndex } from "@/lib/types";
import { mockInvoke, rejectWith, setInvokeHandlers, wireError } from "@/test/mocks/tauri";

import { pageKey, slotOf, useDocsStore } from "./docs";

const index = (lang: string): DocsIndex => ({
  sections: [
    {
      id: "overview",
      title: `overview ${lang}`,
      path: "overview.md",
      lang,
      wizardStep: "welcome",
      children: [],
    },
  ],
  fetchedAt: "2026-01-01T00:00:00Z",
  source: "bundled",
});

const page = (id: string, lang: string): DocPage => ({
  id,
  title: `${id} ${lang}`,
  lang,
  markdown: `# ${id}\n\nbody`,
  source: "cache",
  fetchedAt: "2026-01-01T00:00:00Z",
});

/** Lets a handler resolve/reject on demand so tests can order responses. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("docs store", () => {
  beforeEach(() => {
    useDocsStore.getState().reset();
    setInvokeHandlers({
      fetch_docs_index: (args) => index(String(args?.lang)),
      fetch_doc_page: (args) => page(String(args?.id), String(args?.lang)),
    });
  });

  it("slotOf / pageKey helpers", () => {
    expect(slotOf(undefined)).toEqual({ data: null, loading: false, error: null });
    expect(pageKey("en", "faq")).toBe("en/faq");
  });

  it("loads and caches the index per language", async () => {
    const data = await useDocsStore.getState().loadIndex("en");
    expect(data?.sections[0]?.title).toBe("overview en");
    expect(useDocsStore.getState().index.en).toEqual({ data, loading: false, error: null });

    await useDocsStore.getState().loadIndex("en");
    expect(mockInvoke).toHaveBeenCalledTimes(1);

    await useDocsStore.getState().loadIndex("zh-CN");
    expect(mockInvoke).toHaveBeenCalledWith("fetch_docs_index", { lang: "zh-CN" });
    expect(useDocsStore.getState().index["zh-CN"]?.data?.sections[0]?.title).toBe("overview zh-CN");
  });

  it("force reloads and keeps the previous data while loading", async () => {
    await useDocsStore.getState().loadIndex("en");
    const pending = deferred<DocsIndex>();
    setInvokeHandlers({ fetch_docs_index: () => pending.promise });
    const run = useDocsStore.getState().loadIndex("en", { force: true });
    expect(useDocsStore.getState().index.en?.loading).toBe(true);
    expect(useDocsStore.getState().index.en?.data).not.toBeNull();
    pending.resolve({ ...index("en"), source: "remote" });
    await run;
    expect(useDocsStore.getState().index.en?.data?.source).toBe("remote");
    expect(useDocsStore.getState().index.en?.loading).toBe(false);
  });

  it("stores a normalised WireError and clears it on the next successful load", async () => {
    setInvokeHandlers({ fetch_docs_index: rejectWith(wireError("network", "offline")) });
    expect(await useDocsStore.getState().loadIndex("en")).toBeUndefined();
    expect(useDocsStore.getState().index.en).toEqual({
      data: null,
      loading: false,
      error: { code: "network", message: "offline", params: {} },
    });

    setInvokeHandlers({ fetch_docs_index: (args) => index(String(args?.lang)) });
    await useDocsStore.getState().loadIndex("en", { force: true });
    expect(useDocsStore.getState().index.en?.error).toBeNull();
  });

  it("does not start a second request while one is in flight (without force)", async () => {
    const pending = deferred<DocsIndex>();
    setInvokeHandlers({ fetch_docs_index: () => pending.promise });
    const first = useDocsStore.getState().loadIndex("en");
    const second = useDocsStore.getState().loadIndex("en");
    expect(mockInvoke).toHaveBeenCalledTimes(1);
    pending.resolve(index("en"));
    expect(await second).toBeUndefined();
    expect((await first)?.sections).toHaveLength(1);
  });

  it("ignores a superseded response (late result never overwrites a newer one)", async () => {
    const slow = deferred<DocsIndex>();
    setInvokeHandlers({ fetch_docs_index: () => slow.promise });
    const first = useDocsStore.getState().loadIndex("en");
    setInvokeHandlers({ fetch_docs_index: () => ({ ...index("en"), source: "remote" }) });
    await useDocsStore.getState().loadIndex("en", { force: true });
    slow.resolve({ ...index("en"), source: "bundled" });
    expect(await first).toBeUndefined();
    expect(useDocsStore.getState().index.en?.data?.source).toBe("remote");
  });

  it("loads pages keyed by language and id", async () => {
    const data = await useDocsStore.getState().loadPage("en", "faq");
    expect(data?.title).toBe("faq en");
    expect(mockInvoke).toHaveBeenCalledWith("fetch_doc_page", { id: "faq", lang: "en" });
    expect(useDocsStore.getState().pages[pageKey("en", "faq")]?.data).toEqual(data);

    setInvokeHandlers({ fetch_doc_page: rejectWith(wireError("http", "404", { status: "404" })) });
    expect(await useDocsStore.getState().loadPage("en", "missing")).toBeUndefined();
    expect(useDocsStore.getState().pages[pageKey("en", "missing")]?.error?.code).toBe("http");
    // the successful page is untouched
    expect(useDocsStore.getState().pages[pageKey("en", "faq")]?.error).toBeNull();
  });

  it("remembers the selection per language and reset clears everything", async () => {
    const { select } = useDocsStore.getState();
    select("en", "faq");
    select("zh-CN", "verify");
    expect(useDocsStore.getState().selected).toEqual({ en: "faq", "zh-CN": "verify" });

    const before = useDocsStore.getState();
    select("en", "faq");
    expect(useDocsStore.getState()).toBe(before);

    await useDocsStore.getState().loadIndex("en");
    useDocsStore.getState().reset();
    expect(useDocsStore.getState().index).toEqual({});
    expect(useDocsStore.getState().pages).toEqual({});
    expect(useDocsStore.getState().selected).toEqual({});
  });
});
