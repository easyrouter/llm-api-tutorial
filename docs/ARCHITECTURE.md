# Architecture

## 1. Positioning (from the answered PRD clarifications)

| PRD question                                                                | Answer                                         | Consequence                                                                                                                                                                                                                                                       |
| --------------------------------------------------------------------------- | ---------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| #3 multi-provider switching needed                                          | yes                                            | CC Switch is **required**, not optional                                                                                                                                                                                                                           |
| #4 may the tool write `~/.codex/`?                                          | **no**, revised 2026-08-19: `config.toml` only | Positioning **A — onboarding companion**: we detect, install, guide the user inside CC Switch, verify, diagnose. We never own config files (ADR-0003) — except `~/.codex/config.toml`, written on explicit click with backup (ADR-0008)                           |
| #5 scope                                                                    | Codex CLI **and** Claude Code                  | `ToolId = codex \| claude-code`; both flow through the same wizard                                                                                                                                                                                                |
| #1 only gateway URL is company-wide                                         |                                                | preset carries base URL; model / reasoning effort are user-editable hints. Revised 2026-08-24: the preset is resolved **per tool** (`config::gateway_defaults`) — `gateway.claudeCode` overrides address + model for Claude Code, whose base URL is the site root |
| #2 gateway supports Responses                                               |                                                | default protocol `responses`; Chat Completions kept only as a diagnosis branch (fault F)                                                                                                                                                                          |
| #6 Windows + macOS, #7 no admin, #8 non-developers, #9 zh-CN + en           | #7 revised 2026-08-19                          | Tauri 2 (ADR-0001), NSIS `currentUser` install, per-user npm prefix, plain-language i18n; admin prompts acceptable when labelled and issued by the OS / installer (Node MSI/pkg, `reg delete` via UAC — ADR-0008)                                                 |
| #10 auto-route mirrors                                                      |                                                | `net::choose_mirrors` probes official + npmmirror and picks by latency                                                                                                                                                                                            |
| #13 no auto-update, #12 signing required                                    |                                                | no updater plugin; signing wired in CI via secrets (`docs/RELEASE.md`)                                                                                                                                                                                            |
| #14–#17 keys, connectivity test allowed, telemetry wanted, no env-var edits | #17 revised 2026-08-19                         | key in memory only; opt-in telemetry; env conflicts → one-click clean-up (plan shows full values → confirm → apply; `remediate`, ADR-0008) plus the manual instructions                                                                                           |
| #18 docs from designated site, #20 auto-diagnosis                           |                                                | `docs/` module fetches remote Markdown with cache + bundled fallback; `diagnose/` rule engine                                                                                                                                                                     |

## 2. Process model

```
┌──────────────────────────── Tauri app ────────────────────────────┐
│  WebView (React 19 + TS + Tailwind 4 + zustand + i18next)          │
│    features/<step>  ──invoke──▶  lib/tauri.ts  ──▶ IPC             │
│    lib/events.ts    ◀──emit────  Rust events                       │
├────────────────────────────────────────────────────────────────────┤
│  Rust core (tokio)                                                 │
│    commands.rs ─▶ checks | install | guide | verify | diagnose |    │
│                   docs | telemetry | remediate                     │
│    shared: config · state · platform · process · net · redact      │
└────────────────────────────────────────────────────────────────────┘
        │ child processes (node/npm/codex/claude, login shell)  │ HTTPS (rustls)
        ▼                                                       ▼
   user machine (read-only except app dirs + ADR-0008 list)  npm registries · GitHub · gateway · docs site · Node dist · telemetry
```

Without user action only the app's own directories are written: `app_cache_dir` (downloads,
docs cache), `app_config_dir` (optional preset override), `app_log_dir` (rotating log). After an
explicit plan → confirm → apply (ADR-0008, `remediate.rs` / `install/installer.rs`) the app may
additionally: append one directory to the **user** PATH (`HKCU\Environment\Path` + broadcast, or
an `export PATH=…` line in the login-shell rc file with backup); remove conflicting env vars
(user registry; machine registry via elevated `reg delete`; rc-file / PowerShell-profile line
commented out with backup; `launchctl unsetenv`); write `~/.codex/config.toml` (backup
`config.toml.seedrouter-<ts>.bak`, restorable); run a downloaded installer from the `downloads`
dir (Node MSI/pkg with the installer's own UAC / password prompt; Codex client MSIX per-user /
DMG copied into `/Applications`). Never: `~/.claude`, `~/.cc-switch`, the machine PATH, silent
elevation.

## 3. Wizard flow

```
welcome ─▶ env_check ─▶ install ─▶ configure ─▶ verify ─▶ done
              │            ▲          (CC Switch     │
              │            └──────────  walkthrough)  ├─ fail ─▶ diagnose (panel) ─▶ fixes ─▶ back to the failing step
              └─ blockers with FixAction (open URL / go to step / install / instructions /
                 repair_path / clean_env_vars / open_system_uri — the last three are one-click
                 plan → confirm → apply, ADR-0008)
```

