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

## Signing

The wiring lives in `.github/workflows/release.yml` and is **dormant until the secrets exist**.
Each signing step is guarded by `secrets.<NAME> != ''`, resolved once into the job-level
`SIGN_WINDOWS` / `SIGN_MACOS` / `NOTARIZE_MACOS` flags (a step-level `if:` cannot read `secrets`
directly). With no secrets set the release workflow behaves exactly as it did before signing was
wired: unsigned artifacts, unsigned wording in the release notes, forced pre-release.

**Adding the repository secrets is the only action required — no workflow edit.**

| secret                         | platform | purpose                                                            |
| ------------------------------ | -------- | ------------------------------------------------------------------ |
| `WINDOWS_CERTIFICATE`          | Windows  | base64 of the `.pfx`; presence turns Windows signing on            |
| `WINDOWS_CERTIFICATE_PASSWORD` | Windows  | `.pfx` password                                                    |
| `WINDOWS_TIMESTAMP_URL`        | Windows  | optional; defaults to `http://timestamp.digicert.com`              |
| `APPLE_CERTIFICATE`            | macOS    | base64 of the Developer ID `.p12`; presence turns macOS signing on |
| `APPLE_CERTIFICATE_PASSWORD`   | macOS    | `.p12` password                                                    |
| `APPLE_SIGNING_IDENTITY`       | macOS    | e.g. `Developer ID Application: <name> (<team id>)`                |
| `APPLE_ID`                     | macOS    | Apple ID for notarization; presence turns notarization on          |
| `APPLE_PASSWORD`               | macOS    | app-specific password, **not** the account password                |
| `APPLE_TEAM_ID`                | macOS    | Developer Team ID                                                  |

How each side is applied:

- **Windows** — the guarded step imports the `.pfx` into `Cert:/CurrentUser/My`, reads back the
  thumbprint and merges `bundle.windows.{certificateThumbprint,digestAlgorithm,timestampUrl}`
  into the build with `tauri build --config <fragment>`. The thumbprint is therefore never
  committed. Timestamping matters: without it, signatures stop validating when the certificate
  expires.
- **macOS** — the guarded steps export the `APPLE_*` variables into `$GITHUB_ENV`; tauri-action
  imports the `.p12` into a throwaway keychain and, when the notarization variables are present,
  submits the bundle to Apple. Signing without notarizing still trips Gatekeeper on first open,
  and the generated release notes say so.

Deliberate detail: the guarded steps write to `$GITHUB_ENV` rather than the workflow declaring
these variables up front. A skipped step leaves a variable genuinely **undefined**, whereas
declaring it would leave it defined-but-empty — and an empty `APPLE_SIGNING_IDENTITY` makes the
bundler attempt to sign with an empty identity instead of skipping.

Caveat for procurement: this wiring assumes an exportable `.pfx`/`.p12`. Since June 2023 the
CA/Browser Forum requires Windows OV/EV code-signing keys to live on FIPS-140-2 hardware
(hardware token, or a cloud service such as Azure Trusted Signing / DigiCert KeyLocker /
SSL.com eSigner), and those cannot produce a `.pfx`. If IT procures one of those, the Windows
step must be swapped for `bundle.windows.signCommand` invoking the vendor's signing tool; the
macOS side and everything else is unaffected. Settle this in Q-12 **before** buying.

Certificates never enter the repository. `ci.yml` bundles stay unsigned on purpose — they are
smoke tests, and cloud signing services bill per signature.

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
