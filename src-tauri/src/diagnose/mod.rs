//! M5 — rule engine translating the guide's fault table (A–G) into actionable diagnoses,
//! plus the redacted diagnostic report (replaces "attach a screenshot" in tickets).
//!
//! Rule mapping (assumption pending confirmation against the original guide — see
//! docs/OPEN_QUESTIONS.md; codes → frontend key `diagnose.<code>.*`):
//!
//! | rule | trigger                                   | code                    | severity |
//! |------|-------------------------------------------|-------------------------|----------|
//! | A    | HttpStatus 401/403                        | auth.key_checklist      | Blocking |
//! | B    | HttpStatus 404                            | url.rule_recheck        | Blocking |
//! | C    | NoEffectAfterConfig / running terminals   | session.restart_terminal| Warning  |
//! | D    | EnvVarConflict                            | env.conflict            | Warning  |
//! | E    | CommandNotFound / tool `on_path=false`    | path.not_refreshed      | Blocking |
//! | F    | ProtocolMismatch / HttpStatus 400/422     | protocol.mismatch       | Blocking |
//! | G    | AppBlockedByOs                            | os.app_blocked          | Warning  |
//! | –    | NetworkError / Timeout                    | network.unreachable     | Blocking |
//! | –    | HttpStatus 429 / 5xx                      | gateway.upstream        | Warning  |
//!
//! Every diagnosis carries `checklist` codes (`diagnose.<code>.steps.<n>`) and `FixAction`s.
//! Env-var fixes are always `Instructions` — the app never edits them (PRD #17).
//!
//! `report::build` renders Markdown: app/OS info, check table, tool versions, masked env vars,
//! diagnoses, and (read-only) presence + top-level keys of `~/.codex/config.toml` /
//! `~/.claude/settings.json` with all values redacted. Reading is allowed; writing is not.

pub mod report;

use crate::models::{AppConfig, DiagnoseRequest, Diagnosis};

pub fn diagnose(req: &DiagnoseRequest, config: &AppConfig) -> Vec<Diagnosis> {
    // TODO(impl): rule engine per module docs; sort by severity desc, dedupe by rule_id
    let _ = (req, config);
    Vec::new()
}