- `env_check` runs all M1 checks concurrently and streams results (`checks://progress`).
- `install` is only shown for missing pieces; each item = plan (shown) → confirm → stream output.
- `configure` per selected tool: preset values with copy buttons + step list; steps with a
  `verifyCheck` re-run that check when the user clicks "I did this". Codex has two account
  paths (`GuideStep.branch`): `chatgpt_login` (CC Switch "OpenAI Official" preset + `codex`
  sign-in) or `api_key` (custom provider); both end with `apply_codex_config`, which writes
  `~/.codex/config.toml` through `remediate` (backup + restore).
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

One-click DTOs (`PathRepairPlan`, `EnvCleanupPlan`/`EnvCleanupItem`, `CodexConfigStatus`,
installer-run `InstallPlan`s) are always produced by a `plan_*` command, shown by the UI, and
handed back unchanged to the matching `apply_*`/`start_install` command, which re-derives and
compares before acting. `EnvCleanupItem.value` is the **full** value (dialog only — never logged).

## 5. Module responsibilities (Rust)

| module      | PRD | key functions                                                                                                                                     | notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| ----------- | --- | ------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `checks`    | M1  | `run_all`, `run_one` + `os/node/tools/cc_switch/codex_app/env_vars/network`                                                                       | three-state `Verdict` (code + params + details + `FixAction`s); streamed on `checks://progress`; codes listed in the `checks/mod.rs` header; `codex_app` (ChatGPT / Codex desktop app, Warn when missing → `install{codex-app}`, Store page + region settings on Windows, download page); `tool.broken` with `nodeMissing` / `node.not_on_path` / `tool.not_on_path` / `env_vars.conflicts` emit the one-click `repair_path` / `clean_env_vars` fixes first                                                                                                                                                                                                                                                                                                                                                                           |
| `install`   | M2  | `plan`, `start`, `fetch_installer_release`, `download_installer`, `plan_installer_run`, `JobRegistry`                                             | show-before-run (`start` re-validates the plan against the preset — `plan::validate_plan`; installer runs against `installer::validate_run_plan` + downloads-dir containment); `plan_install(target, excludeRegistry)` re-plans on another mirror after a failed job; `InstallPlan.explanationCode` ∈ `node.download_page` / `node.homebrew` / `npm_global` / `cc_switch.download` / `codex_app.download` / `node.msi` / `node.pkg` / `codex_app.msix` / `codex_app.dmg`; `downloadUrl` for manual plans, `installerPath` + `requiresAdmin` for installer runs; `fetch_installer_release(target)` → cc-switch (GitHub / intranet), node (`install/node.rs`: newest LTS from the probed dist mirror, SHA-256 from `SHASUMS256.txt`), codex-app (static URLs from `codexApp`); msiexec 3010 counts as success; cancel via watch channel |
| `remediate` | —   | `plan_path_repair`/`apply_path_repair`, `plan_env_cleanup`/`apply_env_cleanup`, `codex_config_status`/`apply_codex_config`/`restore_codex_config` | ADR-0008 one-click changes; each `apply` re-derives the plan and refuses on mismatch; user PATH only (`HKCU\Environment\Path` + `WM_SETTINGCHANGE`, or rc-file `export` line with backup); env-var removal per source (user registry, machine registry via `Start-Process -Verb RunAs reg delete`, rc line commented out with backup, `launchctl unsetenv`, process-only = not removable); `~/.codex/config.toml` with `.seedrouter-<ts>.bak` backups; pure helpers unit-tested                                                                                                                                                                                                                                                                                                                                                       |
| `guide`     | M3  | `build_guide`, `validate_api_key`, `preview_url`, `codex_config_template` / `codex_config_template_response`                                      | pure functions, unit-tested; step codes = step ids (`guide:<id>.title` / `guide:<id>.body`); `GuideStep.branch` (`chatgpt_login` / `api_key` / null) selects the Codex account path; template carries `<API-KEY>` placeholder, substituted by the UI at apply/copy time; `get_codex_config_template` returns `CodexConfigTemplate` (toml + `model_context_window` / `model_auto_compact_token_limit`) so the UI quotes the limits instead of hard-coding them                                                                                                                                                                                                                                                                                                                                                                         |
| `verify`    | M4  | `verify`, `probe_gateway`, `list_models`, `running_terminals`                                                                                     | fresh-session env from `process::fresh_session_env`; CLI check + gateway probe (Responses / Chat Completions for Codex, Anthropic Messages for Claude Code; `https` only) + terminal scan run concurrently; symptoms handed to `diagnose`; the configure screen's `test_connectivity` runs `list_models` next to the probe, and both use `net::gateway_client(GATEWAY_TIMEOUT)` — 45 s, because a reasoning model needs 20–30 s to answer even a "ping"                                                                                                                                                                                                                                                                                                                                                                               |
| `diagnose`  | M5  | `diagnose` (rules A–G, NET, GW), `report::build`                                                                                                  | table-driven tests per rule; snapshot-only diagnosis (D / E) also offered from the env-check screen; fault G reachable from the CC Switch download; report fully redacted (incl. a `Verification` section with the latest verify results), read-only inspection of tool config dirs                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `docs`      | M6  | `fetch_index`, `fetch_page`                                                                                                                       | remote → cache → bundled cascade                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `telemetry` | #16 | `track`, `flush`, `status`                                                                                                                        | opt-in; disabled without endpoint                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `config`    | —   | `load` (resource → embedded → override)                                                                                                           | `TODO(IT)` placeholders in preset                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `process`   | —   | `run`, `run_streaming`, `fresh_session_env`                                                                                                       | timeouts, `CREATE_NO_WINDOW`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `net`       | —   | `probe`, `choose_mirrors`, `download`                                                                                                             | rustls, proxy-aware, SHA-256                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `redact`    | —   | `redact_secrets`, `mask_value`, `tail_redacted`                                                                                                   | mandatory for anything user-visible/logged                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |

