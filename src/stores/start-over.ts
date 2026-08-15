/**
 * "Start over" — resets every store that carries per-run wizard state so a second run begins
 * clean: the wizard itself (step, snapshot, verification results, help state) and the install
 * store (explicit install requests, skipped items). The docs store is deliberately kept: its
 * cached index/pages are still valid and the last-read topic is not per-run state.
 */
import { useInstallStore } from "./install";
import { useWizardStore } from "./wizard";

export function startOver(): void {
  useInstallStore.getState().reset();
  useWizardStore.getState().reset();
}
