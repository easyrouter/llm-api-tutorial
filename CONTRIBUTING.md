# Contributing / 开发规范

This document is the development standard for the project. `CLAUDE.md` is the short version
for agents; `docs/ARCHITECTURE.md` explains the design; `docs/adr/` records why.

## 1. Branching

- `main` is always releasable; protected — changes land via pull request only.
- Branch names: `feat/<module>-<short>`, `fix/<module>-<short>`, `docs/<short>`,
  `chore/<short>`, `release/vX.Y.Z`. Module = `checks | install | guide | verify | diagnose |
docs | telemetry | ui | core | ci`.
- Rebase on `main` before opening the PR; squash-merge; delete the branch after merge.

## 2. Commits — Conventional Commits

```
<type>(<scope>)!: <subject ≤ 72 chars, imperative, English>

<body — what & why, wrap at 100; Chinese is fine here>

Refs: PRD M1 / #issue
```

Types: `feat fix docs style refactor perf test build ci chore revert`.
Scope: module name (see above) or `i18n`, `config`, `deps`. `!` marks a breaking change of the
IPC contract (`models.rs` / `types.ts`) or config schema. Enforced by `.githooks/commit-msg`
(activated automatically by `npm install` through the `prepare` script; or run
`git config core.hooksPath .githooks`).

One logical change per commit. Keep IPC contract changes (Rust + TS mirror) in one commit.

## 3. Definition of done (per PR)

- `npm run check` passes locally; CI green on Windows **and** macOS.
- New / changed user-facing strings exist in **both** `zh-CN` and `en`
  (`npm run i18n:check`). Rust never emits prose — only codes + params.
- Hard rules honoured (see `CLAUDE.md` §Hard rules): no writes to `~/.codex` / `~/.claude` /
  `~/.cc-switch`, no env-var edits, no key persistence/logging, commands shown before run.
- Unit tests for new logic (Rust `#[cfg(test)]`, TS `*.test.ts(x)`); rule-engine changes in
  `diagnose/` need a table-driven test per rule.
- `docs/` updated when behaviour or contracts change; `CHANGELOG.md` gets an entry under
  _Unreleased_.
- PR description follows `.github/PULL_REQUEST_TEMPLATE.md`; at least one reviewer approval.

## 4. Code style

| area          | tool                             | config                                                                              |
| ------------- | -------------------------------- | ----------------------------------------------------------------------------------- |
| Rust format   | `cargo fmt`                      | `src-tauri/rustfmt.toml` (max_width 100)                                            |
| Rust lint     | `cargo clippy -D warnings`       | `[lints]` in `src-tauri/Cargo.toml` — pedantic on, `unwrap/expect/print/dbg` denied |
| TS/TSX format | Prettier                         | `.prettierrc` (100 cols, double quotes, trailing commas, Tailwind class sorting)    |
| TS lint       | ESLint flat config, type-checked | `eslint.config.js`                                                                  |
| Editor        | EditorConfig                     | `.editorconfig` (LF, UTF-8, 2 spaces; Rust 4)                                       |

Naming: Rust `snake_case` modules/functions, `CamelCase` types, serde `camelCase` on the wire;
TS `camelCase` values, `PascalCase` components/types, files `PascalCase.tsx` for components and
`kebab-case.ts` otherwise; i18n keys `camelCase` leaves inside `snake_case` sections that mirror
Rust codes.

Errors: Rust returns `AppResult<T>`; never `unwrap()` outside tests; map external errors into
`AppError` at module boundaries. TS surfaces `WireError.code` via `errors.<code>` i18n keys.

## 5. Security & privacy checklist

- Secrets: never in logs, telemetry, reports, git. `redact::redact_secrets` on every string that
  leaves the process boundary. Signing certificates never enter the repository (`.gitignore`).
- Downloads: https only, SHA-256 verified when a hash is available, saved to the app cache dir.
- Opening URLs/files: only `http(s)` URLs and files inside the app cache dir (`commands.rs`).
- Telemetry: opt-in, disabled until an intranet endpoint is configured; schema in
  `docs/ARCHITECTURE.md`; nothing identifying.

## 6. Versioning & releases

Semantic versioning; the version lives in `package.json`, `src-tauri/Cargo.toml` and
`src-tauri/tauri.conf.json` (keep them equal). Release process: `docs/RELEASE.md`.
Changelog: _Keep a Changelog_ format in `CHANGELOG.md`.

## 7. Documentation

- ADRs in `docs/adr/NNNN-title.md` (MADR-style: context, decision, consequences). Add one for
  every architectural or product-scope decision; never rewrite history — supersede.
- Open product questions live in `OPEN_QUESTIONS.md` of the private `ViaAurorae/llm-api-tutorial-internal`
  repo (the PRD is there too) and are referenced from code as `TODO(IT #n)` comments.
