//! Child-process execution with timeouts, captured/streamed output and Windows no-window
//! semantics. **All** command execution in this crate goes through this module.
//!
//! TODO(impl):
//! - `run`: spawn with tokio, apply timeout, capture stdout/stderr, never inherit a console
//!   window on Windows (CREATE_NO_WINDOW), return `CommandOutput`.
//! - `run_streaming`: same but invoke `on_line` per line as it arrives; honour `cancel`.
//! - `fresh_session_env`: PATH (and friends) as a *newly opened* terminal would see it —
//!   Windows: HKLM + HKCU `Environment` registry values with `%VAR%` expansion;
//!   macOS: output of `$SHELL -ilc env` (login+interactive shell) — used by M4 verify to
//!   detect the "terminal not restarted" fault (guide fault C/E).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::watch;

use crate::error::{AppError, AppResult};
use crate::models::OutputStream;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    /// When `true`, `env` *replaces* the inherited environment instead of extending it.
    pub clear_env: bool,
    pub cwd: Option<PathBuf>,
    pub timeout: Duration,
}

impl CommandSpec {
    pub fn new(
        program: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
            env: BTreeMap::new(),
            clear_env: false,
            cwd: None,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Shell-style rendering for display to the user (never executed as a shell string).
    pub fn display(&self) -> String {
        let mut parts = vec![quote(&self.program)];
        parts.extend(self.args.iter().map(|a| quote(a)));
        parts.join(" ")
    }
}

fn quote(s: &str) -> String {
    if s.is_empty() || s.chars().any(|c| c.is_whitespace() || c == '"') {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_owned()
    }
}

#[derive(Debug, Clone, Default)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub timed_out: bool,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.exit_code == Some(0) && !self.timed_out
    }
}

/// Runs a command to completion.
pub async fn run(spec: &CommandSpec) -> AppResult<CommandOutput> {
    let _ = spec;
    Err(AppError::Other("process::run not implemented".into()))
}

/// Runs a command, calling `on_line` for every output line. Stops early when `cancel`
/// flips to `true`.
pub async fn run_streaming<F>(
    spec: &CommandSpec,
    cancel: watch::Receiver<bool>,
    on_line: F,
) -> AppResult<CommandOutput>
where
    F: FnMut(OutputStream, String) + Send + 'static,
{
    let _ = (spec, cancel, on_line);
    Err(AppError::Other(
        "process::run_streaming not implemented".into(),
    ))
}

/// Environment variables as a freshly opened terminal would see them (see module docs).
pub async fn fresh_session_env() -> AppResult<BTreeMap<String, String>> {
    Err(AppError::Other(
        "process::fresh_session_env not implemented".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_quotes_args_with_spaces() {
        let spec = CommandSpec::new("npm", ["install", "-g", "@openai/codex", "a b"]);
        assert_eq!(spec.display(), "npm install -g @openai/codex \"a b\"");
    }
}
