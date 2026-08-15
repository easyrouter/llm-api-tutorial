//! Child-process execution with timeouts, captured/streamed output and Windows no-window
//! semantics. **All** command execution in this crate goes through this module.
//!
//! Guarantees
//! - Programs are spawned directly (argument vector, no shell string), so user-controlled
//!   values can never be interpreted by a shell. On Windows `npm`/`codex`-style shims
//!   (`*.cmd`, `*.bat`) are resolved to their full path via `which` first, because
//!   `CreateProcess` only finds `.exe` files on its own.
//! - Every run has a timeout (`CommandSpec::timeout`). A timeout is *data*
//!   (`CommandOutput::timed_out`), not an error; only a failed spawn is an error.
//! - Windows children are created with `CREATE_NO_WINDOW`, stdin is always null.
//! - Timeout / cancel kill the child's whole tree, best effort: Windows `taskkill /T`; on
//!   Unix every child starts in its own process group and the group is signalled
//!   (`kill -9 -- -<pgid>`) before the direct child is killed, so `brew`'s ruby/curl or npm
//!   lifecycle scripts do not linger. Descendants that left the group (daemons) are not chased.
//! - `fresh_session_env` reconstructs the environment a *newly opened* terminal would see
//!   (Windows: registry; macOS/Linux: login shell), which lets M4 verify detect the "terminal
//!   not restarted" fault (guide faults C/E). `run_in_fresh_session` runs a spec against it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, watch};

use crate::error::{AppError, AppResult};
use crate::models::OutputStream;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);

/// Time allowed for a login shell to print its environment (`fresh_session_env`).
#[cfg(not(windows))]
const FRESH_ENV_TIMEOUT: Duration = Duration::from_secs(8);

/// Grace period for draining output pipes after the child was killed (grandchildren may still
/// hold the pipe handles; we never wait for them).
const DRAIN_AFTER_KILL: Duration = Duration::from_millis(750);

/// Windows `CREATE_NO_WINDOW` process-creation flag.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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

    /// Builder-style timeout override.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Builder-style single environment variable.
    #[must_use]
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Builder-style working directory.
    #[must_use]
    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
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

// ---------------------------------------------------------------------------
// Program resolution
// ---------------------------------------------------------------------------

/// `true` when `program` is a path (contains a separator) rather than a bare command name.
fn is_path_like(program: &str) -> bool {
    program.contains('/') || program.contains('\\')
}

/// Resolves a bare command name to an executable on the current `PATH` (`which`). On Windows
/// this also finds `.cmd`/`.bat`/`.exe` shims via `PATHEXT` (e.g. `npm` → `npm.cmd`).
/// Returns `None` for names that cannot be found; paths are returned as-is when they exist.
pub fn resolve_program(name: &str) -> Option<PathBuf> {
    which::which_global(name).ok()
}

/// Resolves `name` against an explicit `PATH` value (`;`/`:` separated) instead of the
/// process environment. Used to check what a *fresh* terminal would find.
pub fn resolve_program_in(name: &str, path: &str) -> Option<PathBuf> {
    which::which_in_global(name, Some(path))
        .ok()
        .and_then(|mut it| it.next())
}

/// Program string handed to the OS. On Windows, bare names without an extension are resolved
/// through `which` so `.cmd`/`.bat` shims work without going through a shell.
fn effective_program(spec: &CommandSpec) -> String {
    if cfg!(windows)
        && !is_path_like(&spec.program)
        && Path::new(&spec.program).extension().is_none()
    {
        let resolved = match (spec.clear_env, get_env_var(&spec.env, "PATH")) {
            (true, Some(path)) => resolve_program_in(&spec.program, path),
            _ => resolve_program(&spec.program),
        };
        if let Some(path) = resolved {
            return path.to_string_lossy().into_owned();
        }
    }
    spec.program.clone()
}

// ---------------------------------------------------------------------------
// Environment map helpers (case-insensitive on Windows)
// ---------------------------------------------------------------------------

