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
npm run nsis:check          # src-tauri/nsis/hooks.nsh registry paths match tauri.conf.json (product name / publisher)
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
  remediate.rs    one-click PATH repair / env-var cleanup / ~/.codex/config.toml — plan → confirm → apply (ADR-0008)
  install/installer.rs  run a downloaded installer (Node MSI/pkg, Codex app MSIX/DMG) as an install job; exit-code rules
  install/node.rs       Node LTS discovery on the chosen dist mirror (index.json → asset → SHASUMS256.txt)
  checks/codex_app.rs   Codex desktop client (ChatGPT app) check — Warn + install / Store / region-settings fixes
  fast_ui.rs      optional Codex Fast UI toolkit (Windows only, opt-in — ADR-0007)
src/
  lib/types.ts    IPC DTOs  <-- mirror of models.rs               lib/tauri.ts  typed invoke wrappers (only place invoke is called)
  lib/events.ts   event channel names + typed listeners           stores/       zustand (wizard, app)
  features/<step>/ one folder per wizard step + help              components/   layout + ui primitives
  i18n/locales/<lang>/<namespace>.json   namespaces: common checks install guide verify diagnose help
src-tauri/resources/app-config.json     company preset (gateway URL, mirrors, docs site, telemetry endpoint)
src-tauri/nsis/hooks.nsh                NSIS installer hooks (legacy-publisher upgrade fix); literal paths checked by nsis:check
```

## Hard rules (from PRD answers; violating these is a bug, not a style issue)

1. **Never write** to `~/.claude/` or `~/.cc-switch/` (ADR-0003). Reading for diagnostics is
   fine. Provider configuration is done by the user inside CC Switch; we guide and verify. The
   one-click import (ADR-0006) is a _hand-off_ via CC Switch's own `ccswitch://v1/import` link.
   The **only** file this app writes under `~/.codex/` is `config.toml`, and only through
   `remediate::apply_codex_config` after the user confirmed the shown template, with the existing
   file backed up as `config.toml.seedrouter-<ts>.bak` (`restore_codex_config` undoes it) —
   ADR-0008, PRD #4 revised 2026-08-19.
2. **Environment variables / PATH are changed only through `remediate`** (plan → confirm →
   apply; ADR-0008, PRD #17 revised): `plan_path_repair`/`apply_path_repair` may _append_ one dir
   to the **user** PATH (`HKCU\Environment\Path` + broadcast, or an `export PATH=…` line in the
   login-shell rc file with backup); `plan_env_cleanup`/`apply_env_cleanup` may remove conflict
   variables from user/machine registry, rc files (commented out, backup) and `launchctl`.
   Never silently, never the machine PATH, never anything the user did not see in the plan.
3. **API keys**: never persisted, logged, put in telemetry or diagnostic reports. They may be held
   in memory for gateway probes (connectivity test, model list, verify), substituted into the
   config.toml at apply time, and — only after the user confirms the masked preview — embedded
   in the `ccswitch://` import link (ADR-0006). `EnvCleanupPlan` carries the **full** env-var
   values for the confirmation dialog only — never log them, never put them in telemetry or
   reports. Every other string shown/logged/reported goes through `redact::redact_secrets`;
   known secrets are masked with `redact::mask_value`.
4. **Show before run**: any command executed or file changed on the user's machine is displayed
   first (`InstallPlan.display_command` incl. installer-run plans, `PathRepairPlan`,
   `EnvCleanupPlan` items, the config.toml template, `FastUiPlan`) and confirmed by the user.
   Every plan is re-derived server-side and compared before it runs, so a tampered webview
   cannot substitute a different one. **No silent elevation** (PRD #7): admin steps are
   labelled (`requiresAdmin` → "(需要管理员权限)" / "(needs administrator rights)") and
   elevation happens only through the OS prompt of the installer itself (`msiexec`,
   `installer -pkg` via `osascript`) or `Start-Process -Verb RunAs` for `reg delete` — this app
   never runs elevated.
5. **Bilingual by construction**: Rust returns _codes_ + params, never prose. Every user-facing
   string lives in both `zh-CN` and `en` locale files (`npm run i18n:check` enforces parity).
6. **All process execution** goes through `process.rs` (timeouts, no console window on Windows).
   All HTTP goes through the shared `reqwest::Client` in `AppState` (rustls, proxy-aware).
7. **Downloads** must be https and, when a hash is available, SHA-256 verified before hand-off.
   Bundled third-party payloads follow the same rule offline: the Codex Fast UI archive is pinned
   by `fast_ui::TOOLKIT_SHA256` and re-verified before every extraction (ADR-0007).
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
