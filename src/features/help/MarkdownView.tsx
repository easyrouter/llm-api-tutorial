/**
 * Renders a help page's Markdown with react-markdown + remark-gfm.
 *
 * Security: no raw HTML (`skipHtml`), no `<a href>` navigation — every link is a `<button>`
 * whose behaviour is decided by `classifyLink` (see `docs-tree.ts` for the conventions):
 * wizard links call `onGoToStep`, section links call `onSelectSection`, http(s) links go
 * through the Rust `open_external` command (validated there), anything else is inert text.
 */
import { ArrowRight, ExternalLink as ExternalLinkIcon } from "lucide-react";
import { useMemo, useState, type ComponentPropsWithoutRef, type JSX, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import Markdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";

import { cn } from "@/lib/cn";
import { openExternal } from "@/lib/tauri";
import type { DocSection, WizardStep } from "@/lib/types";

import { classifyLink, docsUrlTransform } from "./docs-tree";

export interface MarkdownViewProps {
  markdown: string;
  /** Index used to resolve relative section links. */
  sections: readonly DocSection[];
  onSelectSection: (id: string) => void;
  onGoToStep: (step: WizardStep) => void;
  className?: string;
}

const linkClass =
  "text-brand-700 hover:text-brand-600 focus-visible:ring-brand-500 dark:text-brand-100 inline-flex items-center gap-1 rounded-sm underline underline-offset-2 focus-visible:ring-2 focus-visible:outline-none";

interface DocLinkProps {
  href?: string;
  children?: ReactNode;
  sections: readonly DocSection[];
  onSelectSection: (id: string) => void;
  onGoToStep: (step: WizardStep) => void;
}

function DocLink({ href, children, sections, onSelectSection, onGoToStep }: DocLinkProps) {
  const { t } = useTranslation("help");
  const [failed, setFailed] = useState(false);
  const target = classifyLink(href, sections);

  switch (target.kind) {
    case "step":
      return (
        <button
          type="button"
          className={linkClass}
          onClick={() => onGoToStep(target.step)}
          title={t("links.toStep", { step: t(`common:steps.${target.step}`) })}
        >
          {children}
          <ArrowRight className="size-3.5" aria-hidden />
        </button>
      );
    case "section":
      return (
        <button
          type="button"
          className={linkClass}
          onClick={() => onSelectSection(target.id)}
          title={t("links.toTopic")}
        >
          {children}
        </button>
      );
    case "external":
      return (
        <button
          type="button"
          className={cn(linkClass, failed && "text-danger-500")}
          title={failed ? t("common:ui.openLinkFailed", { url: target.url }) : target.url}
          onClick={() => {
            setFailed(false);
            openExternal(target.url).catch(() => setFailed(true));
          }}
        >
          {children}
          <ExternalLinkIcon className="size-3.5" aria-hidden />
          <span className="sr-only">{t("common:ui.opensInBrowser")}</span>
        </button>
      );
    case "unsupported":
      return (
        <span className="underline decoration-dotted" title={t("links.unsupported")}>
          {children}
        </span>
      );
  }
}

type Props<T extends keyof JSX.IntrinsicElements> = ComponentPropsWithoutRef<T>;

/** Static element styling; links are injected per page in `MarkdownView`. */
const baseComponents: Components = {
  h1: ({ children }: Props<"h1">) => (
    <h1 className="mt-2 mb-3 text-lg font-semibold">{children}</h1>
  ),
  h2: ({ children }: Props<"h2">) => (
    <h2 className="mt-5 mb-2 text-base font-semibold">{children}</h2>
  ),
  h3: ({ children }: Props<"h3">) => (
    <h3 className="mt-4 mb-1.5 text-sm font-semibold">{children}</h3>
  ),
  h4: ({ children }: Props<"h4">) => <h4 className="mt-3 mb-1 text-sm font-medium">{children}</h4>,
  p: ({ children }: Props<"p">) => <p className="my-2 leading-6">{children}</p>,
  ul: ({ children }: Props<"ul">) => (
    <ul className="my-2 list-disc space-y-1 pl-5 leading-6">{children}</ul>
  ),
  ol: ({ children }: Props<"ol">) => (
    <ol className="my-2 list-decimal space-y-1 pl-5 leading-6">{children}</ol>
  ),
  li: ({ children }: Props<"li">) => <li>{children}</li>,
  blockquote: ({ children }: Props<"blockquote">) => (
    <blockquote className="my-2 border-l-2 border-neutral-300 pl-3 text-neutral-600 dark:border-neutral-700 dark:text-neutral-300">
      {children}
    </blockquote>
  ),
  pre: ({ children }: Props<"pre">) => (
    <pre
      data-selectable
      className="my-2 overflow-x-auto rounded-md border border-neutral-200 bg-neutral-100 p-3 font-mono text-xs leading-5 dark:border-neutral-800 dark:bg-neutral-950"
    >
      {children}
    </pre>
  ),
  code: ({ children, className }: Props<"code">) => (
    <code
      className={cn(
        "rounded bg-neutral-100 px-1 py-0.5 font-mono text-[0.85em] dark:bg-neutral-800",
        className,
      )}
    >
      {children}
    </code>
  ),
  table: ({ children }: Props<"table">) => (
    <div className="my-2 overflow-x-auto">
      <table className="w-full border-collapse text-xs">{children}</table>
    </div>
  ),
  th: ({ children }: Props<"th">) => (
    <th className="border border-neutral-200 bg-neutral-50 px-2 py-1 text-left font-semibold dark:border-neutral-800 dark:bg-neutral-900">
      {children}
    </th>
  ),
  td: ({ children }: Props<"td">) => (
    <td className="border border-neutral-200 px-2 py-1 align-top dark:border-neutral-800">
      {children}
    </td>
  ),
  hr: () => <hr className="my-4 border-neutral-200 dark:border-neutral-800" />,
  img: ({ src, alt }: Props<"img">) => (
    <img src={src} alt={alt ?? ""} loading="lazy" className="my-2 max-w-full rounded-md" />
  ),
};

/** Markdown page body. Wide content (code, tables) scrolls inside its own container. */
export function MarkdownView({
  markdown,
  sections,
  onSelectSection,
  onGoToStep,
  className,
}: MarkdownViewProps) {
  const components = useMemo<Components>(
    () => ({
      ...baseComponents,
      a: ({ href, children }: Props<"a">) => (
        <DocLink
          href={href}
          sections={sections}
          onSelectSection={onSelectSection}
          onGoToStep={onGoToStep}
        >
          {children}
        </DocLink>
      ),
    }),
    [sections, onSelectSection, onGoToStep],
  );

  return (
    <div
      className={cn("text-sm text-neutral-800 dark:text-neutral-100", className)}
      data-selectable
    >
      <Markdown
        remarkPlugins={[remarkGfm]}
        components={components}
        urlTransform={docsUrlTransform}
        skipHtml
      >
        {markdown}
      </Markdown>
    </div>
  );
}
