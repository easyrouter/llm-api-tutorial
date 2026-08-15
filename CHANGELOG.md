# Changelog

All notable changes to this project are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning: [SemVer](https://semver.org/).

## [Unreleased]

### Added

- Project scaffold: Tauri 2 + React 19 + TypeScript + Vite + Tailwind 4; Rust core module
  skeleton for M1–M6; IPC contract (`models.rs` / `types.ts`); company preset config; bilingual
  i18n (zh-CN / en) with parity check; wizard shell (stepper, welcome screen); CI (lint /
  typecheck / test on web; fmt / clippy / test on Windows + macOS; bundle job); git hooks
  (Conventional Commits, pre-commit gate); development standards (`CLAUDE.md`,
  `CONTRIBUTING.md`, `docs/`).
- Shared UI kit (`src/components/ui`): StatusBadge, Alert, CopyField, LogView, ProgressBar,
  Spinner, ConfirmDialog, KeyValueList, FixActionButtons, StepFooter, ErrorBanner,
  ExternalLink; helpers `lib/format`, `lib/errors`, `lib/useAsync`, `lib/clipboard`, hooks
  `useCopy` / `useStickToBottom`; Tauri test doubles (`src/test/mocks/tauri.ts`) installed
  globally by the vitest setup.
- Core services: `process` (timeouts, cancellation, fresh-session env, no console window),
  `net` (probes, mirror choice, https + SHA-256 verified downloads), `platform` (OS / PATH /
  npm prefix introspection) and `config` (resource → embedded → override load order).
- M6 help docs: remote → cache → bundled cascade with bilingual bundled pages
  (`src-tauri/resources/docs/{zh-CN,en}`), shipped as bundle resources; telemetry batched
  opt-in flush with a periodic background flush started at app setup.
- M4 verification (`verify`): fresh-session `<cli> --version` check (guide fault E aware),
  minimal gateway probe per protocol with status → error-class mapping and key-scrubbed
  messages, running-terminal detection (Windows / macOS bundles, own process tree excluded),
  symptoms fed into the M5 rule engine.
- M1 environment checks (`checks`): OS / Node / npm / Codex / Claude Code / CC Switch /
  env-var conflicts / network probes run concurrently and streamed over `checks://progress`;
  three-state verdicts with `FixAction`s (install, open URL, instructions, rerun); "installed
  but not on PATH" and "broken binary" distinguished from "missing".
- M2 install (`install`): show-before-run `InstallPlan`s (npm global installs with the chosen
  registry mirror and per-user-prefix admin detection, Node download page / Homebrew, CC Switch
  release lookup from GitHub or an intranet `latest.json`), streamed `npm install -g` jobs
  with cancel, SHA-256 verified installer downloads. `InstallPlan.downloadUrl` added to the
  IPC contract.
- M3 guide (`guide`): ordered CC Switch walkthrough steps per tool with copyable preset
  values, API-key format validation (never stored) and the URL rule preview (Q-U1).
- M5 diagnose (`diagnose`): rule engine A–G plus network / upstream rules with table-driven
  tests, platform-specific instructions, and the fully redacted Markdown diagnostic report
  (read-only inspection of `~/.codex` / `~/.claude` / CC Switch data dirs, names only).
- UI: Environment check screen (streamed rows, per-row re-check, install fixes, continue-anyway
  gate, snapshot-based diagnosis panel), Install screen (plan confirmation, live output, mirror
  retry, Node download flow, CC Switch release download with integrity badge), Configure
  walkthrough (preset values, key / URL validation cards, per-tool step list), Verify screen
  with diagnosis panel and report copy / export, Help panel (docs tree, Markdown viewer,
  wizard links) and Done screen.
- Tooling: `scripts/check-codes.mjs` (part of `npm run i18n:check`) verifies that every code
  the Rust core emits has a `zh-CN` translation.
