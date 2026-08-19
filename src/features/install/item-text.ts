/**
 * Text helpers shared by the install item bodies (kept out of the component files so React
 * Fast Refresh keeps working there).
 */
import type { InstallDoneEvent } from "@/lib/types";

type Translate = (key: string, opts?: Record<string, unknown>) => string;

/** Localised name of the release source (`install:release.source.<id>`; raw id as fallback). */
export function releaseSourceLabel(t: Translate, source: string): string {
  return t(`install:release.source.${source}`, { defaultValue: source });
}

/**
 * Headline for a job that did not succeed. `ns` picks the wording: `npm` (mentions switching
 * the registry) or `installer` (a downloaded installer was run).
 */
export function jobFailureMessage(
  t: Translate,
  done: InstallDoneEvent | null,
  ns: "npm" | "installer" = "npm",
): string {
  if (!done) return t(`install:${ns}.startFailed`);
  if (done.cancelled) return t(`install:${ns}.cancelled`);
  if (done.timedOut) {
    return t(`install:${ns}.timedOut`, {
      minutes: Math.max(1, Math.round(done.durationMs / 60_000)),
    });
  }
  if (done.exitCode === null) return t(`install:${ns}.failedNoCode`);
  return t(`install:${ns}.failedHint`, { code: done.exitCode });
}