/// `true` when environment variable names compare case-insensitively (Windows).
fn env_names_case_insensitive() -> bool {
    cfg!(windows)
}

/// Looks up `name` in an environment map, case-insensitively on Windows.
pub fn get_env_var<'a>(env: &'a BTreeMap<String, String>, name: &str) -> Option<&'a str> {
    if let Some(v) = env.get(name) {
        return Some(v.as_str());
    }
    if env_names_case_insensitive() {
        env.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    } else {
        None
    }
}

/// Inserts `name=value`, replacing an existing entry that differs only in case on Windows.
pub fn set_env_var(env: &mut BTreeMap<String, String>, name: &str, value: String) {
    if env_names_case_insensitive() && !env.contains_key(name) {
        let existing = env.keys().find(|k| k.eq_ignore_ascii_case(name)).cloned();
        if let Some(existing) = existing {
            env.insert(existing, value);
            return;
        }
    }
    env.insert(name.to_owned(), value);
}

// ---------------------------------------------------------------------------
// Spawning
// ---------------------------------------------------------------------------

fn build_command(spec: &CommandSpec) -> Command {
    let mut cmd = Command::new(effective_program(spec));
    cmd.args(&spec.args);
    if spec.clear_env {
        cmd.env_clear();
    }
    cmd.envs(&spec.env);
    if let Some(cwd) = &spec.cwd {
        cmd.current_dir(cwd);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    // Own process group (pgid = child pid) so `kill_child` can signal the whole tree.
    #[cfg(unix)]
    cmd.process_group(0);
    cmd
}

fn spawn(spec: &CommandSpec) -> AppResult<Child> {
    build_command(spec).spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::CommandNotFound {
                program: spec.program.clone(),
            }
        } else {
            AppError::Io(e)
        }
    })
}

/// Reads `reader` line by line (lossy UTF-8, `\r\n`/`\n` stripped) into `tx`.
async fn pump_lines<R>(
    reader: R,
    stream: OutputStream,
    tx: mpsc::UnboundedSender<(OutputStream, String)>,
) where
    R: AsyncRead + Unpin,
{
    let mut reader = BufReader::new(reader);
    let mut buf = Vec::with_capacity(256);
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buf)
                    .trim_end_matches(['\n', '\r'])
                    .to_owned();
                if tx.send((stream, line)).is_err() {
                    break;
                }
            }
        }
    }
}

/// Kills the child and, best effort, its whole tree: Windows `taskkill /T` (killing a `.cmd`
/// shim alone would leave the real `node.exe` running); Unix `kill -9 -- -<pgid>` on the
/// process group `build_command` created for the child. The direct child is killed last.
async fn kill_child(child: &mut Child) {
    if let Some(pid) = child.id() {
        let mut tree_kill = tree_kill_command(pid);
        if let Ok(mut proc) = tree_kill.spawn() {
            let _ = tokio::time::timeout(Duration::from_secs(5), proc.wait()).await;
        }
    }
    let _ = child.kill().await;
}

/// Platform command that terminates the process tree rooted at `pid` (silent, no window).
fn tree_kill_command(pid: u32) -> Command {
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("taskkill");
        c.args(["/T", "/F", "/PID", &pid.to_string()]);
        c
    } else {
        // `--` so the negative (group) pid is not parsed as a signal; `pid` doubles as the
        // pgid because the child was started with `process_group(0)`.
        let mut c = Command::new("kill");
        c.args(["-9", "--", &format!("-{pid}")]);
        c
    };
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Why the run loop stopped waiting for the child.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    Exited(Option<i32>),
    TimedOut,
    Cancelled,
}

/// Waits until `cancel` flips to `true` (or forever when the sender is gone before that).
async fn cancelled(cancel: &mut Option<watch::Receiver<bool>>) {
    match cancel {
        Some(rx) => {
            if *rx.borrow() {
                return;
            }
            while rx.changed().await.is_ok() {
                if *rx.borrow() {
                    return;
                }
            }
            std::future::pending::<()>().await;
        }
        None => std::future::pending::<()>().await,
    }
}

