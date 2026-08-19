import { describe, expect, it } from "vitest";

import i18n from "@/i18n";

import { describeError, errorCode, OTHER_ERROR_CODE, toWireError } from "./errors";
import type { WireError } from "./types";

const t = (key: string, options?: Record<string, unknown>) => i18n.t(key, options);

describe("toWireError", () => {
  it("passes real wire errors through unchanged", () => {
    const wire: WireError = {
      code: "command_failed",
      message: "npm failed",
      params: { program: "npm", code: "1" },
    };
    expect(toWireError(wire)).toEqual(wire);
  });

  it("defaults missing params on wire errors", () => {
    const partial = { code: "network", message: "connect" } as unknown;
    expect(toWireError(partial)).toEqual({ code: "network", message: "connect", params: {} });
  });

  it.each([
    [new Error("boom"), "boom"],
    [new TypeError(""), "TypeError"],
    ["plain string", "plain string"],
    [undefined, ""],
    [null, ""],
    [42, "42"],
    [{ message: "object with message" }, "object with message"],
    [{ foo: "bar" }, '{"foo":"bar"}'],
  ])("coerces %s into an other-error with message %j", (input, message) => {
    expect(toWireError(input)).toEqual({ code: OTHER_ERROR_CODE, message, params: {} });
  });

  it("errorCode exposes the code", () => {
    expect(errorCode({ code: "io", message: "x", params: {} })).toBe("io");
    expect(errorCode(new Error("x"))).toBe(OTHER_ERROR_CODE);
  });
});

describe("describeError", () => {
  it("uses the translated message for a known code with params", async () => {
    await i18n.changeLanguage("en");
    const wire: WireError = {
      code: "command_failed",
      message: "raw",
      params: { program: "npm", code: "1" },
    };
    expect(describeError(t, wire)).toBe("Command npm failed (exit code 1).");
  });

  it("works in zh-CN too", async () => {
    await i18n.changeLanguage("zh-CN");
    const wire: WireError = { code: "network", message: "raw", params: {} };
    expect(describeError(t, wire)).toBe("网络请求失败。");
  });

  it("prefers the specific key named by the reason param", async () => {
    await i18n.changeLanguage("en");
    const withReason: WireError = {
      code: "unsupported",
      message: "raw",
      params: { reason: "common:errors.network" },
    };
    expect(describeError(t, withReason)).toBe(t("common:errors.network"));
    expect(describeError(t, withReason)).not.toBe(t("errors.unsupported"));
  });

  it("falls back to errors.<code> when the reason key has no translation", async () => {
    await i18n.changeLanguage("en");
    const wire: WireError = {
      code: "unsupported",
      message: "raw",
      params: { reason: "guide:fastui.blocked.no_such_key" },
    };
    expect(describeError(t, wire)).toBe(t("errors.unsupported"));
  });

  it("falls back to errors.generic for unknown codes", async () => {
    await i18n.changeLanguage("en");
    const wire: WireError = { code: "not_a_real_code", message: "detail here", params: {} };
    expect(describeError(t, wire)).toBe("Operation failed: detail here");
  });

  it("shows the raw message for JS-side errors", async () => {
    await i18n.changeLanguage("en");
    expect(describeError(t, new Error("kaboom"))).toBe("Operation failed: kaboom");
  });

  it("uses errors.other for a message-less other error", async () => {
    await i18n.changeLanguage("en");
    expect(describeError(t, undefined)).toBe("Unknown error.");
  });
});
