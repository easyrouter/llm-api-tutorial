/**
 * Pure helpers for the help panel: section-tree lookups, search filtering, selection
 * resolution and the Markdown link conventions. No React, no stores — everything here is
 * unit-tested in `docs-tree.test.ts`.
 *
 * # Link conventions inside help pages
 *
 * | href in Markdown                    | behaviour                                              |
 * | ----------------------------------- | ------------------------------------------------------ |
 * | `wizard://<step>` or `#step:<step>` | jumps the wizard to `<step>` (`WizardStep` value)      |
 * | `http(s)://…`                       | opened in the system browser (`open_external`)         |
 * | `<section-id>` / `<path>.md`        | opens that help section inside the panel               |
 * | anything else                       | rendered as plain text (no navigation)                 |
 *
 * `#step:` is the form that survives generic Markdown tooling (a fragment link); `wizard://`
 * reads better in hand-written docs. Both are supported; use whichever the docs site prefers.
 */
import { defaultUrlTransform } from "react-markdown";

import { DEFAULT_LANG, normalizeLang, type Lang } from "@/i18n";
import type { DocSection, WizardStep } from "@/lib/types";
import { stepIndex } from "@/stores/wizard";

/** Section shown when nothing else applies. */
export const DEFAULT_SECTION_ID = "overview";

/** Maps the UI language to a docs language (`zh*` → zh-CN, otherwise en). */
export function docsLang(uiLanguage: string): Lang {
  return normalizeLang(uiLanguage) ?? DEFAULT_LANG;
}

/**
 * Pages start with a `# Title` line that duplicates the section title shown in the panel
 * header; drops it when it is the very first line and matches the title.
 */
export function stripLeadingTitle(markdown: string, title: string): string {
  const firstBreak = markdown.indexOf("\n");
  const firstLine = (firstBreak === -1 ? markdown : markdown.slice(0, firstBreak)).trim();
  if (firstLine.startsWith("# ") && firstLine.slice(2).trim() === title.trim()) {
    return firstBreak === -1 ? "" : markdown.slice(firstBreak + 1);
  }
  return markdown;
}

const ALL_STEPS: readonly WizardStep[] = [
  "welcome",
  "env_check",
  "install",
  "configure",
  "verify",
  "diagnose",
  "done",
];

/** Type guard for the wizard-step vocabulary shared with Rust. */
export function isWizardStep(value: string): value is WizardStep {
  return (ALL_STEPS as readonly string[]).includes(value);
}

/** Depth-first flattening (parents before children). */
export function flattenSections(sections: readonly DocSection[]): DocSection[] {
  const out: DocSection[] = [];
  const walk = (list: readonly DocSection[]) => {
    for (const s of list) {
      out.push(s);
      walk(s.children);
    }
  };
  walk(sections);
  return out;
}

/** Depth-first lookup by id. */
export function findSection(
  sections: readonly DocSection[],
  id: string | null | undefined,
): DocSection | undefined {
  if (!id) return undefined;
  return flattenSections(sections).find((s) => s.id === id);
}

/** First section (document order) whose `wizardStep` equals `step`. */
export function sectionForStep(
  sections: readonly DocSection[],
  step: WizardStep,
): DocSection | undefined {
  return flattenSections(sections).find((s) => s.wizardStep === step);
}

/**
 * Picks the section to show: the first `candidate` that exists in the index, else
 * `overview`, else the first section. `null` only for an empty index.
 */
export function resolveSelection(
  sections: readonly DocSection[],
  candidates: ReadonlyArray<string | null | undefined>,
): string | null {
  for (const id of candidates) {
    if (findSection(sections, id)) return id ?? null;
  }
  if (findSection(sections, DEFAULT_SECTION_ID)) return DEFAULT_SECTION_ID;
  return flattenSections(sections)[0]?.id ?? null;
}

/** Case-insensitive, whitespace-trimmed "contains" test. */
export function matchesQuery(text: string, query: string): boolean {
  const q = query.trim().toLocaleLowerCase();
  if (!q) return true;
  return text.toLocaleLowerCase().includes(q);
}

/**
 * Filters the tree by `query`: a section stays when its title matches, when its page text
 * (if `pageText(id)` returns one) matches, or when any descendant stays. An empty query
 * returns the tree unchanged.
 */
export function filterSections(
  sections: readonly DocSection[],
  query: string,
  pageText: (id: string) => string | undefined = () => undefined,
): DocSection[] {
  if (!query.trim()) return [...sections];
  const keep = (s: DocSection): DocSection | null => {
    const children = s.children.map(keep).filter((c): c is DocSection => c !== null);
    const own = matchesQuery(s.title, query) || matchesQuery(pageText(s.id) ?? "", query);
    if (!own && children.length === 0) return null;
    return { ...s, children };
  };
  return sections.map(keep).filter((s): s is DocSection => s !== null);
}

/** Where a Markdown link inside a help page should lead. */
export type LinkTarget =
  | { kind: "step"; step: WizardStep }
  | { kind: "external"; url: string }
  | { kind: "section"; id: string }
  | { kind: "unsupported" };

const WIZARD_SCHEME = "wizard://";
const STEP_FRAGMENT = "#step:";

/** Extracts the wizard step from `wizard://<step>` / `#step:<step>`; `null` when not one. */
export function parseWizardLink(href: string): WizardStep | null {
  const value = href.trim();
  let step: string | null = null;
  if (value.startsWith(WIZARD_SCHEME)) step = value.slice(WIZARD_SCHEME.length);
  else if (value.startsWith(STEP_FRAGMENT)) step = value.slice(STEP_FRAGMENT.length);
  if (step === null) return null;
  step = step.replace(/\/+$/, "");
  return isWizardStep(step) ? step : null;
}

function isHttpUrl(value: string): boolean {
  return /^https?:\/\//i.test(value);
}

/** Normalises `./verify.md#x` → `verify.md` and `faq` → `faq` for section matching. */
function relativeDocName(href: string): string {
  return href.replace(/^\.\//, "").split("#")[0]?.split("?")[0] ?? "";
}

/**
 * Classifies a link `href` found in a help page (see the module doc for the conventions).
 * Relative links are matched against section ids and paths so pages may cross-link with the
 * same relative paths the docs site itself uses.
 */
export function classifyLink(
  href: string | undefined,
  sections: readonly DocSection[],
): LinkTarget {
  if (!href) return { kind: "unsupported" };
  const step = parseWizardLink(href);
  if (step) return { kind: "step", step };
  const value = href.trim();
  if (isHttpUrl(value)) return { kind: "external", url: value };
  const name = relativeDocName(value);
  if (name) {
    const hit = flattenSections(sections).find((s) => s.id === name || s.path === name);
    if (hit) return { kind: "section", id: hit.id };
  }
  return { kind: "unsupported" };
}

/**
 * react-markdown URL transform: keeps `wizard://` links (the default transform blanks unknown
 * schemes) and sanitises everything else with the library default.
 */
export function docsUrlTransform(url: string): string {
  return parseWizardLink(url) ? url : defaultUrlTransform(url);
}

/**
 * True when the wizard may jump to `step` from where the user currently is: never beyond the
 * furthest step reached (the wizard is linear). `diagnose` is a panel inside Verify.
 */
export function stepReachable(step: WizardStep, furthest: WizardStep): boolean {
  const target = stepIndex(step);
  return target >= 0 && target <= stepIndex(furthest);
}
