# Changelog

All notable changes to this project are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning: [SemVer](https://semver.org/).

## [Unreleased]

### Changed (open-sourcing)

- Licensed under **Apache-2.0** (`LICENSE`, `NOTICE`, `license` fields in `package.json` and
  `Cargo.toml`); the README now names SeedRouter as sponsor instead of calling the tool
  internal-only.
- The PRD (`docs/prd/`) and `docs/OPEN_QUESTIONS.md` moved to the private
  `ViaAurorae/llm-api-tutorial-internal` repo; every reference in docs, comments and
  `app-config.json` points there. Prepared for the transfer of this repo to `easyrouter`.

## [0.1.1] - 2026-09-12

First release after v0.1.0: GPT-6 Astra becomes the Codex preset model, the bundled Codex
Fast UI toolkit is re-pinned to the current Codex desktop builds, and its Verify / Restore
actions work for the first time. Built unsigned (Q-12), so published as a pre-release.

### Changed (gateway preset)

- **The Codex preset model is `gpt-6-astra`** (GPT-6 Astra, released 2026-09-03/04; Codex CLI
  0.153.4 made it its bundled default). The company gateway lists it in `GET /models` and
  answers `POST /responses` for it, so the wizard now suggests it everywhere the Codex preset
  shows up — the values card, the `set_model` step, the connectivity test and the `config.toml`
  template (`app-config.json` → `gateway.defaultModel`). `gpt-5.6-sol` stays selectable; nothing
  else in the preset changed and every value remains user-editable (Q-M3 in
  `docs/OPEN_QUESTIONS.md`; Claude Code keeps `claude-sonnet-5`).
- The template's `model_context_window = 372000` / `model_auto_compact_token_limit = 300000`
  are unchanged on purpose: Codex's bundled model catalog lists `gpt-6-astra` and `gpt-5.6-sol`
  with identical `context_window` (272000) / `max_context_window` (872000), so IT's numbers
  (Q-M1) apply to both models.
- `get_codex_config_template` returns a `CodexConfigTemplate` DTO (`toml` + `modelContextWindow`
  + `modelAutoCompactTokenLimit`) instead of a bare string, and the config card quotes those
  numbers — and the live model name — through parametrised strings: `guide:config.description`
  (`{{model}}`, with the new `guide:config.modelFallback` when the field is blank),
  `guide:config.scopeHint` (`{{limit}}`) and
  `guide:config.scope.body_after_prefix.explanation` (`{{contextWindow}}`, rendered as `372k`;
  a value named `context` would double as i18next's context option). Before
  the first template response the two strings that quote a number are left out rather than
  showing a placeholder. The copy previously hard-coded `gpt-5.6-sol`, `300000` and `372k`,
  which is how a model change could have left the text stale without any test noticing.

### Changed (Codex Fast UI toolkit)

- **Re-pinned the bundled toolkit to the current Codex builds.** `CodexFastUI-Minimal-2026.09.12.zip`
  (`2026.09.12-minimal`, rebuilt from the 2026.08.19 tree) replaces the 2026.08.19 archive;
  `TOOLKIT_ARCHIVE` / `TOOLKIT_DIR_NAME` / `TOOLKIT_VERSION` / `TOOLKIT_SHA256` follow, and
  `TESTED_CODEX_BUILD` — shown verbatim in the card's caveat — now reads
  `OpenAI.Codex 26.903.8094.0 / 26.908.4834.0`: the offline MSIX `ChatGPT-x64.msix` (Electron
  26.903.61454, build 8378) and the Microsoft Store / auto-updated client (26.908.40834, build
  8881). The patch itself is unchanged — the gate string still occurs exactly once, in
  `webview/assets/app-initial-<hash>.js` of both builds, with no patched marker present. Because
  the consumers of `isServiceTierAllowed` moved into `app-primary-<hash>.js`, `patch-fast-ui.mjs`
  adds `app-primary` to its candidate file-name regex; that is a defensive widening only, the
  exactly-one-occurrence rule stays the guard. Verified on a Windows 10 host from the built
  toolkit: `install.ps1` reports `patchStatus: patched` for both builds, `verify.ps1` passes on
  both copies, and `restore.ps1` brings `app.asar` back to the recorded source hash. ADR-0007
  records the pin history (26.721 → 26.814 → 26.903 / 26.908).

### Fixed (Codex Fast UI toolkit)

- **`verify.ps1` / `restore.ps1` failed before doing anything when launched the way the app
  launches them.** Both derived their default `-Root` from `$MyInvocation.MyCommand.Path` inside a
  `[CmdletBinding()]` `param()` default, which Windows PowerShell 5.1 leaves null there (as it
  does `$PSScriptRoot`), so `powershell.exe -File verify.ps1` without `-Root` — exactly what
  `fast_ui::command_for` runs for Verify and Restore — died in `Split-Path` with "Cannot bind
  argument to parameter 'Path' because it is null". Present since `2026.07.30-minimal`, i.e. the
  "Check the copy" and "Undo the patch" buttons never worked. The scripts now default `-Root` to
  `''` and fall back to `$PSScriptRoot` in the body; an explicit `-Root` behaves as before.
- **The app no longer runs the `verify.ps1` / `restore.ps1` copies that `install.ps1` left in the
  install root.** Those copies belong to whichever toolkit made the root — on every pilot machine
  that installed with the 2026.07.30 or 2026.08.19 toolkit they are the broken ones above, and
  re-pinning the archive alone would not have reached them. `fast_ui::command_for` now runs all
  three actions from the freshly extracted, hash-verified toolkit and passes `-Root
  <install root>` to Verify / Restore (both scripts honour it). `start` refuses Verify / Restore
  when the install marker is gone, as before. A new test pins the toolkit's top-level directory
  name to the shipped archive, next to the SHA-256 test.

## [0.1.0] - 2026-08-24

First stable release. Everything below shipped through the `v0.1.0-test.1` … `v0.1.0-test.7`
pilot rounds and is collected here.

### Added (branding)

- The header carries a `seedrouter.net` link next to the version (opened in the system browser
  through `open_external`, like every other link in the app), and the footer shows the
  copyright notice `© <year> seedrouter.net` in place of the developer-facing "config source"
  line.
- The SeedRouter logo is the application icon: installer, executable, window and taskbar,
  Windows Store tiles and the macOS `.icns`, plus the app header and the webview favicon. The
  source lives at `app-icon.png`; regenerate the set with `npx tauri icon app-icon.png`
  (the Android / iOS output it also writes is git-ignored — this app ships Windows + macOS).

### Added (one-click remediation, ADR-0008)

- One-click **Node.js install** from the env-check / install step: the network probe picks the
  Node dist source (official vs npmmirror), `install/node.rs` resolves the newest LTS
  (`index.json` → `node-v<X>-x64|arm64.msi` / `node-v<X>.pkg`) with its SHA-256 from the same
  source's `SHASUMS256.txt`; `plan_installer_run` + `start_install` run `msiexec /i … /passive
  /norestart` (UAC from Windows Installer, exit 3010 = reboot pending) or `installer -pkg` via
  `osascript … with administrator privileges`. Buttons that elevate say so.
- One-click **PATH repair** (`remediate::plan_path_repair` / `apply_path_repair`): appends the
  Node dir or npm global bin dir to the **user** PATH (`HKCU\Environment\Path` kept
  `REG_EXPAND_SZ` + `WM_SETTINGCHANGE`; macOS `export PATH=…` line in the login-shell rc file,
  backup first). Offered first on `node.not_on_path`, `tool.not_on_path`, `tool.broken` with
  `nodeMissing` (`'"node"' is not recognized`) and diagnose rule E.
- One-click **env-var clean-up** (`plan_env_cleanup` / `apply_env_cleanup`): per-source removal
  (user registry; machine registry via elevated `reg delete`; rc-file / PowerShell-profile line
  commented out as `# removed by SeedRouter Onboarding: …` with backup; `launchctl unsetenv`;
  process-only values reported as not removable). The confirmation dialog shows the **full**
  values on purpose; they are never logged or sent.
- **Codex `config.toml` auto-apply** (`codex_config_status` / `apply_codex_config` /
  `restore_codex_config`): writes `~/.codex/config.toml` with the real key substituted at apply
  time, after backing the existing file up as `config.toml.seedrouter-<ts>.bak`; one-click
  restore of the newest backup.
- New check `codex_app` (Codex desktop client, now inside the ChatGPT app; Warn when missing)
  with fixes: install via the wizard (Store-signed MSIX `Add-AppxPackage` per user / DMG copied
  into `/Applications`), open the Microsoft Store page (`9PLM9XGG6VKS`), open Windows region
  settings (`ms-settings:regionformatting`) when the Store says "not available in your region",
  open the download page. `AppConfig.codexApp`, `InstallTarget = codex-app`,
  `fetchInstallerRelease(target)` (replaces `fetchCcSwitchRelease`), `FixAction` kinds
  `repair_path` / `clean_env_vars` / `open_system_uri`.
- Codex account paths in the configure step: `GuideStep.branch` (`chatgpt_login` → CC Switch
  "OpenAI Official" preset + `codex` sign-in; `api_key` → custom provider), new steps
  `add_official_provider`, `apply_codex_config`.
- Help: new bilingual page `one-click` (what every button does step by step, rights, undo,
  failure handling); env-check / install-node / install-cli / configure-cc-switch /
  troubleshooting / FAQ / overview pages updated; ADR-0008; ADR-0003 superseded in part;
  `CLAUDE.md` hard rules 1–4, `docs/ARCHITECTURE.md`, `docs/OPEN_QUESTIONS.md` (PRD #4/#7/#17
  revised 2026-08-19; Q-OC1–Q-OC3) and `README.md` aligned.

### Added (per-tool gateway preset)

- **Claude Code has its own gateway defaults** (`app-config.json` → `gateway.claudeCode`):
  model `claude-sonnet-5` and address `https://seedrouter.net` — the **root**, because Claude
  Code appends `/v1/messages` to `ANTHROPIC_BASE_URL` itself, so the Codex address
  (`…/v1`) would produce `…/v1/v1/messages` → 404 (guide fault B). Codex keeps
  `https://seedrouter.net/v1` + `gpt-5.6-sol`. Both remain user-editable everywhere.
  Resolved through `config::gateway_defaults` (Rust) / `gatewayDefaults` (`src/lib/gateway.ts`);
  an empty override field falls back to the shared value, so an intranet override that replaces
  the whole `gateway` object keeps working — the address falls back to the shared one **minus a
  trailing `/v1`** (`config::anthropic_root_of`), never to an address that would 404. New DTO
  field `GatewayPreset.claudeCode`.
- The URL rules are protocol-aware: for `anthropic_messages` a bare origin stays the root
  (new rule `root_kept` instead of `appended_v1`), and an address ending in `/v1` gets the new
  `anthropic_v1_suffix` warning. This applies to the configure step's connectivity test, the
  one-click CC Switch import endpoint and the verify card. `preview_effective_url` now takes a
  `tool`.
- Diagnosis rules follow the tool: rule B's `expected_base_url` is that tool's address (and the
  `v1_suffix` checklist explains both rules instead of only the Codex one); rule F reports the
  protocol the tool actually speaks — for Claude Code `anthropic_messages` with the new
  `check_anthropic_base_url` checklist step, since it has no protocol setting in CC Switch (it
  previously always said "Responses"). The diagnostic report prints both gateway addresses.
- Offline help (`resources/docs/{en,zh-CN}/troubleshooting.md`, `configure-cc-switch.md`) states
  the address rule per tool, including the `…/v1/v1/messages` → 404 failure mode.

### Changed (one-click remediation)

- The "never writes `~/.codex`, never edits env vars, never needs admin" promises are narrowed:
  `~/.codex/config.toml` (explicit click, backup), user PATH append and env-var removal are
  allowed through plan → confirm → apply only; admin steps are labelled and prompted by the OS /
  installer; `~/.claude`, `~/.cc-switch` and the machine PATH remain untouchable.
- `login_chatgpt` is no longer "optional" — it is the `chatgpt_login` path; the config.toml
  template is applied by the tool instead of being pasted into CC Switch.

### Fixed (VM regression round 3)

- **The connectivity test declared a healthy gateway unreachable.** `POST /v1/responses` needs
  ~27 s on the company gateway to answer even a one-token "ping" — a reasoning model thinks
  before the first byte arrives — while `verify::GATEWAY_TIMEOUT` allowed 15 s. Users who had
  just pasted a working key were told the gateway could not be reached. Raised to 45 s. Raising
  the constant alone would not have been enough: the shared client sets `read_timeout(30 s)`,
  and reqwest applies the read timeout to the wait for the *response headers* too, so the probe
  would have stayed capped at 30 s. `net::gateway_client()` now builds a client whose read
  timeout equals its overall timeout; `AppState` keeps it as `http_gateway` and the three
  gateway entry points use it, so downloads and reachability probes keep their 30 s stall
  detection. A unit test asserts the two constants stay in that order.
- **The connectivity test now also fetches the model list.** `GET {base}/models` runs
  concurrently with the protocol probe, so the test costs no more than the slower of the two. It
  answers in about a second and proves address + key on its own — the verdict a user needs when
  the probe is merely slow, or when the model name is simply wrong. `ConnectivityReport.models`
  carries it, the UI shows it as its own section above the probe, and an info hint points at the
  model name / protocol when the list succeeds but the probe does not.
- **A slow `Get-AppxPackage` made the Codex desktop client look uninstalled.** The probe ran
  with a 30 s timeout; right after the wizard installs the 745 MB Codex MSIX a cold package
  cache regularly needs longer, and a timed-out probe is indistinguishable from "not
  installed" — the Fast UI action then refused to run. Raised to 90 s, and the two copies of
  the probe collapsed into one (`checks::codex_app::windows_appx`) so there is a single
  timeout to keep in sync.
- **`Unsupported` errors lost the reason they carried.** `AppError::Unsupported` names *why*
  something is unavailable with a fully qualified i18n key, but `params()` dropped it, so all
  of them reached the user as the generic "Not supported on this platform." The key now
  travels as `params.reason` and `describeError` prefers it over `errors.<code>`.
- **The bundled Fast UI toolkit could not patch the current Codex build.** `patch-fast-ui.mjs`
  listed `webview/assets` with `spawnSync(rg, ["--files", …])` and no `maxBuffer`, so Node's
  1 MB default applied. On `OpenAI.Codex 26.814.5167.0` that directory holds 7,598 files and
  `install.ps1` runs from a long temp path, which puts the listing at ~1.11 MB: `spawnSync`
  aborted with `ENOBUFS` before the Fast UI gate was ever inspected. Rebuilt as
  `2026.08.19-minimal` with an explicit `maxBuffer` and an error message that reports
  `error.code` (the old one printed `status`, which is `null` in exactly this case). The patch
  logic is unchanged — the gate still occurs exactly once in this build — and
  `TESTED_CODEX_BUILD` now names the build the toolkit was verified against end to end.
- **The toolkit archive was no longer declared in `bundle.resources`**, so a clean build would
  have shipped the feature without it while local builds kept working from a stale staged copy.
  Restored, and two tests now guard the packaging: the archive must be declared, and the file
  on disk must match `TOOLKIT_SHA256`.

### Fixed (VM regression rounds 1-2)

- **Node.js one-click install could never start.** `install::downloaded_file_in` returned the
  canonical path, which on Windows always carries the verbatim `\?\` prefix; `msiexec` rejects
  it with "This installation package could not be opened" before Windows Installer ever asks for
  elevation, so the UAC prompt never appeared either. The path now leaves the module in its
  ordinary spelling (`\?\UNC\srv\share` becomes `\srv\share`; volume-GUID paths are left
  alone, having no ordinary form).
- **Every `open_url` fix button rendered the raw key `fixes.labels.undefined`.**
  `#[serde(rename_all = …)]` renames enum *variants*, not the fields inside them, so
  `FixAction::OpenUrl` reached the webview carrying `label_code` while `src/lib/types.ts` reads
  `labelCode`. Both internally tagged DTO enums now also carry
  `rename_all_fields = "camelCase"`.
- **`diagnose` rejected any request containing a `command_failed` symptom** — the same mismatch
  in the other direction: the UI sends `outputTail`, `Symptom::CommandFailed` expected
  `output_tail`, and the whole call failed as soon as a CLI failure was reported.
- **A declined UAC prompt left the installer card silent.** `ErrorBanner` renders nothing
  without a `WireError`, which is exactly what an installer that merely exits non-zero produces
  (`msiexec` 1602/1625), so the already-written explanation and the retry button both
  disappeared and only an app restart got the one-click install back. New shared
  `JobFailureAlert` falls back to a plain danger alert carrying the same headline and action.
- **The first Codex CLI install after Node always failed with `command_not_found: npm`.** The
  install step plans every item once on mount, on a clean machine before the Node MSI has run,
  so `install::resolve_npm` fell back to the bare program name and nothing re-derived the plan
  when Node appeared. A passing Node re-check now re-plans every npm target still waiting for
  confirmation.

### Added

- Project scaffold: Tauri 2 + React 19 + TypeScript + Vite + Tailwind 4; Rust core module
  skeleton for M1–M6; IPC contract (`models.rs` / `types.ts`); company preset config; bilingual
  i18n (zh-CN / en) with parity check; wizard shell (stepper, welcome screen); CI (lint /
  typecheck / test on web; fmt / clippy / test on Windows + macOS; bundle job); git hooks
  (Conventional Commits, pre-commit gate); development standards (`CLAUDE.md`,
  `CONTRIBUTING.md`, `docs/`).
- Shared UI kit (`src/components/ui`): StatusBadge, Alert, CopyField, LogView, ProgressBar,
  Spinner, ConfirmDialog, KeyValueList, FixActionButtons, StepFooter, ErrorBanner,
  ExternalLink; helpers `lib/format`, `lib/errors`, `lib/useAsync`, `lib/clipboard`, hooks
  `useCopy` / `useStickToBottom`; Tauri test doubles (`src/test/mocks/tauri.ts`) installed
  globally by the vitest setup.
- Core services: `process` (timeouts, cancellation, fresh-session env, no console window),
  `net` (probes, mirror choice, https + SHA-256 verified downloads), `platform` (OS / PATH /
  npm prefix introspection) and `config` (resource → embedded → override load order).
- M6 help docs: remote → cache → bundled cascade with bilingual bundled pages
  (`src-tauri/resources/docs/{zh-CN,en}`), shipped as bundle resources; telemetry batched
  opt-in flush with a periodic background flush started at app setup.
- M4 verification (`verify`): fresh-session `<cli> --version` check (guide fault E aware),
  minimal gateway probe per protocol with status → error-class mapping and key-scrubbed
  messages, running-terminal detection (Windows / macOS bundles, own process tree excluded),
  symptoms fed into the M5 rule engine.
- M1 environment checks (`checks`): OS / Node / npm / Codex / Claude Code / CC Switch /
  env-var conflicts / network probes run concurrently and streamed over `checks://progress`;
  three-state verdicts with `FixAction`s (install, open URL, instructions, rerun); "installed
  but not on PATH" and "broken binary" distinguished from "missing".
- M2 install (`install`): show-before-run `InstallPlan`s (npm global installs with the chosen
  registry mirror and per-user-prefix admin detection, Node download page / Homebrew, CC Switch
  release lookup from GitHub or an intranet `latest.json`), streamed `npm install -g` jobs
  with cancel, SHA-256 verified installer downloads. `InstallPlan.downloadUrl` added to the
  IPC contract.
- M3 guide (`guide`): ordered CC Switch walkthrough steps per tool with copyable preset
  values, API-key format validation (never stored) and the URL rule preview (Q-U1).
- M5 diagnose (`diagnose`): rule engine A–G plus network / upstream rules with table-driven
  tests, platform-specific instructions, and the fully redacted Markdown diagnostic report
  (read-only inspection of `~/.codex` / `~/.claude` / CC Switch data dirs, names only).
- UI: Environment check screen (streamed rows, per-row re-check, install fixes, continue-anyway
  gate, snapshot-based diagnosis panel), Install screen (plan confirmation, live output, mirror
  retry, Node download flow, CC Switch release download with integrity badge), Configure
  walkthrough (preset values, key / URL validation cards, per-tool step list), Verify screen
  with diagnosis panel and report copy / export, Help panel (docs tree, Markdown viewer,
  wizard links) and Done screen.
- Tooling: `scripts/check-codes.mjs` (part of `npm run i18n:check`) verifies that every code
  the Rust core emits has a `zh-CN` translation.

### Fixed (review wave 2)

- Verify: the gateway probe for Claude Code now speaks Anthropic Messages
  (`POST {base}/v1/messages`, `x-api-key` + `anthropic-version`; `Protocol::AnthropicMessages`
  added to the IPC contract) instead of the OpenAI Responses shape; a non-`https` base URL is
  refused before the key is sent (`ErrorClass::NotHttps`, inline warning in the UI); the
  diagnostic report gains a `Verification` section (`build_diagnostic_report(…, verify)`).
- Install: "Switch mirror and retry" really switches — the retry re-plans without the registry
  the job failed on (`plan_install(target, excludeRegistry)`); `start_install` re-validates the
  plan server-side against the preset (program, arguments, package, registry, no env); `npm`
  is resolved on the fresh-session `PATH` like the checks do; Homebrew proposes `brew install
  node` (linked into `PATH`) instead of the keg-only `node@<major>`; installer downloads use an
  `https_only` client and refuse downgraded redirects; the Install step reports `step_result`
  telemetry; wizard navigation is locked while a job / download runs; the CC Switch download
  offers "The installer was blocked" (guide fault G) inline.
- Core: `open_downloaded_file` canonicalises and requires a file directly inside the downloads
  dir; `save_diagnostic_report` refuses paths inside `~/.codex` / `~/.claude` / `~/.cc-switch`;
  rule D no longer counts proxy variables as conflicts; Unix children run in their own process
  group and the group is killed on timeout / cancel; Windows scans PowerShell profiles for
  `$env:NAME =` / `SetEnvironmentVariable`; check details carry raw facts only (no English
  wording).
- UI: `diagnose` navigation targets land on Verify (Next/Back no longer jump to Welcome);
  re-requesting the same help section works; "Start over" also resets the install store; the
  env-check diagnosis is cleared on re-run; Done / guide copy corrected.

### Changed

- Branding: `bundle.publisher` is set to `SeedRouter`, so the installer, the executable's
  version info (CompanyName) and the Windows "Apps & features" entry all show `SeedRouter`
  instead of `company` (which Tauri derived from the identifier's second segment).
- Bundle identifier: `com.company.seedrouter-onboarding` -> `com.seedrouter.onboarding`. This
  moves the per-user config and log directories (`%APPDATA%/com.seedrouter.onboarding/`,
  `~/Library/Application Support/com.seedrouter.onboarding/`); an installation made before this
  change keeps using the old directory and should be uninstalled rather than upgraded in place.
- Release workflow: code signing and macOS notarization are wired but dormant, each step guarded
  by the presence of its repository secret. Adding the secrets is the only step needed to turn
  signing on. Release notes now describe the actual signed/unsigned state instead of hardcoding
  "unsigned", and an unsigned build is always published as a pre-release. See `docs/RELEASE.md`.
- Terminology: "company gateway" → "service gateway" (zh-CN 公司网关 → 服务网关) across the UI,
  bundled help docs, preset provider name and living docs. Internal codes (e.g.
  `differs_from_company_gateway`) are unchanged.

### Fixed (smoke run)

- Diagnose: the rule engine now derives rule NET from the environment snapshot — a Blocked
  gateway / npm-registry check or a Warn-level GitHub check yields `network.unreachable`
  (targets listed, gateway first; Warning when only Warn-level checks are affected). Before,
  "Run diagnosis" on the Environment step answered "No diagnosis available yet" for an
  unreachable gateway.
- Install: `InstallDoneEvent.timedOut` (additive; mirrored in `types.ts`) marks a job the
  install timeout killed; the npm item shows a dedicated hint ("stopped after N min … switch
  the registry / check VPN") instead of the generic "did not finish normally".
- Windows installer: upgrading v0.1.0-test.2 (built before `bundle.publisher` was set) to
  v0.1.0-test.3 failed with "NSIS Error: Error launching installer". Tauri's reinstall page runs the previous
  uninstaller with `_?=<dir>` read from `HKCU\Software\<publisher>\<product>`; the old
  builds wrote that key under the fallback publisher `company`, so `_?=` was empty and the
  uninstaller aborted. `src-tauri/nsis/hooks.nsh` (wired via `installerHooks`) recovers the
  directory from the legacy key or the uninstall string before any page runs;
  `scripts/check-nsis-hooks.mjs` (`npm run nsis:check`) keeps its literal registry paths in
  sync with `tauri.conf.json`.
- Environment check: the status badge now sits on the same line as the check title (weight
  distinguishes them); message, details and fix buttons follow underneath.
- Configure: the Edit button of each provider value now renders inside `CopyField` (new
  `actions` slot) next to Copy, and the value box is 40 px like the buttons, so the right-hand
  buttons line up with the field instead of floating on the hint line.
