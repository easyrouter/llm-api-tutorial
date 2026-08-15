# ADR-0001: Use Tauri 2 (Rust core + React/TypeScript UI)

- Status: accepted
- Date: 2026-08-15

## Context

The PRD (§五) compared Tauri 2, Wails, Electron, .NET WPF/WinUI 3 and a TUI. Hard constraints
from the clarification answers: lightweight, Windows **and** macOS (#6), local system operations
(run node/npm/codex, read env vars, list processes), non-admin install (#7), signed distribution
(#12), bilingual UI (#9), non-developer audience (#8).

## Decision

Tauri 2 with React 19 + TypeScript + Vite + Tailwind 4 on the frontend, tokio-based Rust core.
Package manager: npm (no extra tooling on IT machines). Frontend state: zustand; i18n: i18next.

## Consequences

- Small installers (~5–15 MB), low memory, native WebView (WebView2 / WKWebView).
- Same stack as CC Switch (MIT) — its environment-detection code and data layout can be
  consulted directly.
- All privileged work (process spawning, filesystem, network) lives in Rust behind validated
  commands; the webview gets no `fs`/`shell` capabilities.
- Team needs some Rust; mitigated by keeping domain modules small and heavily unit-tested.
- Rejected: Electron (size), WPF/WinUI (Windows-only), Wails (smaller ecosystem, no CC Switch
  parity), TUI (audience).
