import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";

import i18n from "@/i18n";
import type { DocPage, DocSection, DocsIndex } from "@/lib/types";
import { useDocsStore } from "@/stores/docs";
import { useWizardStore } from "@/stores/wizard";
import { mockInvoke, rejectWith, setInvokeHandlers, wireError } from "@/test/mocks/tauri";

import { HelpLink } from "./HelpLink";
import { HelpPanel } from "./HelpPanel";

const titles: Record<string, Record<string, string>> = {
  en: {
    overview: "Welcome & overview",
    "env-check": "Environment check",
    "install-node": "Install Node.js",
    verify: "Apply & verify",
    faq: "FAQ",
  },
  "zh-CN": {
    overview: "欢迎与总览",
    "env-check": "环境检查",
    "install-node": "安装 Node.js",
    verify: "应用与验证",
    faq: "常见问题",
  },
};

const sectionsFor = (lang: string): DocSection[] => {
  const t = titles[lang] ?? titles.en ?? {};
  const s = (id: string, wizardStep: DocSection["wizardStep"], children: DocSection[] = []) => ({
    id,
    title: t[id] ?? id,
    path: `${id}.md`,
    lang,
    wizardStep,
    children,
  });
  return [
    s("overview", "welcome"),
    s("env-check", "env_check"),
    s("install-node", "install"),
    s("verify", "verify"),
    s("faq", null),
  ];
};

const bodies: Record<string, string> = {
  overview: "# Welcome & overview\n\nWhat this tool does.",
  "env-check": "# Environment check\n\nChecks your machine.",
  "install-node": "# Install Node.js\n\nUse the LTS release.",
  verify:
    "# Apply & verify\n\nClose every terminal first.\n\n" +
    "- [Go to install](wizard://install)\n" +
    "- [Go to env check](#step:env_check)\n" +
    "- [Node.js site](https://nodejs.org/en/download)\n" +
    "- [See the FAQ](faq.md)\n" +
    "- [Odd link](mailto:it@example.com)\n\n" +
    "```\ncodex --version\n```\n\n<script>alert(1)</script>",
  faq: "# FAQ\n\n**Where do I get an API key?** Ask IT.",
};

const indexFor = (lang: string, source: DocsIndex["source"] = "remote"): DocsIndex => ({
  sections: sectionsFor(lang),
  fetchedAt: "2026-01-01T00:00:00Z",
  source,
});

const pageFor = (id: string, lang: string): DocPage => ({
  id,
  title: titles[lang]?.[id] ?? id,
  lang,
  markdown: bodies[id] ?? `# ${id}\n\nbody`,
  source: "cache",
  fetchedAt: "2026-01-01T00:00:00Z",
});

function installDefaultHandlers() {
  setInvokeHandlers({
    fetch_docs_index: (args) => indexFor(String(args?.lang)),
    fetch_doc_page: (args) => pageFor(String(args?.id), String(args?.lang)),
  });
}

const pageHeading = (name: string) => screen.findByRole("heading", { level: 3, name });
const topics = () => screen.getByRole("navigation", { name: "Help topics" });

