# Release

No auto-update (PRD #13): users download new versions from the intranet portal / official
download page (PRD #11). Every distributed build must be signed (PRD #12).

## Version bump

1. Update `version` in `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`
   (identical values), move _Unreleased_ to a new section in `CHANGELOG.md`.
2. `git commit -m "chore(release): vX.Y.Z"` on a `release/vX.Y.Z` branch → PR → merge → tag
   `vX.Y.Z` on `main`.

## Build

CI (`.github/workflows/ci.yml`, job `bundle`) produces:

- Windows: `src-tauri/target/release/bundle/nsis/*-setup.exe` (per-user install, no admin)
- macOS: `src-tauri/target/universal-apple-darwin/release/bundle/dmg/*.dmg`

Local: `npm run tauri build` (add `--target universal-apple-darwin` on macOS).

## Signing (to be wired once IT provides material — OPEN_QUESTIONS Q-12)

| platform | mechanism                                                                      | secrets                                                                                                                    |
| -------- | ------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------- |
| Windows  | Tauri `bundle.windows.signCommand` or `certificateThumbprint` + `timestampUrl` | `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD`                                                                      |
| macOS    | Developer ID Application cert + notarization                                   | `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` |

Certificates never enter the repository. Unsigned CI artifacts are for smoke tests only.

## Pre-release checklist (pilot gate, PRD P4)

- [ ] Clean Windows 10/11 VM: zero → `codex --version` + gateway probe OK without reading docs
- [ ] Clean macOS 12+ VM: same
- [ ] Fault injection — each of A–G reproduced and correctly diagnosed with an actionable fix
- [ ] Both languages reviewed by a native reader
- [ ] Diagnostic report contains no secrets (search for `sk-`, `Bearer`, key fragments)
- [ ] Telemetry payload reviewed against `docs/ARCHITECTURE.md` §7
- [ ] Installer signed; SmartScreen / Gatekeeper do not warn
- [ ] `app-config.json` `TODO(IT …)` markers resolved or consciously deferred

## Distribution

Upload the signed installers to the intranet portal and the official download page; publish
the SHA-256 of each file next to the download link.
