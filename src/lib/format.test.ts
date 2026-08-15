import { describe, expect, it } from "vitest";

import { formatBytes, formatDuration, formatPercent, maskSecret } from "./format";

describe("formatBytes", () => {
  it.each([
    [0, "0 B"],
    [-5, "0 B"],
    [Number.NaN, "0 B"],
    [1, "1 B"],
    [512, "512 B"],
    [1023, "1023 B"],
    [1024, "1 KB"],
    [1536, "1.5 KB"],
    [1048576, "1 MB"],
    [12.3 * 1024 * 1024, "12.3 MB"],
    [1024 ** 3, "1 GB"],
    [1024 ** 4 * 3, "3 TB"],
    [1024 ** 5, "1024 TB"],
  ])("formats %s as %s", (input, expected) => {
    expect(formatBytes(input)).toBe(expected);
  });
});

describe("formatDuration", () => {
  it.each([
    [0, "0 ms"],
    [-1, "0 ms"],
    [Number.NaN, "0 ms"],
    [850, "850 ms"],
    [999.4, "999 ms"],
    [999.6, "1 s"],
    [1000, "1 s"],
    [2400, "2.4 s"],
    [59_940, "59.9 s"],
    [60_000, "1 min 00 s"],
    [65_000, "1 min 05 s"],
    [3_599_000, "59 min 59 s"],
    [3_600_000, "1 h 00 min"],
    [3_720_000, "1 h 02 min"],
  ])("formats %s ms as %s", (input, expected) => {
    expect(formatDuration(input)).toBe(expected);
  });
});

describe("formatPercent", () => {
  it.each([
    [0, "0%"],
    [0.42, "42%"],
    [0.999, "100%"],
    [1, "100%"],
    [1.5, "100%"],
    [-0.2, "0%"],
    [Number.NaN, "0%"],
  ])("formats %s as %s", (input, expected) => {
    expect(formatPercent(input)).toBe(expected);
  });
});

describe("maskSecret (mirror of Rust mask_value)", () => {
  it.each([
    ["sk-abcdefghijkl", "sk-****ijkl"],
    ["short", "*****"],
    ["12345678", "********"],
    ["123456789", "123****6789"],
    ["  sk-abcdefghijkl  ", "sk-****ijkl"],
    ["", ""],
    ["   ", ""],
    ["密码密码密码密码密码", "密码密****密码密码"],
  ])("masks %j as %j", (input, expected) => {
    expect(maskSecret(input)).toBe(expected);
  });
});