### 5.0 Codes ↔ i18n

Every code the Rust core emits (`CheckResult.code`, `Diagnosis.code` + checklist keys,
`FixAction::Instructions.code` (fully qualified, e.g. `checks:env_vars.instructions.windows`),
`InstallPlan.explanationCode` → `install:plan.<code>`, `GuideStep.code` → `guide:<code>.title|body`,
`FixAction` kinds incl. `repair_path` / `clean_env_vars` / `open_system_uri` → `common:fixes.*`,
`AppError.code` → `common:errors.<code>`, wire enums `KeyIssue` / `UrlRule` / `UrlWarning` /
`ErrorClass`) must have a translation in `src/i18n/locales/zh-CN` (and, by parity, `en`).
`scripts/check-codes.mjs` extracts these literals from `src-tauri/src` and fails `npm run
i18n:check` when one is missing; exceptions go into `scripts/check-codes.allowlist.json`.

### 5.0.1 CC Switch distribution contract (M2, Q-CC1)

`install::latest_cc_switch_release` prefers the intranet mirror when `ccSwitch.intranetMirror`
is set: `GET <intranetMirror>/latest.json` →
`{ "version": "3.2.1", "assets": [ { "name": "<GitHub-style asset name>", "url": "https://…", "sha256": "<hex, optional>" } ] }`.
Otherwise the GitHub releases API (`ccSwitch.releasesApi`) is used, with the SHA-256 taken
from `assets[].digest`, a sibling `<asset>.sha256`, or `SHA256SUMS*`. Downloads land in
`<app-cache-dir>/downloads` and are verified when a hash is known (`DownloadResult.verified`).

### 5.1 Help docs contract (M6, PRD #18)

The designated documentation site (`docs.baseUrl` in `app-config.json`) must publish, for
`lang` in `{zh-CN, en}`: `{baseUrl}/{lang}/index.json` (section list: `id`, `title`, `path`,
`lang`, optional `wizardStep`, `children`) and `{baseUrl}/{lang}/<path>.md` (Markdown, UTF-8).
Resolution cascade per document: remote (10 s timeout, 60 s backoff after a transport failure)
→ cache `<app-cache-dir>/docs/{lang}/{path}` (fresh within `docs.cacheTtlSeconds`, stale copy
served when the remote fails) → bundled `resources/docs/{lang}/{path}` (also embedded in the
binary, so `tauri dev` works without a resource dir). Sections with unsafe `id`/`path` are
dropped; page bodies and titles pass through `redact_secrets`. Wizard step → section id:
`welcome`→`overview`, `env_check`→`env-check`, `install`→`install-node` / `install-cli` /
`install-cc-switch`, `configure`→`configure-cc-switch`, `verify`→`verify`,
`diagnose`→`troubleshooting`; `faq` has no step; `one-click` (also `env_check`) is opened by
every one-click button via `openHelp("one-click")`. Markdown links `wizard://<step>` /
`#step:<step>` jump the wizard, `http(s)` opens externally, a section id opens another topic
(`src/features/help/docs-tree.ts`). Full field rules: `src-tauri/resources/docs/README.md` and
the `docs/mod.rs` module doc.

## 6. Diagnosis rules (M5) — mapping to guide faults A–G

See `src-tauri/src/diagnose/mod.rs` header table. Assumed mapping (to confirm against the
original guide, `OPEN_QUESTIONS.md` Q-D1 in the internal repo): A auth (401/403) · B URL (404) · C terminal not
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
