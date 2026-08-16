# ADR-0007 — Ship the Codex "Fast UI" toolkit as an optional, isolated, reversible extra

- Status: accepted (branch `feat/codex-fast-ui`; `feat/onboarding-revisions` is the same product
  without it, so both can be piloted side by side)
- Date: 2026-08-16
- Related: ADR-0003 (never write the tools' config), ADR-0006 (CC Switch deep-link import),
  PRD #4, #7, hard rules 1, 4, 6, 7

## Context

The Codex client only shows its speed / service-tier option to users who are signed in with a
ChatGPT account. The service gateway supports the `priority` tier and the `config.toml` template
we ship enables it, but a user who cannot sign in (OpenAI auth unavailable) never sees the
control — the same configuration therefore produces a visibly different product for different
people, which is exactly what this onboarding tool exists to prevent.

The dev team supplied `CodexFastUI-Minimal-2026.07.30.zip` (MIT, ~1.6 MB, vendors
`@electron/asar` 4.2.1; the approach is adapted from the public `congwa/codex-openfast`). What it
actually does, after reading every file in it:

- `install.ps1` **copies** the officially installed Codex app (AppX package `OpenAI.Codex`, from
  its own install location) into a target directory, then patches only the copy.
- `patch-fast-ui.mjs` unpacks the copy's `app.asar`, locates the minified service-tier module by
  content, and replaces exactly one expression (`isServiceTierAllowed`'s final gate) with `true`.
  It matches two known variants and **refuses to patch anything else**, so an unrecognised Codex
  build stops the run instead of being corrupted.
- The copy gets its own `owl-app.ini` user-data directory (`CodexFastUI`), so it runs beside the
  official client instead of replacing it.
- `install.ps1` refuses to write into `WindowsApps` or into the source application directory,
  backs the original `app.asar` up, and `restore.ps1` puts it back. `verify.ps1` re-checks the
  installed copy (patch marker present, `node --check` on the patched file, isolated profile,
  shortcut target).
- No account, token or configuration file is read or written anywhere in the toolkit.

## Decision

Ship it, as an **optional** action on the Codex tab of the configure step, Windows only, behind
the same show-before-run confirmation every other command uses.

Concretely (`src-tauri/src/fast_ui.rs`, `src/features/configure/CodexFastUiCard.tsx`):

1. The archive is a bundled app resource. Its SHA-256 is pinned in `fast_ui::TOOLKIT_SHA256` and
   verified before every use; a mismatch makes the feature unavailable rather than running it.
   This is the offline equivalent of hard rule 7 — nothing is downloaded at runtime.
2. It is extracted into the app cache dir from the verified archive on every run, so the files
   that execute are always the ones the pinned hash covers.
3. `-OutputRoot` is forced to `<local app data>/SeedRouter/CodexFastUI`. The toolkit's own default
   (`~/Downloads/Report/CodexFastUI`) is not used, and the install root is visible in the command.
4. The exact `powershell.exe …` command line is rendered from the same `CommandSpec` that is
   executed, shown in the confirmation dialog, and re-derived and compared in `fast_ui::start` —
   a webview that tampers with the plan gets `invalid_input`, not a different command.
5. Install / verify / restore are all offered; restore is styled as the destructive action. The
   card states that the official installation is untouched and that deleting the folder is a
   complete uninstall.
6. The card is framed as the fallback: the supported route is the optional
   "sign in with your ChatGPT account" step, which sits above it and needs no patch at all.

## Consequences

- Users without OpenAI auth get the same speed option as everyone else — the reason PM asked for
  this — without us touching their official Codex install, their configuration, or requiring
  elevation.
- We take on a real maintenance obligation: the patch is pinned to Codex build
  `26.721.11231.0`. On a newer build the patcher stops cleanly (good), but the feature is then
  dead until IT supplies a new archive; updating means replacing the resource **and**
  `TOOLKIT_SHA256`, in one commit. Tracked as Q-FU1.
- We are distributing an unofficial modification of a third-party application. The card says so
  in both languages, names the tested build, and points at the supported alternative. Legal /
  IT sign-off on redistribution is Q-FU2 — the code is inert without the archive, so the
  decision can be reversed by dropping one resource file.
- The bundle grows by ~1.6 MB on both platforms (the resource is not per-platform); the feature
  itself is inert on macOS, where `fast_ui::status` reports `supported: false` and the card
  renders nothing.

## Alternatives considered

- **Refuse outright.** The original recommendation, made before reading the archive — on the
  assumption that it patched the user's installed client in place. It does not: it builds an
  isolated copy and is reversible, which removes the objection that mattered.
- **Hand users the zip and a wiki page.** Keeps the patch out of our binary, but every user then
  runs an unverified script from a chat message; we would lose the hash pin, the forced install
  root, the show-before-run dialog and the restore button.
- **Download the toolkit at runtime from an intranet URL** (like the CC Switch installer). Better
  for updates, but makes the feature dependent on IT hosting before it can be piloted at all.
  Worth revisiting if the toolkit needs to track Codex releases closely (Q-FU1).
