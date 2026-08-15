# Codex Onboarding · Codex 接入引导工具

Guided desktop assistant (Windows / macOS) that takes a non-developer from a clean machine to a
working **Codex CLI** and **Claude Code** setup through the company gateway configured in
**CC Switch** — environment check → install → guided configuration → verification →
automatic diagnosis — in Chinese and English.

Built with Tauri 2 (Rust) + React / TypeScript.

|                                                       |                                                    |
| ----------------------------------------------------- | -------------------------------------------------- |
| Product requirements                                  | [`docs/prd/`](docs/prd/)                           |
| Architecture & IPC contract                           | [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)     |
| Development setup & commands                          | [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md)       |
| Contribution standard (branches, commits, DoD, style) | [`CONTRIBUTING.md`](CONTRIBUTING.md)               |
| Decisions (ADRs)                                      | [`docs/adr/`](docs/adr/)                           |
| Open product questions                                | [`docs/OPEN_QUESTIONS.md`](docs/OPEN_QUESTIONS.md) |
| Release & signing                                     | [`docs/RELEASE.md`](docs/RELEASE.md)               |
| Agent guide                                           | [`CLAUDE.md`](CLAUDE.md)                           |

## Quick start

```bash
npm install
npm run tauri dev
```

Requires Node ≥ 20 and a Rust stable toolchain (see `docs/DEVELOPMENT.md`).

## What the tool never does

- write to `~/.codex`, `~/.claude` or `~/.cc-switch` (the user configures inside CC Switch)
- change environment variables
- store or upload API keys
- run a command without showing it first

Internal tool — not licensed for external distribution.
