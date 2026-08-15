import { describe, expect, it } from "vitest";

import type { KeyIssue, KeyValidation } from "@/lib/types";

import { BLOCKING_KEY_ISSUES, isBlockingKeyIssue, splitKeyIssues } from "./key-issues";

const v = (valid: boolean, issues: KeyIssue[], length = 40): KeyValidation => ({
  valid,
  issues,
  length,
});

describe("isBlockingKeyIssue", () => {
  it.each<[KeyIssue, boolean]>([
    ["empty", true],
    ["leading_or_trailing_whitespace", true],
    ["contains_whitespace", true],
    ["contains_newline", true],
    ["non_ascii", true],
    ["looks_like_placeholder", true],
    ["unexpected_prefix", false],
    ["too_short", false],
  ])("%s → blocking=%s", (issue, expected) => {
    expect(isBlockingKeyIssue(issue)).toBe(expected);
    expect(BLOCKING_KEY_ISSUES.has(issue)).toBe(expected);
  });
});

describe("splitKeyIssues", () => {
  it("returns nothing for a clean valid key", () => {
    expect(splitKeyIssues(v(true, []))).toEqual({ blocking: [], hints: [] });
  });

  it("treats every issue of a valid key as a hint", () => {
    expect(splitKeyIssues(v(true, ["unexpected_prefix", "too_short"]))).toEqual({
      blocking: [],
      hints: ["unexpected_prefix", "too_short"],
    });
  });

  it("splits an invalid key into blocking and hints", () => {
    const out = splitKeyIssues(
      v(false, ["leading_or_trailing_whitespace", "unexpected_prefix", "contains_newline"]),
    );
    expect(out.blocking).toEqual(["leading_or_trailing_whitespace", "contains_newline"]);
    expect(out.hints).toEqual(["unexpected_prefix"]);
  });

  it("promotes all issues to blocking when Rust says invalid but none is in the blocking set", () => {
    expect(splitKeyIssues(v(false, ["too_short"]))).toEqual({
      blocking: ["too_short"],
      hints: [],
    });
  });

  it("de-duplicates repeated issues", () => {
    expect(splitKeyIssues(v(false, ["empty", "empty"]))).toEqual({
      blocking: ["empty"],
      hints: [],
    });
  });
});