/// Shared engine behind `run` and `run_streaming`.
async fn execute<F>(
    spec: &CommandSpec,
    mut cancel: Option<watch::Receiver<bool>>,
    mut on_line: F,
) -> AppResult<CommandOutput>
where
    F: FnMut(OutputStream, String),
{
    let started = Instant::now();
    let mut child = spawn(spec)?;

    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut readers = Vec::with_capacity(2);
    if let Some(out) = child.stdout.take() {
        readers.push(tokio::spawn(pump_lines(
            out,
            OutputStream::Stdout,
            tx.clone(),
        )));
    }
    if let Some(err) = child.stderr.take() {
        readers.push(tokio::spawn(pump_lines(
            err,
            OutputStream::Stderr,
            tx.clone(),
        )));
    }
    drop(tx);

    let deadline = tokio::time::sleep(spec.timeout);
    tokio::pin!(deadline);
    let mut readers_open = true;

    // `biased` + timeout/cancel first: a process flooding stdout can never starve the deadline.
    // Lines still queued when the child exits are delivered by the drain loop below.
    let stop = loop {
        tokio::select! {
            biased;
            () = &mut deadline => break Stop::TimedOut,
            () = cancelled(&mut cancel) => break Stop::Cancelled,
            status = child.wait() => {
                break Stop::Exited(status.ok().and_then(|s| s.code()));
            }
            line = rx.recv(), if readers_open => match line {
                Some((stream, text)) => on_line(stream, text),
                None => readers_open = false,
            },
        }
    };

    let drain_deadline = match stop {
        Stop::Exited(_) => {
            Instant::now()
                + spec
                    .timeout
                    .saturating_sub(started.elapsed())
                    .max(DRAIN_AFTER_KILL)
        }
        Stop::TimedOut | Stop::Cancelled => {
            kill_child(&mut child).await;
            Instant::now() + DRAIN_AFTER_KILL
        }
    };
    if readers_open {
        while let Ok(Some((stream, text))) =
            tokio::time::timeout_at(drain_deadline.into(), rx.recv()).await
        {
            on_line(stream, text);
        }
    }
    for reader in readers {
        reader.abort();
    }

    let mut output = CommandOutput {
        duration_ms: elapsed_ms(started),
        ..CommandOutput::default()
    };
    match stop {
        Stop::Exited(code) => output.exit_code = code,
        Stop::TimedOut => output.timed_out = true,
        Stop::Cancelled => on_line(OutputStream::System, "cancelled".to_owned()),
    }
    Ok(output)
}

