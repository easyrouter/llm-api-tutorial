import { describe, expect, it } from "vitest";

import type { AppConfig, ToolId, VerifyResult } from "@/lib/types";

import { allVerified, commandForTool, outcomeOf, summarizeTools } from "./done-summary";

const verifyResult = (tool: ToolId, ok: boolean, version: string | null): VerifyResult => ({
  tool,
  cli: { ok, version, path: null, outputTail: null, errorClass: ok ? null : "command_not_found" },
  gateway: null,
  runningTerminals: [],
  ok,
  diagnoses: [],
});

const configWith = (tools: AppConfig["tools"]): AppConfig => ({ tools }) as unknown as AppConfig;

describe("commandForTool", () => {
  it("uses the binary from the preset and falls back to the built-in default", () => {
    const config = configWith([
      {
        id: "codex",
        displayName: "",
        npmPackage: "",
        binary: "codex-cli",
        versionArgs: [],
        configDir: "",
      },
      {
        id: "claude-code",
        displayName: "",
        npmPackage: "",
        binary: "  ",
        versionArgs: [],
        configDir: "",
      },
    ]);
    expect(commandForTool("codex", config)).toBe("codex-cli");
    expect(commandForTool("claude-code", config)).toBe("claude");
    expect(commandForTool("codex", null)).toBe("codex");
    expect(commandForTool("claude-code", null)).toBe("claude");
  });
});

describe("outcomeOf / summarizeTools / allVerified", () => {
  it.each<[VerifyResult | undefined, ReturnType<typeof outcomeOf>]>([
    [undefined, "not_verified"],
    [verifyResult("codex", true, "1.2.3"), "ok"],
    [verifyResult("codex", false, null), "failed"],
  ])("outcomeOf(%j) → %s", (result, expected) => {
    expect(outcomeOf(result)).toBe(expected);
  });

  it("builds one row per selected tool in selection order", () => {
    const rows = summarizeTools(
      ["claude-code", "codex"],
      { codex: verifyResult("codex", true, "0.9.0") },
      null,
    );
    expect(rows).toEqual([
      { tool: "claude-code", outcome: "not_verified", version: null, command: "claude" },
      { tool: "codex", outcome: "ok", version: "0.9.0", command: "codex" },
    ]);
    expect(allVerified(rows)).toBe(false);
  });

  it("allVerified needs at least one tool and every tool ok", () => {
    expect(allVerified([])).toBe(false);
    expect(
      allVerified(summarizeTools(["codex"], { codex: verifyResult("codex", true, null) }, null)),
    ).toBe(true);
    expect(
      allVerified(
        summarizeTools(
          ["codex", "claude-code"],
          {
            codex: verifyResult("codex", true, null),
            "claude-code": verifyResult("claude-code", false, null),
          },
          null,
        ),
      ),
    ).toBe(false);
  });
});
