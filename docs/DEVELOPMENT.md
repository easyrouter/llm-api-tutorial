# Development

## Prerequisites

| tool    | version                                               | notes                                       |
| ------- | ----------------------------------------------------- | ------------------------------------------- |
| Node.js | ≥ 20 (22 LTS recommended)                             | `npm` ships with it                         |
| Rust    | stable (≥ 1.85) via `rustup`                          | `rustup component add clippy rustfmt`       |
| Windows | VS 2022 Build Tools (C++ workload) + WebView2 runtime | WebView2 is preinstalled on Win 10 2004+/11 |
| macOS   | Xcode Command Line Tools (`xcode-select --install`)   |                                             |

Full list: <https://tauri.app/start/prerequisites/>.

## Setup

```bash
git clone <repo> && cd seedrouter-onboarding
npm install            # installs deps and activates .githooks (prepare script)
npm run tauri dev      # first run compiles the Rust side (a few minutes)
```

`npm run tauri dev` starts Vite on :1420 and the Tauri window with hot reload for the UI;
Rust changes trigger a rebuild.

## Everyday commands

| command                                          | what                                                    |
| ------------------------------------------------ | ------------------------------------------------------- |
| `npm run check`                                  | everything CI runs (web + rust)                         |
| `npm run lint` / `npm run lint:fix`              | ESLint                                                  |
| `npm run format` / `format:check`                | Prettier                                                |
| `npm run typecheck`                              | `tsc -b`                                                |
| `npm run test` / `test:watch`                    | Vitest (jsdom)                                          |
| `npm run i18n:check`                             | locale key parity (zh-CN is source)                     |
| `npm run rust:fmt` / `rust:clippy` / `rust:test` | Cargo equivalents                                       |
| `npm run tauri build`                            | production bundle in `src-tauri/target/release/bundle/` |

## Project conventions in one minute

- IPC contract = `src-tauri/src/models.rs` ⇄ `src/lib/types.ts`; commands = `commands.rs` ⇄
  `lib/tauri.ts`. Change both sides in one commit.
- Rust returns codes; UI translates. Every string exists in `zh-CN` **and** `en`.
- Never write to `~/.codex`, `~/.claude`, `~/.cc-switch`; never edit env vars; never persist keys.
- Show every command before running it.

## Debugging

- Logs: `tauri-plugin-log` writes to the app log dir (path shown in the app footer) and to the
  webview console in dev.
- Rust: `RUST_LOG=debug npm run tauri dev` (log plugin level is Info by default; adjust in
  `lib.rs` for local debugging only).
- Config override for local testing without touching the bundled preset:
  put an `app-config.json` in the app config dir (`%APPDATA%/com.seedrouter.onboarding/` on
  Windows, `~/Library/Application Support/com.seedrouter.onboarding/` on macOS).

## Testing strategy

| layer                                                                                | how                                                                                                             |
| ------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------- |
| Rust pure logic (guide rules, diagnosis rules, redaction, config parsing, URL rules) | unit tests, table-driven                                                                                        |
| Rust process/network                                                                 | thin, exercised manually via the app + smoke tests using `node --version` when Node is present (skip otherwise) |
| UI components / stores                                                               | Vitest + Testing Library; `lib/tauri.ts` mocked with `vi.mock`                                                  |
| End-to-end                                                                           | manual pilot checklist in `docs/RELEASE.md`; fault injection for A–G                                            |
