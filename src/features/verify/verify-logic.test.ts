import { describe, expect, it } from "vitest";

import type { AppConfig, Diagnosis, ToolSpec, VerifyResult } from "@/lib/types";

import {
  GATEWAY_TARGET,
  baseUrlNeedsHttps,
  primaryErrorClass,
  symptomsFromResult,
  toolBinary,
  unverifiedTools,
  verifyTelemetryEvent,
} from "./verify-logic";

function result(overrides: Partial<VerifyResult> = {}): VerifyResult {
  return {
    tool: "codex",
    cli: {
      ok: true,
      version: "0.42.0",
      path: "/usr/local/bin/codex",
      outputTail: null,
      errorClass: null,
    },
    gateway: null,
    runningTerminals: [],
    ok: true,
    diagnoses: [],
    ...overrides,
  };
}

const diag = (ruleId: string, severity: Diagnosis["severity"]): Diagnosis => ({
  ruleId,
  severity,
  code: `x.${ruleId}`,
  params: {},
  actions: [],
  checklist: [],
});

describe("symptomsFromResult", () => {
  it("yields nothing for a fully successful result", () => {
    expect(symptomsFromResult(result())).toEqual([]);
  });

  it("maps a missing command to command_not_found", () => {
    const r = result({
      ok: false,
      cli: {
        ok: false,
        version: null,
        path: null,
        outputTail: null,
        errorClass: "command_not_found",
      },
    });
    expect(symptomsFromResult(r)).toEqual([{ kind: "command_not_found", tool: "codex" }]);
  });

  it("maps any other CLI failure to command_failed with the redacted tail", () => {
    const r = result({
      ok: false,
      tool: "claude-code",
      cli: {
        ok: false,
        version: null,
        path: "/x",
        outputTail: "boom",
        errorClass: "command_failed",
      },
    });
    expect(symptomsFromResult(r)).toEqual([
      { kind: "command_failed", tool: "claude-code", outputTail: "boom" },
    ]);
  });

  it.each([
    [
      { ok: false, httpStatus: 401, latencyMs: 12, errorClass: "auth", message: null },
      { kind: "http_status", status: 401, tool: "codex" },
    ],
    [
      { ok: false, httpStatus: null, latencyMs: null, errorClass: "timeout", message: null },
      { kind: "timeout", target: GATEWAY_TARGET },
    ],
    [
      { ok: false, httpStatus: null, latencyMs: null, errorClass: "network", message: null },
      { kind: "network_error", target: GATEWAY_TARGET },
    ],
    [
      { ok: false, httpStatus: null, latencyMs: null, errorClass: "tls", message: null },
      { kind: "network_error", target: GATEWAY_TARGET },
    ],
    [
      {
        ok: false,
        httpStatus: null,
        latencyMs: null,
        errorClass: "protocol_mismatch",
        message: null,
      },
      { kind: "protocol_mismatch", tool: "codex" },
    ],
  ] as const)("maps gateway failure %j", (gateway, symptom) => {
    const r = result({ ok: false, gateway: { ...gateway } });
    expect(symptomsFromResult(r)).toEqual([symptom]);
  });

  it("ignores a successful gateway probe and unknown gateway failures without status", () => {
    expect(
      symptomsFromResult(
        result({
          gateway: { ok: true, httpStatus: 200, latencyMs: 30, errorClass: null, message: null },
        }),
      ),
    ).toEqual([]);
    expect(
      symptomsFromResult(
        result({
          ok: false,
          gateway: {
            ok: false,
            httpStatus: null,
            latencyMs: null,
            errorClass: "unknown",
            message: null,
          },
        }),
      ),
    ).toEqual([]);
  });

  it("adds no_effect_after_config only when something failed while terminals are running", () => {
    const terminals = [{ pid: 1, name: "WindowsTerminal.exe" }];
    expect(symptomsFromResult(result({ runningTerminals: terminals }))).toEqual([]);
    const failing = result({
      ok: false,
      runningTerminals: terminals,
      cli: {
        ok: false,
        version: null,
        path: null,
        outputTail: null,
        errorClass: "command_not_found",
      },
    });
    expect(symptomsFromResult(failing)).toEqual([
      { kind: "command_not_found", tool: "codex" },
      { kind: "no_effect_after_config", tool: "codex" },
    ]);
  });
});

