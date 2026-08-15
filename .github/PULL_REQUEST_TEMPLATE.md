## What

<!-- One paragraph: what changes and why. Link the PRD module (M1–M6) or issue. -->

## Checklist

- [ ] `npm run check` passes locally
- [ ] New user-facing strings added to **both** `zh-CN.json` and `en.json`
- [ ] No writes to `~/.codex/`, `~/.claude/` or `~/.cc-switch/` (ADR-0003)
- [ ] API keys never logged, persisted, or included in telemetry / diagnostic reports
- [ ] Every command executed on the user's machine is shown to the user before it runs
- [ ] Docs updated (`docs/`, `CHANGELOG.md`) if behaviour changed
