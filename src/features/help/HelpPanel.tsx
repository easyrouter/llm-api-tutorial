/**
 * M6 — help panel, rendered in the right drawer by `AppShell` while `wizard.helpOpen`.
 *
 * Data: `stores/docs.ts` (index + pages per language, last selection per language). The
 * language follows `i18n.language` (`zh-CN` | `en`); switching languages re-fetches the index
 * and carries the current section over (ids are language-independent by contract).
 *
 * Which section opens (first match wins):
 *   1. `wizard.helpSectionId` — set by `openHelp(id)` / `<HelpLink sectionId>`;
 *   2. the section whose `wizardStep` equals the current wizard step;
 *   3. the last section selected in this language;
 *   4. `overview`, else the first section of the index.
 *
 * Links inside pages: `wizard://<step>` and `#step:<step>` jump the wizard (both spellings are
 * supported, see `docs-tree.ts`), http(s) opens the browser, relative ids/paths open another
 * topic. Sections that carry a `wizardStep` also get a "Go to this step" button.
 */
import { ChevronDown, ChevronUp, Search, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button, ErrorBanner, Spinner } from "@/components/ui";
import type { Lang } from "@/i18n";
import { cn } from "@/lib/cn";
import type { DocSection, DocsSource, WizardStep } from "@/lib/types";
import { pageKey, slotOf, useDocsStore } from "@/stores/docs";
import { useWizardStore } from "@/stores/wizard";

import {
  docsLang,
  filterSections,
  findSection,
  resolveSelection,
  sectionForStep,
  stepReachable,
  stripLeadingTitle,
} from "./docs-tree";
import { MarkdownView } from "./MarkdownView";
import { SectionTree } from "./SectionTree";

const NO_SECTIONS: readonly DocSection[] = [];

/** Small pill telling where the shown content came from (remote / cache / bundled). */
function SourceChip({ source }: { source: DocsSource }) {
  const { t } = useTranslation("help");
  return (
    <span
      data-source={source}
      title={t("source.label")}
      className="inline-flex shrink-0 items-center rounded-full border border-neutral-300 px-2 py-0.5 text-[11px] whitespace-nowrap text-neutral-600 dark:border-neutral-700 dark:text-neutral-300"
    >
      {t(`source.${source}`)}
    </span>
  );
}

