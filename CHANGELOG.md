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
