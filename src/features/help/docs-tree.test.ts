import { describe, expect, it } from "vitest";

import type { DocSection } from "@/lib/types";

import {
  classifyLink,
  docsLang,
  docsUrlTransform,
  filterSections,
  findSection,
  flattenSections,
  isWizardStep,
  matchesQuery,
  parseWizardLink,
  resolveSelection,
  sectionForStep,
  stepReachable,
  stripLeadingTitle,
} from "./docs-tree";

const section = (id: string, extra: Partial<Omit<DocSection, "id">> = {}): DocSection => ({
  id,
  title: extra.title ?? id,
  path: extra.path ?? `${id}.md`,
  lang: "en",
  wizardStep: extra.wizardStep ?? null,
  children: extra.children ?? [],
});

const tree: DocSection[] = [
  section("overview", { title: "Welcome & overview", wizardStep: "welcome" }),
  section("install", {
    title: "Install",
    wizardStep: "install",
    children: [
      section("install-node", { title: "Install Node.js", wizardStep: "install" }),
      section("install-cli", { title: "Install Codex CLI", path: "guides/cli.md" }),
    ],
  }),
  section("verify", { title: "Apply & verify", wizardStep: "verify" }),
  section("faq", { title: "FAQ" }),
];

describe("flattenSections / findSection / sectionForStep", () => {
  it("flattens depth-first, parents first", () => {
    expect(flattenSections(tree).map((s) => s.id)).toEqual([
      "overview",
      "install",
      "install-node",
      "install-cli",
      "verify",
      "faq",
    ]);
  });

  it("finds nested sections and tolerates empty ids", () => {
    expect(findSection(tree, "install-cli")?.title).toBe("Install Codex CLI");
    expect(findSection(tree, "nope")).toBeUndefined();
    expect(findSection(tree, null)).toBeUndefined();
    expect(findSection(tree, "")).toBeUndefined();
  });

  it("returns the first section (document order) for a step", () => {
    expect(sectionForStep(tree, "install")?.id).toBe("install");
    expect(sectionForStep(tree, "verify")?.id).toBe("verify");
    expect(sectionForStep(tree, "done")).toBeUndefined();
  });
});

describe("resolveSelection", () => {
  it.each<[ReadonlyArray<string | null | undefined>, string | null]>([
    [["verify"], "verify"],
    [[null, undefined, "missing", "faq"], "faq"],
    [["missing"], "overview"],
    [[], "overview"],
  ])("candidates %j → %s", (candidates, expected) => {
    expect(resolveSelection(tree, candidates)).toBe(expected);
  });

  it("falls back to the first section when there is no overview, null when empty", () => {
    expect(resolveSelection([section("a"), section("b")], ["zzz"])).toBe("a");
    expect(resolveSelection([], ["a"])).toBeNull();
  });
});

describe("matchesQuery / filterSections", () => {
  it("matches case-insensitively and treats blank queries as match-all", () => {
    expect(matchesQuery("Apply & verify", "VERIFY")).toBe(true);
    expect(matchesQuery("Apply & verify", "   ")).toBe(true);
    expect(matchesQuery("Apply & verify", "node")).toBe(false);
  });

  it("returns the tree unchanged for an empty query", () => {
    expect(filterSections(tree, "")).toEqual(tree);
  });

  it("keeps matching titles and the ancestors of matching children", () => {
    const filtered = filterSections(tree, "node");
    expect(filtered.map((s) => s.id)).toEqual(["install"]);
    expect(filtered[0]?.children.map((s) => s.id)).toEqual(["install-node"]);
  });

  it("keeps a parent whose own title matches together with all its children", () => {
    const filtered = filterSections(tree, "install");
    expect(filtered[0]?.children.map((s) => s.id)).toEqual(["install-node", "install-cli"]);
  });

  it("also searches loaded page text", () => {
    const pageText = (id: string) => (id === "faq" ? "Where do I get an API key?" : undefined);
    expect(filterSections(tree, "api key", pageText).map((s) => s.id)).toEqual(["faq"]);
    expect(filterSections(tree, "api key")).toEqual([]);
  });
});

describe("wizard links", () => {
  it.each<[string, string | null]>([
    ["wizard://verify", "verify"],
    ["wizard://env_check/", "env_check"],
    ["#step:install", "install"],
    [" #step:diagnose ", "diagnose"],
    ["wizard://nope", null],
    ["#step:", null],
    ["#verify", null],
    ["https://example.com/wizard://verify", null],
  ])("parseWizardLink(%j) → %j", (href, expected) => {
    expect(parseWizardLink(href)).toBe(expected);
  });

  it("isWizardStep accepts the shared vocabulary only", () => {
    expect(isWizardStep("configure")).toBe(true);
    expect(isWizardStep("Configure")).toBe(false);
  });

  it("docsUrlTransform keeps wizard links and sanitises unknown schemes", () => {
    expect(docsUrlTransform("wizard://verify")).toBe("wizard://verify");
    expect(docsUrlTransform("#step:verify")).toBe("#step:verify");
    expect(docsUrlTransform("https://example.com/x")).toBe("https://example.com/x");
    expect(docsUrlTransform("javascript:alert(1)")).toBe("");
    expect(docsUrlTransform("custom://thing")).toBe("");
  });
});

describe("classifyLink", () => {
  it.each<[string | undefined, ReturnType<typeof classifyLink>]>([
    ["wizard://install", { kind: "step", step: "install" }],
    ["#step:verify", { kind: "step", step: "verify" }],
    ["https://nodejs.org/", { kind: "external", url: "https://nodejs.org/" }],
    ["HTTP://example.com", { kind: "external", url: "HTTP://example.com" }],
    ["faq", { kind: "section", id: "faq" }],
    ["./verify.md#by-hand", { kind: "section", id: "verify" }],
    ["guides/cli.md", { kind: "section", id: "install-cli" }],
    ["mailto:it@example.com", { kind: "unsupported" }],
    ["unknown.md", { kind: "unsupported" }],
    ["#anchor", { kind: "unsupported" }],
    ["", { kind: "unsupported" }],
    [undefined, { kind: "unsupported" }],
  ])("%j", (href, expected) => {
    expect(classifyLink(href, tree)).toEqual(expected);
  });
});

describe("stepReachable", () => {
  it.each<[Parameters<typeof stepReachable>[0], Parameters<typeof stepReachable>[1], boolean]>([
    ["welcome", "welcome", true],
    ["install", "env_check", false],
    ["install", "install", true],
    ["env_check", "verify", true],
    ["diagnose", "configure", false],
    ["diagnose", "verify", true],
    ["done", "verify", false],
  ])("%s reachable from furthest %s → %s", (step, furthest, expected) => {
    expect(stepReachable(step, furthest)).toBe(expected);
  });
});

describe("docsLang / stripLeadingTitle", () => {
  it("maps UI languages onto the two docs languages", () => {
    expect(docsLang("zh-CN")).toBe("zh-CN");
    expect(docsLang("zh-TW")).toBe("zh-CN");
    expect(docsLang("en-US")).toBe("en");
    expect(docsLang("fr")).toBe("zh-CN");
  });

  it("drops the leading H1 only when it duplicates the title", () => {
    expect(stripLeadingTitle("# FAQ\n\nBody", "FAQ")).toBe("\nBody");
    expect(stripLeadingTitle("# FAQ", "FAQ")).toBe("");
    expect(stripLeadingTitle("# Other\n\nBody", "FAQ")).toBe("# Other\n\nBody");
    expect(stripLeadingTitle("Intro\n# FAQ", "FAQ")).toBe("Intro\n# FAQ");
  });
});
