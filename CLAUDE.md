# CLAUDE.md — project guide for AI agents and humans

SeedRouter Onboarding (SeedRouter 接入引导工具) is a **Tauri 2 desktop app** (Rust core + React/TypeScript UI)
that walks non-developer users through: environment check → install missing pieces →
configure the service gateway inside **CC Switch** → verify → auto-diagnose. Windows + macOS,
zh-CN + en. PRD: `docs/prd/`. Architecture: `docs/ARCHITECTURE.md`. Decisions: `docs/adr/`.

## Commands

```bash
npm install                 # once; also sets git hooksPath via "prepare"
npm run tauri dev           # run the app (Vite + cargo)
npm run check               # full local gate: prettier, eslint, tsc, i18n parity, vitest, cargo fmt/clippy/test
npm run test                # vitest (jsdom)
npm run rust:test           # cargo test in src-tauri
npm run rust:clippy         # clippy with -D warnings (pedantic enabled — see Cargo.toml [lints])
npm run i18n:check          # locale parity with zh-CN + every Rust code has a translation (scripts/check-codes.mjs)
npm run tauri build         # produce installers (nsis / dmg)
```

Rust toolchain must be on PATH (`~/.cargo/bin`). First `cargo` build takes a few minutes.

## Layout

```
src-tauri/src/
  lib.rs          builder, plugins, command registration        commands.rs   thin #[tauri::command] layer
  models.rs       IPC DTOs  <-- mirror of src/lib/types.ts       error.rs      AppError -> {code,message,params}
  config.rs       company preset (resources/app-config.json)      state.rs      AppState (config, jobs, telemetry, http)
  platform.rs     OS/PATH introspection                            process.rs    ALL child-process execution
  net.rs          probes, mirror choice, verified downloads       redact.rs     secret redaction (use it everywhere)
  checks/  M1     install/  M2     guide.rs  M3     verify.rs  M4     diagnose/  M5     docs/  M6     telemetry.rs
src/
  lib/types.ts    IPC DTOs  <-- mirror of models.rs               lib/tauri.ts  typed invoke wrappers (only place invoke is called)
  lib/events.ts   event channel names + typed listeners           stores/       zustand (wizard, app)
  features/<step>/ one folder per wizard step + help              components/   layout + ui primitives
  i18n/locales/<lang>/<namespace>.json   namespaces: common checks install guide verify diagnose help
src-tauri/resources/app-config.json     company preset (gateway URL, mirrors, docs site, telemetry endpoint)
```

## Hard rules (from PRD answers; violating these is a bug, not a style issue)

1. **Never write** to `~/.codex/`, `~/.claude/` or `~/.cc-switch/` (PRD #4, ADR-0003). Reading for
   diagnostics is fine. Configuration is done by the user inside CC Switch; we guide and verify.
   The one-click import (ADR-0006) is a _hand-off_: it opens CC Switch's own
   `ccswitch://v1/import` deep link after user confirmation — CC Switch confirms again and does
   its own writing.
2. **Never modify environment variables** (PRD #17). Report conflicts + give instructions.
3. **API keys**: never persisted, logged, put in telemetry or diagnostic reports. They may be held
   in memory for gateway probes (connectivity test, model list, verify) and — only after the user
   confirms the masked preview — embedded in the `ccswitch://` import link handed to CC Switch
   (ADR-0006). Every string shown/logged/reported goes through
   `redact::redact_secrets`; known secrets are masked with `redact::mask_value`.
4. **Show before run**: any command executed on the user's machine is displayed
   (`InstallPlan.display_command`) and confirmed by the user first. No silent elevation (PRD #7).
5. **Bilingual by construction**: Rust returns _codes_ + params, never prose. Every user-facing
   string lives in both `zh-CN` and `en` locale files (`npm run i18n:check` enforces parity).
6. **All process execution** goes through `process.rs` (timeouts, no console window on Windows).
   All HTTP goes through the shared `reqwest::Client` in `AppState` (rustls, proxy-aware).
7. **Downloads** must be https and, when a hash is available, SHA-256 verified before hand-off.
8. Rust `[lints]`: `unwrap_used`, `expect_used`, `print_*`, `dbg_macro` are **denied** outside tests.

## Conventions

- Rust: `AppResult<T>` everywhere; domain modules are UI-agnostic (no `AppHandle` except where
  events are emitted); commands are thin. Add unit tests next to logic (`#[cfg(test)]`).
- TS: strict; `import type`; no `invoke` outside `lib/tauri.ts`; screens read state from zustand
  stores; components in `components/ui` are dumb. Tests: `*.test.ts(x)` beside the source.
- i18n keys: `namespace:section.key` (`t("checks:node.too_old", params)`); codes from Rust map
  1:1 (`CheckResult.code = "node.too_old"` → `checks:node.too_old`).
- Commits: Conventional Commits, English subject ≤ 72 chars, scope = module (`feat(checks): …`).
  Enforced by `.githooks/commit-msg`. See `CONTRIBUTING.md`.
- When you change `models.rs`, change `src/lib/types.ts` in the same commit (and vice versa).
- Placeholder values in `app-config.json` marked `TODO(IT …)` are tracked in `docs/OPEN_QUESTIONS.md`.
