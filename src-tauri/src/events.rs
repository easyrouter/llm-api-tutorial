//! Names of the Tauri event channels emitted by the Rust core.
//! Mirrored in `src/lib/events.ts`.

/// `InstallOutputEvent` — one line of stdout/stderr from a running install job.
pub const INSTALL_OUTPUT: &str = "install://output";
/// `InstallDoneEvent` — an install job finished (success, failure or cancel).
pub const INSTALL_DONE: &str = "install://done";
/// `DownloadProgressEvent` — bytes downloaded so far for a download job.
pub const DOWNLOAD_PROGRESS: &str = "download://progress";
/// `CheckResult` — an individual environment check completed (streamed while `run_env_checks` runs).
pub const CHECK_PROGRESS: &str = "checks://progress";
/// `InstallOutputEvent` — one line from the optional Codex Fast UI toolkit (ADR-0007).
pub const FAST_UI_OUTPUT: &str = "fastui://output";
/// `InstallDoneEvent` — a Codex Fast UI toolkit job finished.
pub const FAST_UI_DONE: &str = "fastui://done";
