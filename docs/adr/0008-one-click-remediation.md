# ADR-0008: One-click remediation — plan → confirm → apply, with a bounded list of machine changes

- Status: accepted
- Date: 2026-08-19
- Supersedes in part: ADR-0003 (the `~/.codex/config.toml` write and the env-var / PATH edits
  below). ADR-0003 remains in force for `~/.claude`, `~/.cc-switch` and everything not listed
  here.
- Related: ADR-0005 (secrets), ADR-0006 (CC Switch deep-link import), PRD #4, #7, #17, hard
  rules 1–4

## Context

The test team and the product owner reviewed the first pilot build (2026-08-19; screenshots
p1–p4) and asked for behaviour that ADR-0003 and the original PRD answers ruled out:

- p1 — a missing / too old Node.js only opened the download page; non-developers did not finish
  the install. Wanted: one-click download + install, from whichever source the network probe
  picks (official vs npmmirror), with the button saying it needs administrator rights.
- p2 — environment-variable conflicts (`ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN`, …) were
  reported with instructions only; users could not find the Windows dialog. Wanted: one-click
  clean-up whose confirmation shows the **full** URL and token values before anything is deleted
  (the tester explicitly rejected a masked preview: "I need to see what I am deleting").
- p3 — "Codex CLI found but broken: `'"node"' is not recognized`" and p4 — "install succeeded
  but the re-check still cannot find the command": both are PATH problems. Wanted: one-click
  PATH repair.
- The Codex `config.toml` template was shown as text to copy into CC Switch; nobody did it.
  Wanted: apply it automatically (write `~/.codex/config.toml`).
- The Codex desktop client (now shipped inside the ChatGPT desktop app) should be a check row on
  the env-check page with a download + install button; Windows users in regions where the Store
  listing is unavailable need a shortcut to the region settings.
- CC Switch semantics were clarified: a user **with** a ChatGPT account uses CC Switch's built-in
  "OpenAI Official" preset and signs in in Codex; only a user **without** an account adds a
  custom provider with the gateway URL + key. The config.toml template applies to both paths.

The PM revised the PRD answers accordingly (recorded in `docs/OPEN_QUESTIONS.md`): the tool may
write `config.toml`, may clean env vars, and admin prompts are acceptable when clearly labelled
and issued by the OS / installer.

## Decision

### Pattern: plan → confirm → apply

Every one-click action is a pair of Rust commands:

1. `plan_*` derives a DTO that describes the exact change — `display_command`, the affected
   location, whether it creates a backup, `requires_admin`, and for env-var clean-up the full
   current value of every item.
2. The UI shows the plan (dialog / inline preview), labels the button "(需要管理员权限)" /
   "(needs administrator rights)" when `requires_admin`, links to the help section `one-click`,
   and only then calls `apply_*` with the confirmed plan.
3. `apply_*` **re-derives** the plan on the Rust side and compares it with the confirmed DTO
   (rendering / program + args / values); a mismatch (tampered webview, machine state changed
   since the preview) is refused. Installer runs additionally require the file to live in the
   app's `downloads` dir (`installer::validate_run_plan`).

All of it lives in `src-tauri/src/remediate.rs` (PATH, env vars, config.toml) and
`src-tauri/src/install/installer.rs` + `install/node.rs` (running downloaded installers, Node
LTS discovery). Decision logic is pure and unit-tested; the functions that touch the machine are
thin.

### Machine changes now allowed (exhaustive)

| change                                                                                                       | how                                                                                                                                                                                                                                                                                                                                    | admin                                                 | undo                                                  |
| ------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------- | ----------------------------------------------------- |
| **User PATH** gains one directory                                                                            | Windows: append to `HKCU\Environment\Path` (type kept `REG_EXPAND_SZ`, nothing removed) + `WM_SETTINGCHANGE` broadcast; macOS/Linux: `export PATH="<dir>:$PATH"  # added by SeedRouter Onboarding` appended to the login shell's rc file (`fish_add_path` for fish), backup first                                                      | no                                                    | remove the entry / line, or restore the backup        |
| **Conflicting env vars removed** from their persistent sources (`*_API_KEY`, `*_BASE_URL`, …; proxies never) | user registry value deleted; machine registry value deleted via `reg delete` started with `Start-Process -Verb RunAs` (UAC); rc-file / PowerShell-profile line commented out as `# removed by SeedRouter Onboarding: <line>` with backup; `launchctl unsetenv`; process-only values reported as not removable                          | only the machine-registry item (UAC)                  | backups; re-add the value (shown in full in the plan) |
| **`~/.codex/config.toml`** written with the confirmed template (real key substituted at apply time)          | TOML validated; existing file copied to `config.toml.seedrouter-<timestamp>.bak`; `restore_codex_config` puts the newest backup back (and backs the live file up first)                                                                                                                                                                | no                                                    | restore button                                        |
| **Downloaded installers run**                                                                                | Node: `msiexec /i <file> /passive /norestart` (Windows Installer shows UAC; exit 3010 = reboot pending counts as success) / `osascript … installer -pkg … with administrator privileges` (macOS password dialog). Codex client: `Add-AppxPackage -Path <msix>` (per-user) / mount DMG, `ditto` the `.app` into `/Applications`, detach | Node yes (prompt from the installer); Codex client no | uninstall via the OS                                  |

Downloads keep hard rule 7: https only, SHA-256 from the same source when available (Node:
`SHASUMS256.txt`; Codex client: none published — recorded as an open point).

### What stays forbidden

- Silent elevation. This app never runs elevated itself; every admin step is an OS / installer
  prompt (`msiexec`, `installer -pkg` via `osascript`, `Start-Process -Verb RunAs`) and is
  labelled as such on the button and in the plan.
- Writing to `~/.claude/` or `~/.cc-switch/` (ADR-0003 unchanged); any file under `~/.codex/`
  other than `config.toml`.
- Editing the machine (`HKLM`) PATH, or removing anything from any PATH.
- Logging, telemetry or reports containing env-var values, the config.toml content or keys.
  The full values travel over IPC once for the dialog; `redact_secrets` / `mask_value` remain
  mandatory everywhere else.
- Running anything the user has not seen: no plan, no run.

### Codex account paths

`guide::build_guide` gains `GuideStep.branch` (`chatgpt_login` | `api_key` | null) and the Codex
steps `add_official_provider`, `login_chatgpt` (both `chatgpt_login`) and `apply_codex_config`
(both paths). The UI lets the user choose the path; Claude Code steps carry no branch. This is
a product clarification rather than a new principle, so no separate ADR.

## Consequences

- Hard rules 1–4 in `CLAUDE.md` are rewritten to name the exact exceptions; ADR-0003's status
  becomes "superseded in part by ADR-0008".
- The "what the tool never does" promises in the README, the help overview and the PRD
  positioning table are narrowed to match (see `docs/ARCHITECTURE.md` §1/§2).
- New help page `one-click` (zh-CN / en) documents every button: what it does, step by step,
  rights, undo, failure handling; env-check / install / configure / troubleshooting / FAQ pages
  cross-link it.
- Open points (`docs/OPEN_QUESTIONS.md`): Store availability of the ChatGPT/Codex app per
  region, trust of the offline MSIX (Store-signed), macOS flows untested on hardware.
- Review checklist: any new machine change must be added to the table above and to the help
  page in the same PR.
