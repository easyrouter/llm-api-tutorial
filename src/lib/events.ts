/**
 * Tauri event channel names emitted by the Rust core (mirror of `src-tauri/src/events.rs`),
 * plus typed `listen` helpers.
 */
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  CheckResult,
  DownloadProgressEvent,
  InstallDoneEvent,
  InstallOutputEvent,
} from "./types";

export const EVENTS = {
  installOutput: "install://output",
  installDone: "install://done",
  downloadProgress: "download://progress",
  checkProgress: "checks://progress",
  fastUiOutput: "fastui://output",
  fastUiDone: "fastui://done",
} as const;

export const onInstallOutput = (cb: (e: InstallOutputEvent) => void): Promise<UnlistenFn> =>
  listen<InstallOutputEvent>(EVENTS.installOutput, (ev) => cb(ev.payload));

export const onInstallDone = (cb: (e: InstallDoneEvent) => void): Promise<UnlistenFn> =>
  listen<InstallDoneEvent>(EVENTS.installDone, (ev) => cb(ev.payload));

export const onDownloadProgress = (cb: (e: DownloadProgressEvent) => void): Promise<UnlistenFn> =>
  listen<DownloadProgressEvent>(EVENTS.downloadProgress, (ev) => cb(ev.payload));

export const onCheckProgress = (cb: (e: CheckResult) => void): Promise<UnlistenFn> =>
  listen<CheckResult>(EVENTS.checkProgress, (ev) => cb(ev.payload));

/** Codex Fast UI toolkit output — same payload shape as the install channels (ADR-0007). */
export const onFastUiOutput = (cb: (e: InstallOutputEvent) => void): Promise<UnlistenFn> =>
  listen<InstallOutputEvent>(EVENTS.fastUiOutput, (ev) => cb(ev.payload));

export const onFastUiDone = (cb: (e: InstallDoneEvent) => void): Promise<UnlistenFn> =>
  listen<InstallDoneEvent>(EVENTS.fastUiDone, (ev) => cb(ev.payload));
