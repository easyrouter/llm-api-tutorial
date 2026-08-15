/**
 * Presentation logic for `KeyValidation` (M3 key format check). The Rust side decides `valid`;
 * this module only decides how each issue is *styled*: blocking (must fix before pasting into
 * CC Switch) versus hint (worth a look, but gateways differ).
 */
import type { KeyIssue, KeyValidation } from "@/lib/types";

/** Issues that make a key unusable regardless of gateway (mirrors `guide::validate_api_key`). */
export const BLOCKING_KEY_ISSUES: ReadonlySet<KeyIssue> = new Set<KeyIssue>([
  "empty",
  "leading_or_trailing_whitespace",
  "contains_whitespace",
  "contains_newline",
  "non_ascii",
  "looks_like_placeholder",
]);

export function isBlockingKeyIssue(issue: KeyIssue): boolean {
  return BLOCKING_KEY_ISSUES.has(issue);
}

export interface SplitKeyIssues {
  blocking: KeyIssue[];
  hints: KeyIssue[];
}

/**
 * Splits issues into blocking vs hint groups, staying consistent with Rust's verdict:
 * - `valid === true`  → everything is a hint (Rust says nothing blocks);
 * - `valid === false` → issues in `BLOCKING_KEY_ISSUES` block, the rest are hints — unless none
 *   of them is in the set, in which case all are promoted to blocking so the UI never shows an
 *   invalid key with only soft hints.
 */
export function splitKeyIssues(validation: KeyValidation): SplitKeyIssues {
  const issues = dedupe(validation.issues);
  if (validation.valid) return { blocking: [], hints: issues };
  const blocking = issues.filter(isBlockingKeyIssue);
  if (blocking.length === 0) return { blocking: issues, hints: [] };
  return { blocking, hints: issues.filter((i) => !isBlockingKeyIssue(i)) };
}

function dedupe(issues: readonly KeyIssue[]): KeyIssue[] {
  return [...new Set(issues)];
}