describe("primaryErrorClass / verifyTelemetryEvent", () => {
  it("prefers the CLI error, then the gateway error, else null", () => {
    expect(primaryErrorClass(result())).toBeNull();
    expect(
      primaryErrorClass(
        result({
          ok: false,
          cli: {
            ok: false,
            version: null,
            path: null,
            outputTail: null,
            errorClass: "command_failed",
          },
          gateway: {
            ok: false,
            httpStatus: 401,
            latencyMs: null,
            errorClass: "auth",
            message: null,
          },
        }),
      ),
    ).toBe("command_failed");
    expect(
      primaryErrorClass(
        result({
          ok: false,
          gateway: {
            ok: false,
            httpStatus: 401,
            latencyMs: null,
            errorClass: "auth",
            message: null,
          },
        }),
      ),
    ).toBe("auth");
  });

  it("builds a step_result event without free text", () => {
    const r = result({
      ok: false,
      gateway: {
        ok: false,
        httpStatus: 401,
        latencyMs: null,
        errorClass: "auth",
        message: "secret",
      },
      diagnoses: [diag("C", "warning"), diag("A", "blocking")],
    });
    expect(verifyTelemetryEvent(r, 1234.6)).toEqual({
      name: "step_result",
      step: "verify",
      status: "fail",
      durationMs: 1235,
      errorClass: "auth",
      ruleId: "A",
    });
    expect(verifyTelemetryEvent(result(), -5)).toMatchObject({
      status: "pass",
      durationMs: 0,
      errorClass: null,
      ruleId: null,
    });
  });
});

describe("unverifiedTools / toolBinary", () => {
  it("lists selected tools that are not ok yet", () => {
    expect(unverifiedTools(["codex", "claude-code"], {})).toEqual(["codex", "claude-code"]);
    expect(
      unverifiedTools(["codex", "claude-code"], {
        codex: result(),
        "claude-code": result({ tool: "claude-code", ok: false }),
      }),
    ).toEqual(["claude-code"]);
    expect(unverifiedTools(["codex"], { codex: result() })).toEqual([]);
  });

  it("resolves the binary from the preset and falls back to the tool id", () => {
    const tools: ToolSpec[] = [
      {
        id: "claude-code",
        displayName: "Claude Code",
        npmPackage: "@anthropic-ai/claude-code",
        binary: "claude",
        versionArgs: ["--version"],
        configDir: "~/.claude",
      },
    ];
    const config = { tools } as unknown as AppConfig;
    expect(toolBinary(config, "claude-code")).toBe("claude");
    expect(toolBinary(config, "codex")).toBe("codex");
    expect(toolBinary(null, "codex")).toBe("codex");
  });
});

describe("baseUrlNeedsHttps", () => {
  it("flags every non-https address except loopback and empty input", () => {
    const cases: Array<[string, boolean]> = [
      ["https://gateway.example.com/v1", false],
      ["  https://gateway.example.com/v1  ", false],
      ["", false],
      ["   ", false],
      ["http://127.0.0.1:8080/v1", false],
      ["http://localhost/v1", false],
      ["http://[::1]/v1", false],
      ["http://gateway.example.com/v1", true],
      ["http://10.0.0.5/v1", true],
      ["ftp://gateway.example.com/v1", true],
      ["gateway.example.com/v1", true],
    ];
    for (const [input, expected] of cases) {
      expect(baseUrlNeedsHttps(input), input).toBe(expected);
    }
  });
});
