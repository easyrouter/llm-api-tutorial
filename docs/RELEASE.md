# Release

No auto-update (PRD #13): users download new versions from the intranet portal / official
download page (PRD #11). Every distributed build must be signed (PRD #12).

## Version bump

1. Update `version` in `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`
   (identical values), move _Unreleased_ to a new section in `CHANGELOG.md`.
2. `git commit -m "chore(release): vX.Y.Z"` on a `release/vX.Y.Z` branch → PR → merge → tag
   `vX.Y.Z` on `main`.

## Pilot test builds

Two builds are piloted side by side; they differ only in the optional Codex Fast UI toolkit
(ADR-0007). Keep their versions distinct so the installer file name says which one it is —
the bundler derives it from `version`, and two files both called `…_0.1.0_x64-setup.exe` are
indistinguishable once downloaded.

| branch                      | version                | tag                     | contains the patch |
| --------------------------- | ---------------------- | ----------------------- | ------------------ |
| `feat/onboarding-revisions` | `0.1.0-test.4`         | `v0.1.0-test.4`         | no                 |
| `feat/codex-fast-ui`        | `0.1.0-test.4-patched` | `v0.1.0-test.4-patched` | yes                |

Every pilot round ships **both** tags (`vX-test.N` from `feat/onboarding-revisions`,
`vX-test.N-patched` from `feat/codex-fast-ui`); fixes land on `feat/onboarding-revisions` and
are merged into `feat/codex-fast-ui`, which then bumps only its own version.

Both are pre-releases and unsigned until Q-12 is answered. Build either with
_Actions → Release → Run workflow_ on the branch, or by pushing its tag. NSIS accepts the
semver pre-release suffix; the DMG is unaffected.

## Build

CI (`.github/workflows/ci.yml`, job `bundle`) produces:

- Windows: `src-tauri/target/release/bundle/nsis/*-setup.exe` (per-user install, no admin)
- macOS: `src-tauri/target/universal-apple-darwin/release/bundle/dmg/*.dmg`

Local: `npm run tauri build` (add `--target universal-apple-darwin` on macOS).

## How the release workflow is wired

Three jobs, in order:

1. **`create-release`** (ubuntu) — composes the release notes and creates the GitHub release
   exactly once, then exposes the tag as an output. Re-running a release finds the existing one
   and reuses it instead of making a second.
2. **`bundle`** (windows + macOS, in parallel) — builds and uploads its installer into that
   release. It deliberately does _not_ pass a release name, body or pre-release flag: those
   belong to the single job that creates the release.
3. **`verify-assets`** — fails the run unless the tag resolves to exactly one release carrying
   exactly one `*-setup.exe` and one `*.dmg`.

The split exists because of a real failure: when both bundle jobs created the release
themselves, they raced. On `v0.1.0-test.3-patched` the Windows and macOS jobs each created a
release for the same tag within the same second, GitHub accepted both, and the `.exe` landed on
the duplicate the tag did not resolve to — a green run with a silently missing installer, fixed
by hand afterwards. `v0.1.0-test.3` escaped only because its jobs started six seconds apart.
`verify-assets` exists so that failure mode can never be silent again.

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

### TBD before purchase — Windows key form (Q-12a)

The Windows wiring above assumes an **exportable `.pfx`**. That assumption may not survive
procurement, so settle it before money is spent:

- Since June 2023 the CA/Browser Forum requires Windows OV/EV code-signing private keys to live
  on FIPS-140-2 hardware — a hardware token, or a cloud signing service (Azure Trusted Signing,
  DigiCert KeyLocker, SSL.com eSigner). **None of those can hand you a `.pfx`.**
- If IT procures a hardware/cloud key, the _Prepare Windows code signing_ step must be replaced
  by `bundle.windows.signCommand` invoking the vendor's signing tool. The guard pattern, the
  release-note composition and the whole macOS side are unaffected.
- Also decide **OV vs EV**: an OV certificate still has to accumulate SmartScreen reputation, so
  early users keep seeing the warning the signing was bought to remove; EV is trusted on first
  download. For intranet-only distribution OV is usually enough.

Until this is answered, treat the Windows half of the table above as provisional.

A third option worth pricing against the others for an intranet-only tool: skip the public CA,
issue an internal certificate and deploy its root to domain machines by GPO. Zero recurring cost
and no SmartScreen reputation problem on managed machines, but useless for anyone off the domain.

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
