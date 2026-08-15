# Architecture

## 1. Positioning (from the answered PRD clarifications)

| PRD question                                                                | Answer                        | Consequence                                                                                                                                           |
| --------------------------------------------------------------------------- | ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| #3 multi-provider switching needed                                          | yes                           | CC Switch is **required**, not optional                                                                                                               |
| #4 may the tool write `~/.codex/`?                                          | **no**                        | Positioning **A — onboarding companion**: we detect, install, guide the user inside CC Switch, verify, diagnose. We never own config files (ADR-0003) |
| #5 scope                                                                    | Codex CLI **and** Claude Code | `ToolId = codex \| claude-code`; both flow through the same wizard                                                                                    |
| #1 only gateway URL is company-wide                                         |                               | preset carries base URL; model / reasoning effort are user-editable hints                                                                             |
| #2 gateway supports Responses                                               |                               | default protocol `responses`; Chat Completions kept only as a diagnosis branch (fault F)                                                              |
| #6 Windows + macOS, #7 no admin, #8 non-developers, #9 zh-CN + en           |                               | Tauri 2 (ADR-0001), NSIS `currentUser` install, per-user npm prefix, plain-language i18n                                                              |
| #10 auto-route mirrors                                                      |                               | `net::choose_mirrors` probes official + npmmirror and picks by latency                                                                                |
| #13 no auto-update, #12 signing required                                    |                               | no updater plugin; signing wired in CI via secrets (`docs/RELEASE.md`)                                                                                |
| #14–#17 keys, connectivity test allowed, telemetry wanted, no env-var edits |                               | key in memory only; opt-in telemetry; env conflicts → instructions only                                                                               |
| #18 docs from designated site, #20 auto-diagnosis                           |                               | `docs/` module fetches remote Markdown with cache + bundled fallback; `diagnose/` rule engine                                                         |

## 2. Process model

```
┌──────────────────────────── Tauri app ────────────────────────────┐
│  WebView (React 19 + TS + Tailwind 4 + zustand + i18next)          │
│    features/<step>  ──invoke──▶  lib/tauri.ts  ──▶ IPC             │
│    lib/events.ts    ◀──emit────  Rust events                       │
├────────────────────────────────────────────────────────────────────┤
│  Rust core (tokio)                                                 │
│    commands.rs ─▶ checks | install | guide | verify | diagnose |    │
│                   docs | telemetry                                 │
│    shared: config · state · platform · process · net · redact      │
└────────────────────────────────────────────────────────────────────┘
        │ child processes (node/npm/codex/claude, login shell)  │ HTTPS (rustls)
        ▼                                                       ▼
   user machine (read-only except app cache/config dirs)   npm registries · GitHub · gateway · docs site · telemetry
```

Only the app's own directories are ever written: `app_cache_dir` (downloads, docs cache),
`app_config_dir` (optional preset override), `app_log_dir` (rotating log).

## 3. Wizard flow

```
welcome ─▶ env_check ─▶ install ─▶ configure ─▶ verify ─▶ done
              │            ▲          (CC Switch     │
              │            └──────────  walkthrough)  ├─ fail ─▶ diagnose (panel) ─▶ fixes ─▶ back to the failing step
              └─ blockers with FixAction (open URL / go to step / install / instructions)
```

- `env_check` runs all M1 checks concurrently and streams results (`checks://progress`).
- `install` is only shown for missing pieces; each item = plan (shown) → confirm → stream output.
- `configure` per selected tool: preset values with copy buttons + step list; steps with a
  `verifyCheck` re-run that check when the user clicks "I did this".
- `verify` = fresh-session CLI + optional gateway probe + running-terminal warning; failure
  auto-opens the diagnose panel with the rule engine's output.

## 4. IPC contract

Source of truth: `src-tauri/src/models.rs` ⇄ `src/lib/types.ts` (must change together).
Commands: `src-tauri/src/commands.rs` ⇄ `src/lib/tauri.ts`. Events: `events.rs` ⇄ `events.ts`.

Principles

- Rust returns **codes + params**, never prose → `t("<ns>:<code>", params)` on the UI.
- Errors: `AppError` → `{ code, message, params }`; UI shows `common:errors.<code>`.
- Nothing crossing IPC or leaving the process may contain an API key. `GatewayProbeRequest.apiKey`
  is the single exception (UI → Rust, one request), and it is never echoed back.

Event channels: `install://output`, `install://done`, `download://progress`, `checks://progress`.

## 5. Module responsibilities (Rust)

| module      | PRD | key functions                                                                    | notes                                               |
| ----------- | --- | -------------------------------------------------------------------------------- | --------------------------------------------------- |
| `checks`    | M1  | `run_all`, `run_one` + `os/node/tools/cc_switch/env_vars/network`                | three-state result, `FixAction`s attached           |
| `install`   | M2  | `plan`, `start`, `latest_cc_switch_release`, `download_installer`, `JobRegistry` | show-before-run; cancel via watch channel           |
| `guide`     | M3  | `build_guide`, `validate_api_key`, `preview_url`                                 | pure functions, unit-tested                         |
| `verify`    | M4  | `verify`, `probe_gateway`, `running_terminals`                                   | fresh-session env from `process::fresh_session_env` |
| `diagnose`  | M5  | `diagnose` (rules A–G), `report::build`                                          | table-driven tests per rule; report fully redacted  |
| `docs`      | M6  | `fetch_index`, `fetch_page`                                                      | remote → cache → bundled cascade                    |
| `telemetry` | #16 | `track`, `flush`, `status`                                                       | opt-in; disabled without endpoint                   |
| `config`    | —   | `load` (resource → embedded → override)                                          | `TODO(IT)` placeholders in preset                   |
| `process`   | —   | `run`, `run_streaming`, `fresh_session_env`                                      | timeouts, `CREATE_NO_WINDOW`                        |
| `net`       | —   | `probe`, `choose_mirrors`, `download`                                            | rustls, proxy-aware, SHA-256                        |
| `redact`    | —   | `redact_secrets`, `mask_value`, `tail_redacted`                                  | mandatory for anything user-visible/logged          |

## 6. Diagnosis rules (M5) — mapping to guide faults A–G

See `src-tauri/src/diagnose/mod.rs` header table. Assumed mapping (to confirm against the
original guide, `docs/OPEN_QUESTIONS.md` Q-D1): A auth (401/403) · B URL (404) · C terminal not
restarted · D env-var conflict · E command not found / PATH · F protocol mismatch · G app blocked
by OS (SmartScreen / Gatekeeper).

## 7. Telemetry payload

```json
{
  "sessionId": "uuid-per-launch",
  "appVersion": "0.1.0",
  "platform": "windows",
  "arch": "x86_64",
  "events": [
    {
      "name": "step_result",
      "step": "verify",
      "status": "fail",
      "durationMs": 1234,
      "errorClass": "auth",
      "ruleId": "A"
    }
  ]
}
```

No hostnames, usernames, URLs, paths, keys or free text. Endpoint and approval: PRD #16.

## 8. Security model

- Tauri capabilities (`src-tauri/capabilities/default.json`): only opener (http/https), dialog
  save/message, clipboard write, log. No `fs`/`shell` plugin exposure to the webview — all
  filesystem/process work happens in Rust behind validated commands.
- CSP set in `tauri.conf.json`; no remote scripts; docs Markdown rendered via react-markdown
  (no raw HTML).
- Installers: Windows NSIS per-user, macOS DMG; both signed/notarized before distribution.
