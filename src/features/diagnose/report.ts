/**
 * Diagnostic report export (M5): builds the redacted Markdown in Rust, then either copies it or
 * asks the user where to save it via the native save dialog. The only place that touches
 * `@tauri-apps/plugin-dialog`, so tests mock exactly one module.
 *
 * The Markdown never contains an API key: Rust builds it from the snapshot, the diagnoses and
 * the (already redacted) verification results only and passes every free-text field through
 * `redact::redact_secrets`; `save_diagnostic_report` redacts again before writing.
 */
import { save } from "@tauri-apps/plugin-dialog";

import { buildDiagnosticReport, saveDiagnosticReport } from "@/lib/tauri";
import type { Diagnosis, EnvSnapshot, VerifyResult } from "@/lib/types";

/** Suggested file name shown in the save dialog. */
export const REPORT_DEFAULT_FILE_NAME = "seedrouter-onboarding-report.md";

export type ExportOutcome = { kind: "saved"; path: string } | { kind: "cancelled" };

/**
 * Fetches the redacted report Markdown from the Rust core. `verify` carries the latest
 * verification result per tool so the report keeps the raw error details (HTTP status, redacted
 * server message, CLI output tail) even when no diagnosis rule matched.
 */
export async function fetchReportMarkdown(
  snapshot: EnvSnapshot | null,
  diagnoses: readonly Diagnosis[],
  verify: readonly VerifyResult[] = [],
): Promise<string> {
  const report = await buildDiagnosticReport(snapshot, [...diagnoses], [...verify]);
  return report.markdown;
}

/**
 * Opens the native "Save as…" dialog (Markdown filter, default name
 * `seedrouter-onboarding-report.md`) and writes the report to the chosen path.
 * Resolves `{ kind: "cancelled" }` when the user dismisses the dialog.
 */
export async function exportDiagnosticReport(
  snapshot: EnvSnapshot | null,
  diagnoses: readonly Diagnosis[],
  verify: readonly VerifyResult[] = [],
): Promise<ExportOutcome> {
  const markdown = await fetchReportMarkdown(snapshot, diagnoses, verify);
  const path = await save({
    defaultPath: REPORT_DEFAULT_FILE_NAME,
    filters: [{ name: "Markdown", extensions: ["md"] }],
  });
  if (!path) return { kind: "cancelled" };
  await saveDiagnosticReport(path, markdown);
  return { kind: "saved", path };
}
