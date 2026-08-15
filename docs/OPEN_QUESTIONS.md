# Open questions / 待澄清事项

The PRD (`docs/prd/`) answered most clarification items. The following are **still open** and
are tracked here so development can proceed under explicit assumptions. Each item names the
assumption currently built into the code and where it lives (`TODO(IT …)` markers).

| id    | question                                                                                                                                           | current assumption in code                                                                                                                                      | where                                                     |
| ----- | -------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------- |
| Q-14  | PRD #14 answer is truncated ("能不能用户在…"). How are API keys issued — self-service from the gateway portal, or pushed via SSO / key-management? | Self-service: user pastes the key; tool validates format only, never stores it. If SSO issuance is possible later, add a `keyProvider` step before `configure`. | `guide::validate_api_key`, Verify screen                  |
| Q-1   | Concrete company gateway base URL (PRD #1 says it is the only constant)                                                                            | placeholder `https://gateway.example.com/v1`                                                                                                                    | `src-tauri/resources/app-config.json` → `gateway.baseUrl` |
| Q-18  | Which documentation site (PRD #18 "指定文档网站")? URL + whether it can serve `index.json` + Markdown                                              | placeholder `https://docs.example.com/codex-onboarding`; contract described in `src-tauri/src/docs/mod.rs`; bundled fallback used when unreachable              | `app-config.json` → `docs`                                |
| Q-16  | Telemetry endpoint, retention, approval owner                                                                                                      | disabled until `telemetry.endpoint` is set; payload schema in `docs/ARCHITECTURE.md` §7                                                                         | `app-config.json` → `telemetry`                           |
| Q-12  | Signing: who holds the Windows Authenticode cert and Apple Developer ID? CI secrets or manual signing?                                             | CI job prepared with secret placeholders; unsigned CI bundles are smoke tests only                                                                              | `.github/workflows/ci.yml`, `docs/RELEASE.md`             |
| Q-CC1 | CC Switch distribution: pin a version + SHA-256 for intranet mirror, or always latest GitHub release?                                              | latest release from `farion1231/cc-switch` GitHub API, hash read from release assets when present; `intranetMirror` empty                                       | `app-config.json` → `ccSwitch`                            |
| Q-D1  | Exact wording / order of the guide's fault table A–G                                                                                               | assumed mapping in `src-tauri/src/diagnose/mod.rs` header                                                                                                       | `diagnose/`                                               |
| Q-U1  | Exact "API 地址处理规则" from the guide (used by the URL preview)                                                                                  | trailing `/` stripped; `#` suffix = literal; `/v1` kept; bare origin → `/v1` appended                                                                           | `guide::preview_url`                                      |
| Q-21  | Acceptance metrics + baseline (first-run success rate, mean duration, ticket volume)                                                               | not needed for build; telemetry events already carry step/status/duration to compute them                                                                       | —                                                         |
| Q-22  | Pilot department & timeline                                                                                                                        | —                                                                                                                                                               | —                                                         |
| Q-M1  | Model names / reasoning-effort options users may pick (PRD #1: vary per user)                                                                      | free-text hints; empty defaults                                                                                                                                 | `app-config.json` → `gateway.defaultModel`                |

Process: when IT answers, update `app-config.json` (or the intranet override), remove the
`TODO(IT …)` marker, and move the row to the _Resolved_ table below with the date.

## Resolved

| id  | answer | date |
| --- | ------ | ---- |
| —   | —      | —    |
