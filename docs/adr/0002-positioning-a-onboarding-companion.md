# ADR-0002: Positioning A — onboarding companion around CC Switch

- Status: accepted
- Date: 2026-08-15

## Context

PRD §二 offered three positionings. IT answered: multi-provider switching is a real need (#3),
the tool may **not** write `~/.codex/` (#4), scope includes Codex CLI and Claude Code (#5),
the gateway supports Responses (#2).

## Decision

Positioning **A**: the tool orchestrates the whole path (check → install → guide → verify →
diagnose) but the configuration itself is performed by the user inside CC Switch. The tool
supplies preset values (gateway URL etc.), validates user input, and auto-verifies each step.
CC Switch is a required install target.

## Consequences

- No config-file ownership conflict with CC Switch; upstream Codex/Claude config-format changes
  do not break us.
- UX cost: the configure step is a guided walkthrough, not one click. Mitigated by copy buttons
  for every value and per-step auto-verification.
- CC Switch UI changes require updating walkthrough text (kept as i18n codes; no screenshots
  bundled — the docs site can carry images).
- Positionings B/C are documented as rejected; revisit only if #4 changes.