export function HelpPanel() {
  const { t, i18n } = useTranslation("help");
  const lang = docsLang(i18n.language);

  const closeHelp = useWizardStore((s) => s.closeHelp);
  const helpSectionId = useWizardStore((s) => s.helpSectionId);
  const helpRequestId = useWizardStore((s) => s.helpRequestId);
  const furthestStep = useWizardStore((s) => s.furthestStep);
  const goTo = useWizardStore((s) => s.goTo);

  const indexSlot = slotOf(useDocsStore((s) => s.index[lang]));
  const selectedId = useDocsStore((s) => s.selected[lang]) ?? null;
  const pageSlot = slotOf(
    useDocsStore((s) => (selectedId ? s.pages[pageKey(lang, selectedId)] : undefined)),
  );
  const pages = useDocsStore((s) => s.pages);
  const loadIndex = useDocsStore((s) => s.loadIndex);
  const loadPage = useDocsStore((s) => s.loadPage);
  const select = useDocsStore((s) => s.select);

  const [query, setQuery] = useState("");
  const [treeOpen, setTreeOpen] = useState(true);

  const sections = indexSlot.data?.sections ?? NO_SECTIONS;

  // 1. index for the current language (cached per session; Retry forces).
  useEffect(() => {
    void loadIndex(lang);
  }, [lang, loadIndex]);

  // 2. selection: resolved when the index arrives, when the language changes and when a new
  //    section is requested via `openHelp(id)` (see the module doc for the priority). The
  //    request counter makes a repeated `openHelp` for the same id count as a new request.
  const resolvedRef = useRef<{ lang: Lang | null; request: number | null }>({
    lang: null,
    request: null,
  });
  useEffect(() => {
    if (!indexSlot.data) return;
    const prev = resolvedRef.current;
    const requestChanged = helpRequestId !== prev.request;
    const langChanged = lang !== prev.lang;
    if (!requestChanged && !langChanged) return;

    const { selected } = useDocsStore.getState();
    const stepMatch = sectionForStep(sections, useWizardStore.getState().step)?.id;
    const candidates: Array<string | null | undefined> = [];
    if (requestChanged) candidates.push(helpSectionId);
    if (prev.lang === null) {
      candidates.push(stepMatch, selected[lang]);
    } else {
      // Language switch: keep reading the same topic, then this language's memory.
      if (langChanged) candidates.push(selected[prev.lang]);
      candidates.push(selected[lang], stepMatch);
    }
    resolvedRef.current = { lang, request: helpRequestId };
    const id = resolveSelection(sections, candidates);
    if (id) select(lang, id);
  }, [indexSlot.data, sections, lang, helpSectionId, helpRequestId, select]);

  // 3. page for the selection.
  useEffect(() => {
    if (selectedId) void loadPage(lang, selectedId);
  }, [lang, selectedId, loadPage]);

  const onSelect = useCallback((id: string) => select(lang, id), [lang, select]);
  const onGoToStep = useCallback((step: WizardStep) => goTo(step), [goTo]);

  const pageText = useCallback(
    (id: string) => pages[pageKey(lang, id)]?.data?.markdown,
    [pages, lang],
  );
  const visibleSections = useMemo(
    () => filterSections(sections, query, pageText),
    [sections, query, pageText],
  );
  const searching = query.trim().length > 0;
  const showTree = treeOpen || searching;

  const section = findSection(sections, selectedId);
  const page = pageSlot.data;
  const stepOfSection = section?.wizardStep ?? null;
  const canGo = stepOfSection ? stepReachable(stepOfSection, furthestStep) : false;

  return (
    <div className="flex h-full flex-col" data-testid="help-panel">
      <header className="flex items-center justify-between gap-2 border-b border-neutral-200 px-4 py-3 dark:border-neutral-800">
        <h2 className="text-sm font-semibold">{t("common:help.panelTitle")}</h2>
        <div className="flex items-center gap-2">
          {indexSlot.data && <SourceChip source={page?.source ?? indexSlot.data.source} />}
          <button
            type="button"
            onClick={closeHelp}
            className="rounded p-1 text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-800"
            aria-label={t("common:help.closePanel")}
          >
            <X className="size-4" aria-hidden />
          </button>
        </div>
      </header>

      <div className="border-b border-neutral-200 px-3 py-2 dark:border-neutral-800">
        <label className="relative block">
          <span className="sr-only">{t("search.label")}</span>
          <Search
            className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-neutral-400"
            aria-hidden
          />
          <input
            type="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("search.placeholder")}
            className="focus-visible:ring-brand-500 h-8 w-full rounded-md border border-neutral-300 bg-white pr-2 pl-8 text-sm text-neutral-900 placeholder:text-neutral-400 focus-visible:ring-2 focus-visible:outline-none dark:border-neutral-700 dark:bg-neutral-950 dark:text-neutral-100"
          />
        </label>
        <button
          type="button"
          onClick={() => setTreeOpen((v) => !v)}
          disabled={searching}
          aria-expanded={showTree}
          className="mt-2 inline-flex items-center gap-1 text-xs text-neutral-500 hover:text-neutral-700 disabled:cursor-default dark:hover:text-neutral-200"
        >
          {showTree ? (
            <ChevronUp className="size-3.5" aria-hidden />
          ) : (
            <ChevronDown className="size-3.5" aria-hidden />
          )}
          {showTree ? t("tree.hide") : t("tree.show")}
        </button>
        {showTree && indexSlot.data && (
          <div className="mt-1 max-h-56 overflow-y-auto">
            {visibleSections.length > 0 ? (
              <SectionTree sections={visibleSections} selectedId={selectedId} onSelect={onSelect} />
            ) : (
              <p className="px-2 py-1 text-xs text-neutral-500">
                {searching ? t("search.noResults", { query: query.trim() }) : t("state.empty")}
              </p>
            )}
          </div>
        )}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
        {indexSlot.error && !indexSlot.data && (
          <ErrorBanner
            error={indexSlot.error}
            title={t("state.loadFailed")}
            onRetry={() => void loadIndex(lang, { force: true })}
          />
        )}
        {indexSlot.loading && !indexSlot.data && (
          <div className="flex items-center gap-2 text-sm text-neutral-500">
            <Spinner size="sm" label={t("state.loading")} />
            {t("state.loading")}
          </div>
        )}
        {indexSlot.data && sections.length === 0 && (
          <p className="text-sm text-neutral-500">{t("state.empty")}</p>
        )}

        {section && (
          <article aria-labelledby="help-page-title">
            <div className="flex items-start justify-between gap-3">
              <h3 id="help-page-title" className="text-base font-semibold">
                {page?.title ?? section.title}
              </h3>
            </div>
            {stepOfSection && (
              <Button
                size="sm"
                variant="secondary"
                className="mt-2"
                onClick={() => onGoToStep(stepOfSection)}
                disabled={!canGo}
                title={canGo ? undefined : t("actions.goToStepLocked")}
              >
                {t("actions.goToStep", { step: t(`common:steps.${stepOfSection}`) })}
              </Button>
            )}

            {pageSlot.error && (
              <ErrorBanner
                error={pageSlot.error}
                className="mt-3"
                onRetry={() => void loadPage(lang, section.id, { force: true })}
              />
            )}
            {pageSlot.loading && !page && (
              <div className="mt-3 flex items-center gap-2 text-sm text-neutral-500">
                <Spinner size="sm" label={t("state.loadingPage")} />
                {t("state.loadingPage")}
              </div>
            )}
            {page && (
              <MarkdownView
                className={cn("mt-2", pageSlot.loading && "opacity-70")}
                markdown={stripLeadingTitle(page.markdown, page.title)}
                sections={sections}
                onSelectSection={onSelect}
                onGoToStep={onGoToStep}
              />
            )}
          </article>
        )}
      </div>
    </div>
  );
}