fn elapsed_ms(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Runs a command to completion, capturing stdout and stderr (lossy UTF-8, `\n` line ends).
/// A timeout kills the process and is reported through `CommandOutput::timed_out`; a program
/// that cannot be found yields `AppError::CommandNotFound`.
pub async fn run(spec: &CommandSpec) -> AppResult<CommandOutput> {
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut output = execute(spec, None, |stream, line| {
        let target = match stream {
            OutputStream::Stdout => &mut stdout,
            OutputStream::Stderr => &mut stderr,
            OutputStream::System => return,
        };
        target.push_str(&line);
        target.push('\n');
    })
    .await?;
    output.stdout = stdout;
    output.stderr = stderr;
    Ok(output)
}

/// Runs a command, calling `on_line` for every output line as it arrives (stdout and stderr
/// are read concurrently). Stops early when `cancel` flips to `true`: the process tree is
/// killed, `on_line(OutputStream::System, "cancelled")` is emitted and the returned output
/// has `exit_code == None` and `timed_out == false`. `stdout`/`stderr` of the returned value
/// stay empty — the lines were already delivered.
pub async fn run_streaming<F>(
    spec: &CommandSpec,
    cancel: watch::Receiver<bool>,
    on_line: F,
) -> AppResult<CommandOutput>
where
    F: FnMut(OutputStream, String) + Send + 'static,
{
    execute(spec, Some(cancel), on_line).await
}

// ---------------------------------------------------------------------------
// Fresh-session environment
// ---------------------------------------------------------------------------

/// Environment variables as a freshly opened terminal would see them (see module docs).
///
/// - Windows: `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment` merged with
///   `HKCU\Environment` (`PATH` = machine `;` user, `%VAR%` expanded, well-known per-logon
///   variables such as `USERPROFILE`/`APPDATA` taken from the current process). `PATH` is
///   stored under the key `PATH`; look values up with [`get_env_var`] (case-insensitive).
/// - macOS: output of `$SHELL -ilc env` (login + interactive shell, 8 s timeout).
/// - Linux: `$SHELL -lc env`.
/// - Any failure on Unix falls back to the current process environment (logged).
pub async fn fresh_session_env() -> AppResult<BTreeMap<String, String>> {
    #[cfg(windows)]
    {
        windows_env::fresh_session_env()
    }
    #[cfg(not(windows))]
    {
        Ok(unix_env::fresh_session_env().await)
    }
}

/// Runs `spec` the way a freshly opened terminal would: the program is resolved against the
/// fresh `PATH` (a program only reachable through the current process' `PATH` yields
/// `AppError::CommandNotFound` — guide fault E), and the child gets *only* the fresh
/// environment plus `spec.env`.
pub async fn run_in_fresh_session(spec: &CommandSpec) -> AppResult<CommandOutput> {
    let fresh = fresh_session_env().await?;
    let program = if is_path_like(&spec.program) {
        spec.program.clone()
    } else {
        let path = get_env_var(&fresh, "PATH").unwrap_or_default();
        resolve_program_in(&spec.program, path)
            .map(|p| p.to_string_lossy().into_owned())
            .ok_or_else(|| AppError::CommandNotFound {
                program: spec.program.clone(),
            })?
    };
    let mut env = fresh;
    for (k, v) in &spec.env {
        set_env_var(&mut env, k, v.clone());
    }
    let fresh_spec = CommandSpec {
        program,
        args: spec.args.clone(),
        env,
        clear_env: true,
        cwd: spec.cwd.clone(),
        timeout: spec.timeout,
    };
    run(&fresh_spec).await
}

/// Parses `KEY=VALUE` lines (output of `env`). Lines that do not look like an assignment
/// (shell banners, multi-line function bodies) are ignored; the first `=` splits key/value.
pub fn parse_env_lines(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if is_env_name(key) {
            map.insert(key.to_owned(), value.to_owned());
        }
    }
    map
}

