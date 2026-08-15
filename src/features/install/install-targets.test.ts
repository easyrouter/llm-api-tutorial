import { describe, expect, it } from "vitest";

import type { AppConfig } from "@/lib/types";
import { checkResult, envSnapshot, installPlan } from "@/test/fixtures/env";

import {
  ccSwitchDownloadPage,
  deriveInstallTargets,
  installKind,
  nodeDownloadPage,
  RECHECK_ID,
  registryToAvoidOnRetry,
} from "./install-targets";

const ALL_TOOLS = ["codex", "claude-code"] as const;

describe("deriveInstallTargets", () => {
  it("returns nothing for a passing snapshot or no snapshot", () => {
    expect(deriveInstallTargets(envSnapshot(), ALL_TOOLS, [])).toEqual([]);
    expect(deriveInstallTargets(null, ALL_TOOLS, [])).toEqual([]);
  });

  it.each([
    ["node.missing", "node"],
    ["node.too_old", "node"],
  ])("node check %s → node", (code, target) => {
    const snap = envSnapshot({ node: checkResult("node", "fail", { code }) });
    expect(deriveInstallTargets(snap, ALL_TOOLS, [])).toEqual([target]);
  });

  it("does not offer Node for not_on_path (that is a PATH refresh, not an install)", () => {
    const snap = envSnapshot({ node: checkResult("node", "warn", { code: "node.not_on_path" }) });
    expect(deriveInstallTargets(snap, ALL_TOOLS, [])).toEqual([]);
  });

  it("npm.missing means Node needs (re)installing", () => {
    const snap = envSnapshot({ npm: checkResult("npm", "fail", { code: "npm.missing" }) });
    expect(deriveInstallTargets(snap, ALL_TOOLS, [])).toEqual(["node"]);
  });

  it("offers tool installs only for selected tools", () => {
    const snap = envSnapshot({
      codex: checkResult("codex", "fail", { code: "tool.missing" }),
      claude_code: checkResult("claude_code", "fail", { code: "tool.missing" }),
    });
    expect(deriveInstallTargets(snap, ALL_TOOLS, [])).toEqual(["codex", "claude-code"]);
    expect(deriveInstallTargets(snap, ["claude-code"], [])).toEqual(["claude-code"]);
    expect(deriveInstallTargets(snap, [], [])).toEqual([]);
  });

  it("offers CC Switch when missing or when only its data dir is left", () => {
    expect(
      deriveInstallTargets(
        envSnapshot({ cc_switch: checkResult("cc_switch", "fail", { code: "cc_switch.missing" }) }),
        ALL_TOOLS,
        [],
      ),
    ).toEqual(["cc-switch"]);
    expect(
      deriveInstallTargets(
        envSnapshot({
          cc_switch: checkResult("cc_switch", "warn", { code: "cc_switch.data_only" }),
        }),
        ALL_TOOLS,
        [],
      ),
    ).toEqual(["cc-switch"]);
  });

  it("honours install fixes attached by Rust on non-passing rows, with the same tool filter", () => {
    const snap = envSnapshot({
      claude_code: checkResult("claude_code", "warn", {
        code: "tool.not_on_path",
        fixes: [{ kind: "install", tool: "claude-code" }],
      }),
      os: checkResult("os", "pass", { fixes: [{ kind: "install", tool: "cc-switch" }] }),
    });
    expect(deriveInstallTargets(snap, ALL_TOOLS, [])).toEqual(["claude-code"]);
    expect(deriveInstallTargets(snap, ["codex"], [])).toEqual([]);
  });

  it("adds explicit requests and keeps the canonical order, without duplicates", () => {
    const snap = envSnapshot({ codex: checkResult("codex", "fail", { code: "tool.missing" }) });
    expect(deriveInstallTargets(snap, ALL_TOOLS, ["cc-switch", "node", "codex"])).toEqual([
      "node",
      "codex",
      "cc-switch",
    ]);
  });
});

describe("installKind / RECHECK_ID", () => {
  it("maps every target", () => {
    expect(installKind("codex")).toBe("npm");
    expect(installKind("claude-code")).toBe("npm");
    expect(installKind("node")).toBe("node");
    expect(installKind("cc-switch")).toBe("cc-switch");
    expect(RECHECK_ID).toEqual({
      node: "node",
      codex: "codex",
      "claude-code": "claude_code",
      "cc-switch": "cc_switch",
    });
  });
});

describe("download pages", () => {
  const config = {
    mirrors: {
      nodeDist: [
        {
          id: "official",
          url: "https://nodejs.org/dist/",
          downloadPage: "https://nodejs.org/en/download",
        },
        {
          id: "npmmirror",
          url: "https://npmmirror.com/mirrors/node/",
          downloadPage: "https://npmmirror.com/mirrors/node/",
        },
      ],
    },
    ccSwitch: { downloadPage: "https://github.com/farion1231/cc-switch/releases/latest" },
  } as unknown as AppConfig;

  it("nodeDownloadPage prefers plan.downloadUrl, then the plan's mirror, then config, then nodejs.org", () => {
    const withUrl = installPlan("node", {
      downloadUrl: "https://mirror.example.com/node/",
      registry: { id: "npmmirror", url: "x", downloadPage: "https://npmmirror.com/mirrors/node/" },
    });
    expect(nodeDownloadPage(withUrl, config)).toBe("https://mirror.example.com/node/");
    const plan = installPlan("node", {
      registry: { id: "npmmirror", url: "x", downloadPage: "https://npmmirror.com/mirrors/node/" },
    });
    expect(nodeDownloadPage(plan, config)).toBe("https://npmmirror.com/mirrors/node/");
    expect(nodeDownloadPage(installPlan("node", { registry: null }), config)).toBe(
      "https://nodejs.org/en/download",
    );
    expect(nodeDownloadPage(null, null)).toBe("https://nodejs.org/en/download");
  });

  it("ccSwitchDownloadPage falls back to the GitHub releases page", () => {
    expect(ccSwitchDownloadPage(config)).toBe(
      "https://github.com/farion1231/cc-switch/releases/latest",
    );
    expect(ccSwitchDownloadPage(null)).toContain("github.com");
  });
});

describe("registryToAvoidOnRetry", () => {
  const twoRegistries = {
    mirrors: {
      npmRegistries: [
        { id: "official", url: "https://registry.npmjs.org/", downloadPage: "" },
        { id: "npmmirror", url: "https://registry.npmmirror.com/", downloadPage: "" },
      ],
    },
  } as unknown as AppConfig;
  const oneRegistry = {
    mirrors: {
      npmRegistries: [
        { id: "npmmirror", url: "https://registry.npmmirror.com/", downloadPage: "" },
      ],
    },
  } as unknown as AppConfig;

  it("names the failed registry only when another one is configured", () => {
    const plan = installPlan("codex"); // registry npmmirror
    expect(registryToAvoidOnRetry(plan, twoRegistries)).toBe("npmmirror");
    expect(registryToAvoidOnRetry(plan, oneRegistry)).toBeNull();
    expect(registryToAvoidOnRetry(plan, null)).toBeNull();
    expect(
      registryToAvoidOnRetry(installPlan("codex", { registry: null }), twoRegistries),
    ).toBeNull();
    expect(registryToAvoidOnRetry(null, twoRegistries)).toBeNull();
  });
});
