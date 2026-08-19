# ADR-0003: Never write to `~/.codex`, `~/.claude`, `~/.cc-switch`; never edit env vars

- Status: superseded in part by ADR-0008 (2026-08-19)
- Date: 2026-08-15

> ADR-0008 allows, after an explicit user confirmation of a shown plan: writing
> `~/.codex/config.toml` (with backup), editing the **user** PATH, removing conflicting env vars
> and running downloaded installers with the OS elevation prompt. Everything else below — no
> writes to `~/.claude` / `~/.cc-switch`, no silent env-var or machine-PATH changes — still
> holds.

## Context

PRD #4 (may not write `~/.codex/`) and #17 (env-var cleanup must be manual). The same reasoning
applies to `~/.claude` (Claude Code is in scope, #5) and to CC Switch's own data directory.

## Decision

The Rust core has no code path that writes to those directories or modifies environment
variables (process, user or machine scope). Reading them for diagnostics is allowed; values are
masked/redacted before display. Remediation for env-var conflicts is delivered as step-by-step
instructions (`FixAction::Instructions`).

## Consequences

- Guarantees no state conflict with CC Switch and no accidental credential exposure.
- Enforced by code review (PR checklist item) and by keeping filesystem writes limited to the
  app's own cache/config/log dirs (`docs/ARCHITECTURE.md` §2).