fn is_env_name(key: &str) -> bool {
    let mut chars = key.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Expands `%VAR%` references the way Windows does: known variables are substituted, unknown
/// ones are kept literally. Pure — usable on every platform (tests, tooling).
pub fn expand_windows_env<F>(value: &str, lookup: F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        // `%NAME%` with a known NAME → substitute; anything else keeps the `%` literally.
        let substitution = after.find('%').and_then(|end| {
            let name = &after[..end];
            let value = if name.is_empty() { None } else { lookup(name) };
            value.map(|v| (v, end + 1))
        });
        if let Some((value, consumed)) = substitution {
            out.push_str(&value);
            rest = &after[consumed..];
        } else {
            out.push('%');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// Merges machine- and user-level Windows environment maps into one block: user values
/// override machine values, except `PATH`, which becomes `machine;user`. Names compare
/// case-insensitively; `PATH` is stored under `PATH`. Pure (no registry access) for tests.
pub fn merge_windows_env(
    machine: &BTreeMap<String, String>,
    user: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut merged: BTreeMap<String, String> = BTreeMap::new();
    for (k, v) in machine.iter().chain(user.iter()) {
        if k.eq_ignore_ascii_case("PATH") {
            continue;
        }
        let existing = merged.keys().find(|e| e.eq_ignore_ascii_case(k)).cloned();
        merged.insert(existing.unwrap_or_else(|| k.clone()), v.clone());
    }
    let path_of = |m: &BTreeMap<String, String>| {
        m.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("PATH"))
            .map(|(_, v)| v.trim_matches(';').to_owned())
            .unwrap_or_default()
    };
    let (m, u) = (path_of(machine), path_of(user));
    let joined = match (m.is_empty(), u.is_empty()) {
        (true, true) => String::new(),
        (false, true) => m,
        (true, false) => u,
        (false, false) => format!("{m};{u}"),
    };
    merged.insert("PATH".to_owned(), joined);
    merged
}

#[cfg(windows)]
mod windows_env {
    use std::collections::BTreeMap;

    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::types::FromRegValue;
    use winreg::RegKey;

    use super::{expand_windows_env, merge_windows_env, set_env_var};
    use crate::error::{AppError, AppResult};

    const MACHINE_KEY: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";
    const USER_KEY: &str = r"Environment";

    /// Per-logon variables that are not stored in the `Environment` registry keys but are
    /// present in every fresh terminal.
    const LOGON_VARS: &[&str] = &[
        "USERPROFILE",
        "USERNAME",
        "USERDOMAIN",
        "HOMEDRIVE",
        "HOMEPATH",
        "APPDATA",
        "LOCALAPPDATA",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramData",
        "ProgramW6432",
        "SystemRoot",
        "SystemDrive",
        "windir",
        "ComSpec",
        "PATHEXT",
        "TEMP",
        "TMP",
        "OS",
        "NUMBER_OF_PROCESSORS",
        "PROCESSOR_ARCHITECTURE",
        "PUBLIC",
        "ALLUSERSPROFILE",
    ];

    fn read_env_key(root: winreg::HKEY, path: &str) -> std::io::Result<BTreeMap<String, String>> {
        let key = RegKey::predef(root).open_subkey_with_flags(path, KEY_READ)?;
        let mut map = BTreeMap::new();
        for entry in key.enum_values() {
            let Ok((name, value)) = entry else { continue };
            if let Ok(text) = String::from_reg_value(&value) {
                map.insert(name, text);
            }
        }
        Ok(map)
    }

    fn lookup_in(map: &BTreeMap<String, String>, name: &str) -> Option<String> {
        super::get_env_var(map, name)
            .map(str::to_owned)
            .or_else(|| std::env::var(name).ok())
    }

    pub(super) fn fresh_session_env() -> AppResult<BTreeMap<String, String>> {
        let machine = read_env_key(HKEY_LOCAL_MACHINE, MACHINE_KEY).map_err(|e| {
            AppError::Other(format!(
                "cannot read machine environment from registry: {e}"
            ))
        })?;
        // HKCU\Environment may legitimately be missing on a pristine profile.
        let user = read_env_key(HKEY_CURRENT_USER, USER_KEY).unwrap_or_default();
        let mut env = merge_windows_env(&machine, &user);
        for name in LOGON_VARS {
            if super::get_env_var(&env, name).is_none() {
                if let Ok(v) = std::env::var(name) {
                    set_env_var(&mut env, name, v);
                }
            }
        }
        // Two passes so a value referencing another `%VAR%` that itself needs expansion works.
        for _ in 0..2 {
            let snapshot = env.clone();
            for value in env.values_mut() {
                if value.contains('%') {
                    *value = expand_windows_env(value, |name| lookup_in(&snapshot, name));
                }
            }
        }
        Ok(env)
    }
}

#[cfg(not(windows))]
mod unix_env {
    use std::collections::BTreeMap;

    use super::{parse_env_lines, CommandSpec, FRESH_ENV_TIMEOUT};

    fn login_shell() -> String {
        std::env::var("SHELL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "/bin/zsh".to_owned())
    }

    /// Flags that make the shell read its startup files like a new terminal window would.
    fn shell_flags() -> &'static str {
        if cfg!(target_os = "macos") {
            "-ilc"
        } else {
            "-lc"
        }
    }

    fn process_env() -> BTreeMap<String, String> {
        std::env::vars().collect()
    }

    pub(super) async fn fresh_session_env() -> BTreeMap<String, String> {
        let shell = login_shell();
        let spec =
            CommandSpec::new(shell.clone(), [shell_flags(), "env"]).with_timeout(FRESH_ENV_TIMEOUT);
        match super::run(&spec).await {
            Ok(out) if out.success() => {
                let env = parse_env_lines(&out.stdout);
                if env.contains_key("PATH") {
                    env
                } else {
                    log::warn!("login shell env has no PATH; using process environment");
                    process_env()
                }
            }
            Ok(out) => {
                log::warn!(
                    "login shell {shell} exited with {:?} (timed out: {}); using process environment",
                    out.exit_code,
                    out.timed_out
                );
                process_env()
            }
            Err(e) => {
                log::warn!("cannot run login shell {shell}: {e}; using process environment");
                process_env()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_quotes_args_with_spaces() {
        let spec = CommandSpec::new("npm", ["install", "-g", "@openai/codex", "a b"]);
        assert_eq!(spec.display(), "npm install -g @openai/codex \"a b\"");
    }

    #[test]
    fn display_escapes_embedded_quotes_and_empty_args() {
        let spec = CommandSpec::new("x", ["say \"hi\"", ""]);
        assert_eq!(spec.display(), "x \"say \\\"hi\\\"\" \"\"");
    }

    #[test]
    fn builder_helpers_set_fields() {
        let spec = CommandSpec::new("a", Vec::<String>::new())
            .with_timeout(Duration::from_secs(1))
            .with_env("K", "v")
            .with_cwd("/tmp");
        assert_eq!(spec.timeout, Duration::from_secs(1));
        assert_eq!(spec.env.get("K").map(String::as_str), Some("v"));
        assert_eq!(spec.cwd, Some(PathBuf::from("/tmp")));
    }

    #[test]
    fn success_requires_zero_exit_and_no_timeout() {
        let ok = CommandOutput {
            exit_code: Some(0),
            ..CommandOutput::default()
        };
        assert!(ok.success());
        let timed = CommandOutput {
            exit_code: Some(0),
            timed_out: true,
            ..CommandOutput::default()
        };
        assert!(!timed.success());
        assert!(!CommandOutput::default().success());
    }

    #[test]
    fn path_like_detection() {
        assert!(is_path_like("./node"));
        assert!(is_path_like("C:\\x\\npm.cmd"));
        assert!(!is_path_like("npm"));
    }

    #[test]
    fn expand_windows_env_substitutes_known_and_keeps_unknown() {
        let lookup = |name: &str| match name {
            "APPDATA" => Some(r"C:\Users\me\AppData\Roaming".to_owned()),
            "SystemRoot" => Some(r"C:\Windows".to_owned()),
            _ => None,
        };
        let cases = [
            (r"%APPDATA%\npm", r"C:\Users\me\AppData\Roaming\npm"),
            (
                r"%SystemRoot%\system32;%APPDATA%\npm",
                r"C:\Windows\system32;C:\Users\me\AppData\Roaming\npm",
            ),
            (r"%UNKNOWN%\bin", r"%UNKNOWN%\bin"),
            ("100%", "100%"),
            ("%%", "%%"),
            ("plain", "plain"),
            ("", ""),
        ];
        for (input, expected) in cases {
            assert_eq!(
                expand_windows_env(input, lookup),
                expected,
                "input {input:?}"
            );
        }
    }

    #[test]
    fn merge_windows_env_combines_path_and_lets_user_override() {
        let machine: BTreeMap<String, String> = [
            ("Path", r"C:\Windows;C:\Windows\System32"),
            ("TEMP", r"C:\Windows\Temp"),
            ("PATHEXT", ".COM;.EXE;.BAT;.CMD"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
        let user: BTreeMap<String, String> =
            [("Path", r"%APPDATA%\npm;"), ("temp", r"C:\Users\me\Temp")]
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect();
        let merged = merge_windows_env(&machine, &user);
        assert_eq!(
            merged.get("PATH").map(String::as_str),
            Some(r"C:\Windows;C:\Windows\System32;%APPDATA%\npm")
        );
        assert_eq!(
            merged.get("TEMP").map(String::as_str),
            Some(r"C:\Users\me\Temp")
        );
        assert!(
            !merged.contains_key("temp"),
            "case-insensitive override keeps first casing"
        );
        assert_eq!(
            merged.get("PATHEXT").map(String::as_str),
            Some(".COM;.EXE;.BAT;.CMD")
        );
    }

    #[test]
    fn merge_windows_env_handles_missing_paths() {
        let machine: BTreeMap<String, String> = [("Path".to_owned(), "C:\\a".to_owned())].into();
        let empty = BTreeMap::new();
        assert_eq!(
            merge_windows_env(&machine, &empty)
                .get("PATH")
                .map(String::as_str),
            Some("C:\\a")
        );
        assert_eq!(
            merge_windows_env(&empty, &machine)
                .get("PATH")
                .map(String::as_str),
            Some("C:\\a")
        );
        assert_eq!(
            merge_windows_env(&empty, &empty)
                .get("PATH")
                .map(String::as_str),
            Some("")
        );
    }

    #[test]
    fn parse_env_lines_keeps_assignments_only() {
        let text = "Last login: today\nPATH=/usr/bin:/bin\nFOO=a=b\n1BAD=x\nno_equals\nBASH_FUNC_x%%=() {  echo\n}\n_UNDER=1\n";
        let env = parse_env_lines(text);
        assert_eq!(env.get("PATH").map(String::as_str), Some("/usr/bin:/bin"));
        assert_eq!(env.get("FOO").map(String::as_str), Some("a=b"));
        assert_eq!(env.get("_UNDER").map(String::as_str), Some("1"));
        assert!(!env.contains_key("1BAD"));
        assert!(!env.contains_key("BASH_FUNC_x%%"));
        assert_eq!(env.len(), 3);
    }

    #[test]
    fn env_var_helpers_respect_platform_case_rules() {
        let mut env = BTreeMap::new();
        set_env_var(&mut env, "Path", "a".to_owned());
        set_env_var(&mut env, "PATH", "b".to_owned());
        if cfg!(windows) {
            assert_eq!(env.len(), 1);
            assert_eq!(get_env_var(&env, "path"), Some("b"));
        } else {
            assert_eq!(env.len(), 2);
            assert_eq!(get_env_var(&env, "PATH"), Some("b"));
            assert_eq!(get_env_var(&env, "path"), None);
        }
    }

    #[test]
    fn resolve_program_returns_none_for_unknown_binary() {
        assert!(resolve_program("definitely-not-a-real-binary-xyz-42").is_none());
        assert!(resolve_program_in("definitely-not-a-real-binary-xyz-42", "").is_none());
    }

    /// `<shell> <flag> "echo hi"` for the current platform, or `None` when the shell is missing.
    fn echo_spec() -> Option<CommandSpec> {
        if cfg!(windows) {
            resolve_program("cmd").map(|_| CommandSpec::new("cmd", ["/c", "echo hi"]))
        } else {
            resolve_program("sh").map(|_| CommandSpec::new("sh", ["-c", "echo hi"]))
        }
    }

    /// A command that runs for several seconds, or `None` when unavailable.
    fn slow_spec() -> Option<CommandSpec> {
        if cfg!(windows) {
            resolve_program("ping").map(|_| CommandSpec::new("ping", ["-n", "6", "127.0.0.1"]))
        } else {
            resolve_program("sleep").map(|_| CommandSpec::new("sleep", ["5"]))
        }
    }

    #[tokio::test]
    async fn run_captures_stdout_and_exit_code() {
        let Some(spec) = echo_spec() else { return };
        let out = run(&spec).await.expect("spawn");
        assert!(out.success(), "{out:?}");
        assert_eq!(out.stdout.trim(), "hi");
        assert!(!out.timed_out);
    }

    #[tokio::test]
    async fn run_reports_missing_program_as_command_not_found() {
        let spec = CommandSpec::new("definitely-not-a-real-binary-xyz-42", ["--version"]);
        match run(&spec).await {
            Err(AppError::CommandNotFound { program }) => {
                assert_eq!(program, "definitely-not-a-real-binary-xyz-42");
            }
            other => panic!("expected CommandNotFound, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn run_times_out_slow_command_without_error() {
        let Some(spec) = slow_spec() else { return };
        let spec = spec.with_timeout(Duration::from_millis(400));
        let started = Instant::now();
        let out = run(&spec).await.expect("spawn");
        assert!(out.timed_out, "{out:?}");
        assert_eq!(out.exit_code, None);
        assert!(!out.success());
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "kill must not wait for the child"
        );
    }

    #[tokio::test]
    async fn run_streaming_delivers_lines() {
        let Some(spec) = echo_spec() else { return };
        let (_tx, rx) = watch::channel(false);
        let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = lines.clone();
        let out = run_streaming(&spec, rx, move |stream, line| {
            if let Ok(mut l) = sink.lock() {
                l.push((stream, line));
            }
        })
        .await
        .expect("spawn");
        assert!(out.success(), "{out:?}");
        let lines = lines.lock().expect("lock");
        assert!(
            lines
                .iter()
                .any(|(s, l)| *s == OutputStream::Stdout && l.trim() == "hi"),
            "{lines:?}"
        );
    }

    #[tokio::test]
    async fn run_streaming_honours_cancel() {
        let Some(spec) = slow_spec() else { return };
        let (tx, rx) = watch::channel(false);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let _ = tx.send(true);
        });
        let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = lines.clone();
        let started = Instant::now();
        let out = run_streaming(&spec, rx, move |stream, line| {
            if let Ok(mut l) = sink.lock() {
                l.push((stream, line));
            }
        })
        .await
        .expect("spawn");
        assert_eq!(out.exit_code, None);
        assert!(!out.timed_out);
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "cancel must kill promptly"
        );
        let lines = lines.lock().expect("lock");
        assert!(
            lines.contains(&(OutputStream::System, "cancelled".to_owned())),
            "{lines:?}"
        );
    }

    #[tokio::test]
    async fn fresh_session_env_has_path() {
        let env = fresh_session_env().await.expect("fresh env");
        let path = get_env_var(&env, "PATH").expect("PATH present");
        assert!(!path.is_empty());
        if cfg!(windows) {
            assert!(!path.contains('%'), "PATH must be expanded: {path}");
            assert!(get_env_var(&env, "SystemRoot").is_some());
        }
    }

    #[tokio::test]
    async fn run_in_fresh_session_runs_shell_from_fresh_path() {
        let Some(spec) = echo_spec() else { return };
        let out = run_in_fresh_session(&spec).await.expect("fresh run");
        assert!(out.success(), "{out:?}");
        assert_eq!(out.stdout.trim(), "hi");
    }

    /// Windows: a bare `npm` must resolve to the `npm.cmd` shim without a shell (skipped when
    /// npm is not installed).
    #[tokio::test]
    async fn run_resolves_windows_cmd_shims() {
        if !cfg!(windows) || resolve_program("npm").is_none() {
            return;
        }
        let spec = CommandSpec::new("npm", ["--version"]);
        let out = run(&spec).await.expect("spawn npm");
        assert!(out.success(), "{out:?}");
        assert!(out.stdout.trim().split('.').count() >= 2, "{out:?}");
    }

    #[tokio::test]
    async fn run_in_fresh_session_rejects_unknown_program() {
        let spec = CommandSpec::new("definitely-not-a-real-binary-xyz-42", ["--version"]);
        assert!(matches!(
            run_in_fresh_session(&spec).await,
            Err(AppError::CommandNotFound { .. })
        ));
    }
}