describe("HelpPanel", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });
  beforeEach(() => {
    useWizardStore.getState().reset();
    useDocsStore.getState().reset();
    installDefaultHandlers();
  });
  afterEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("loads the index and opens the section matching the current wizard step", async () => {
    useWizardStore.setState({ step: "verify", furthestStep: "verify", helpOpen: true });
    render(<HelpPanel />);

    expect(await pageHeading("Apply & verify")).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledWith("fetch_docs_index", { lang: "en" });
    expect(mockInvoke).toHaveBeenCalledWith("fetch_doc_page", { id: "verify", lang: "en" });

    // Markdown rendered: body text, code block, no raw HTML, duplicate H1 dropped
    expect(screen.getByText("Close every terminal first.")).toBeInTheDocument();
    expect(screen.getByText("codex --version")).toBeInTheDocument();
    expect(screen.queryByText(/alert\(1\)/)).toBeNull();
    expect(screen.queryByRole("heading", { level: 1 })).toBeNull();

    // tree with the selected topic marked, source chip from the page (cache)
    expect(within(topics()).getByRole("button", { name: "Apply & verify" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.getByText("Cached copy")).toBeInTheDocument();
    expect(useDocsStore.getState().selected.en).toBe("verify");
  });

  it("prefers the explicitly requested section (openHelp(id)) over the step match", async () => {
    useWizardStore.setState({ step: "verify", furthestStep: "verify" });
    useWizardStore.getState().openHelp("faq");
    render(<HelpPanel />);
    expect(await pageHeading("FAQ")).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledWith("fetch_doc_page", { id: "faq", lang: "en" });
  });

  it("falls back to overview when nothing matches, and remembers the last topic per language", async () => {
    useWizardStore.setState({ step: "done", furthestStep: "done", helpOpen: true });
    const first = render(<HelpPanel />);
    expect(await pageHeading("Welcome & overview")).toBeInTheDocument();

    fireEvent.click(within(topics()).getByRole("button", { name: "Install Node.js" }));
    expect(await pageHeading("Install Node.js")).toBeInTheDocument();
    first.unmount();

    render(<HelpPanel />);
    expect(await pageHeading("Install Node.js")).toBeInTheDocument();
  });

  it("selects a newly requested section while already open", async () => {
    useWizardStore.setState({ step: "verify", furthestStep: "verify", helpOpen: true });
    render(
      <>
        <HelpLink sectionId="env-check">Learn</HelpLink>
        <HelpPanel />
      </>,
    );
    expect(await pageHeading("Apply & verify")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Learn" }));
    expect(await pageHeading("Environment check")).toBeInTheDocument();
    expect(useWizardStore.getState().helpSectionId).toBe("env-check");
  });

  it("re-requesting the section that was already requested selects it again", async () => {
    useWizardStore.setState({ step: "verify", furthestStep: "verify", helpOpen: true });
    useWizardStore.getState().openHelp("env-check");
    render(
      <>
        <HelpLink sectionId="env-check">Learn</HelpLink>
        <HelpPanel />
      </>,
    );
    expect(await pageHeading("Environment check")).toBeInTheDocument();

    fireEvent.click(within(topics()).getByRole("button", { name: "FAQ" }));
    expect(await pageHeading("FAQ")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Learn" }));
    expect(await pageHeading("Environment check")).toBeInTheDocument();
    expect(useDocsStore.getState().selected.en).toBe("env-check");
  });

  it("filters the topic tree by title and loaded page text", async () => {
    useWizardStore.setState({ step: "welcome", helpOpen: true });
    render(<HelpPanel />);
    await pageHeading("Welcome & overview");

    const box = screen.getByRole("searchbox", { name: "Search help topics" });
    fireEvent.change(box, { target: { value: "node" } });
    expect(
      within(topics())
        .getAllByRole("button")
        .map((b) => b.textContent),
    ).toEqual(["Install Node.js"]);

    // page text of the loaded overview page is searchable too
    fireEvent.change(box, { target: { value: "what this tool" } });
    expect(
      within(topics())
        .getAllByRole("button")
        .map((b) => b.textContent),
    ).toEqual(["Welcome & overview"]);

    fireEvent.change(box, { target: { value: "zzz" } });
    expect(screen.getByText("No topics match “zzz”")).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "Help topics" })).toBeNull();
  });

  it("wires markdown links: wizard step, external URL, section, unsupported", async () => {
    useWizardStore.setState({ step: "verify", furthestStep: "verify", helpOpen: true });
    render(<HelpPanel />);
    await pageHeading("Apply & verify");

    fireEvent.click(screen.getByRole("button", { name: /^Node\.js site/ }));
    expect(mockInvoke).toHaveBeenCalledWith("open_external", {
      url: "https://nodejs.org/en/download",
    });

    // unsupported schemes render as plain text
    expect(screen.queryByRole("button", { name: "Odd link" })).toBeNull();
    expect(screen.getByText("Odd link")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Go to install" }));
    expect(useWizardStore.getState().step).toBe("install");

    fireEvent.click(screen.getByRole("button", { name: "Go to env check" }));
    expect(useWizardStore.getState().step).toBe("env_check");

    fireEvent.click(screen.getByRole("button", { name: "See the FAQ" }));
    expect(await pageHeading("FAQ")).toBeInTheDocument();
  });

  it("offers 'Go to this step' for sections with a wizardStep, locked beyond the furthest step", async () => {
    useWizardStore.setState({ step: "env_check", furthestStep: "env_check", helpOpen: true });
    useWizardStore.getState().openHelp("verify");
    render(<HelpPanel />);
    await pageHeading("Apply & verify");

    const go = screen.getByRole("button", { name: "Go to this step: Verify" });
    expect(go).toBeDisabled();

    fireEvent.click(within(topics()).getByRole("button", { name: "Welcome & overview" }));
    await pageHeading("Welcome & overview");
    const goStart = screen.getByRole("button", { name: "Go to this step: Start" });
    expect(goStart).toBeEnabled();
    fireEvent.click(goStart);
    expect(useWizardStore.getState().step).toBe("welcome");

    fireEvent.click(within(topics()).getByRole("button", { name: "FAQ" }));
    await pageHeading("FAQ");
    expect(screen.queryByRole("button", { name: /Go to this step/ })).toBeNull();
  });

  it("shows an error banner with retry when the index cannot be loaded", async () => {
    setInvokeHandlers({ fetch_docs_index: rejectWith(wireError("network", "offline")) });
    useWizardStore.setState({ helpOpen: true });
    render(<HelpPanel />);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Help could not be loaded");
    expect(alert).toHaveTextContent("Network request failed.");
    expect(screen.queryByRole("navigation", { name: "Help topics" })).toBeNull();

    setInvokeHandlers({ fetch_docs_index: (args) => indexFor(String(args?.lang), "bundled") });
    fireEvent.click(within(alert).getByRole("button", { name: "Retry" }));
    expect(await pageHeading("Welcome & overview")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("shows a page error with retry while keeping the tree usable", async () => {
    setInvokeHandlers({
      fetch_doc_page: (args) =>
        args?.id === "faq"
          ? rejectWith(wireError("http", "404", { status: "404" }))(args)
          : pageFor(String(args?.id), String(args?.lang)),
    });
    useWizardStore.getState().openHelp("faq");
    render(<HelpPanel />);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Server returned HTTP 404.");
    installDefaultHandlers();
    fireEvent.click(within(alert).getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(screen.getByText("Ask IT.")).toBeInTheDocument());
  });

  it("re-fetches the index on language change and carries the current topic over", async () => {
    useWizardStore.setState({ step: "verify", furthestStep: "verify", helpOpen: true });
    render(<HelpPanel />);
    await pageHeading("Apply & verify");
    fireEvent.click(within(topics()).getByRole("button", { name: "FAQ" }));
    await pageHeading("FAQ");

    await act(async () => {
      await i18n.changeLanguage("zh-CN");
    });
    expect(await pageHeading("常见问题")).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledWith("fetch_docs_index", { lang: "zh-CN" });
    expect(mockInvoke).toHaveBeenCalledWith("fetch_doc_page", { id: "faq", lang: "zh-CN" });
    expect(useDocsStore.getState().selected).toEqual({ en: "faq", "zh-CN": "faq" });
    expect(screen.getByRole("heading", { level: 2, name: "帮助" })).toBeInTheDocument();
  });

  it("closes via the header button", async () => {
    useWizardStore.setState({ helpOpen: true });
    render(<HelpPanel />);
    await pageHeading("Welcome & overview");
    fireEvent.click(screen.getByRole("button", { name: "Close help" }));
    expect(useWizardStore.getState().helpOpen).toBe(false);
  });
});

describe("HelpLink", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });
  beforeEach(() => {
    useWizardStore.getState().reset();
  });

  it("opens the help drawer on the given section with a default label", () => {
    render(<HelpLink sectionId="install-node" />);
    fireEvent.click(screen.getByRole("button", { name: "Learn more" }));
    expect(useWizardStore.getState().helpOpen).toBe(true);
    expect(useWizardStore.getState().helpSectionId).toBe("install-node");
  });
});
