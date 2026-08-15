# ADR-0003: Never write to `~/.codex`, `~/.claude`, `~/.cc-switch`; never edit env vars

- Status: accepted
- Date: 2026-08-15

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
