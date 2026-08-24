# ADR-0006: One-click provider hand-off via CC Switch's `ccswitch://` deep link

- Status: accepted
- Date: 2026-08-16

## Context

The test team asked for a one-click way to add/edit the provider in the local CC Switch
(revision R3), while ADR-0003 forbids writing to `~/.cc-switch` (and `~/.codex`, `~/.claude`).
CC Switch ≥ 3.x ships an official import mechanism for exactly this purpose: the
`ccswitch://v1/import?resource=provider&…` deep link (documented in its user manual, §5.3).
Opening the link starts CC Switch, which validates the payload, shows its own confirmation
dialog with a preview, and only then writes to its own data store (with its own backups). For
`app=codex` it generates a complete `config.toml` snippet (`wire_api = "responses"`); for
`app=claude` it fills the `env`-based settings.

## Decision

One-click import is implemented as a _hand-off_, not a write:

- `guide::build_import_url` renders the deep link from the values on the configure screen
  (provider name, effective base URL, API key, optional model); `guide::masked_import_url`
  renders the same link with the key masked. The endpoint is the effective URL **for that
  tool's protocol** (`config::gateway_defaults` / `tool_protocol`): `app=claude` gets the bare
  root, because Claude Code appends `/v1/messages` to it itself, while `app=codex` gets the
  `…/v1` form.
- "Show before run" (hard rule 4) applies: the UI shows the masked link in a confirmation
  dialog first; only after the user confirms does `open_cc_switch_import` open the real link
  via the OS opener. CC Switch then asks for confirmation a second time before importing.
- The API key is embedded in the link (CC Switch requires it for a provider import). It is
  held in memory only, never logged (opener errors are scrubbed), and never persisted by this
  app — this extends hard rule 3's "one gateway probe" wording to the configure screen's
  in-memory usage (connectivity test, model list, confirmed import).
- The manual copy/paste walkthrough remains fully available; repeated imports create a new
  provider in CC Switch (its deep link cannot edit an existing one), which the UI says out
  loud.

## Consequences

- ADR-0003 stands untouched: this app still has no code path that writes to `~/.cc-switch`,
  `~/.codex` or `~/.claude` — CC Switch does its own, user-confirmed writing.
- The feature degrades gracefully: without a (recent enough) CC Switch the deep link fails to
  open and the UI points at the install step / manual flow.
- The deep link contents leave the app through the OS URL-open mechanism; that is the
  documented CC Switch integration path and requires two explicit user confirmations.
