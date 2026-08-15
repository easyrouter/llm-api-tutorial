//! M4 — verification. Runs the CLI in a *fresh session* environment (as a newly opened
//! terminal would see it), optionally probes the gateway with the user's key (held in memory
//! only for the duration of the request; never logged), lists still-running terminal processes
//! (guide fault C: "close all terminal windows"), and feeds any failure straight into M5.
//!
//! What each entry point does
//! - [`running_terminals`]: one `sysinfo` process scan with the cheapest refresh kind. A
//!   process counts as a terminal when its name matches the platform list (case-insensitive,
//!   `.exe` optional) — Windows: `WindowsTerminal.exe`, `wt.exe`, `cmd.exe`, `powershell.exe`,
//!   `pwsh.exe`, `mintty.exe`, `git-bash.exe`, `alacritty.exe`, `Hyper.exe`, `wezterm-gui.exe`,
//!   `Cursor.exe`, `Code.exe` (VS Code integrated terminal, reported as `Visual Studio Code`);
//!   `conhost.exe` / `OpenConsole.exe` are *not* counted (they host a window, they are not the
//!   session). macOS: `Terminal`, `iTerm2`, `Warp`, `Hyper`, `Alacritty`, `kitty`, `WezTerm`,
//!   `Ghostty`, `Cursor`, VS Code — recognised through the `.app` bundle of the main executable
//!   (`…/Visual Studio Code.app/Contents/MacOS/Electron`, `…/Warp.app/Contents/MacOS/stable`),
//!   so helper/renderer processes are excluded. Our own process and its descendants (e.g. the
//!   `cmd.exe` behind an npm shim we spawned) are excluded; a process whose parent is the same
//!   app (VS Code renderers, nested `cmd.exe`) is folded into that parent so one window is one
//!   row; entries are deduplicated by `(name, pid)`, sorted, and capped at [`MAX_TERMINALS`].
//!   Names are shown verbatim (they are not secrets).
//! - [`probe_gateway`]: one minimal request per protocol —
//!     responses:        `POST {base}/responses`        `{"model": m, "input": "ping", "max_output_tokens": 16}`
//!     chat_completions: `POST {base}/chat/completions` `{"model": m, "messages":[{"role":"user","content":"ping"}], "max_tokens": 1}`
//!   (`max_output_tokens` is 16 because OpenAI-compatible Responses endpoints reject smaller
//!   values with a 400 that would look like a protocol mismatch). `Authorization: Bearer <key>`
//!   is the only place the key goes; [`GATEWAY_TIMEOUT`] bounds the whole exchange. Status
//!   mapping lives in the pure [`classify_response`]: 2xx → ok; 400/422 mentioning `model`
//!   (unknown model — auth and URL are proven) → ok with message; other 400/422 →
//!   `ProtocolMismatch`; 401/403 → `Auth`; 404/405/410 → `NotFound`; 429 → `RateLimited`;
//!   5xx → `ServerError`; transport errors → `Timeout` / `Network` / `Tls`
//!   ([`transport_error_class`]). `message` is the server's error text (or body prefix), scrubbed
//!   of the key itself — including partially masked echoes such as `sk-abcde***6789`
//!   ([`scrub_known_secret`]) — passed through [`redact_secrets`] and truncated to
//!   [`MAX_MESSAGE_CHARS`] characters.
//! - [`verify`]: resolves the tool's `ToolSpec`, runs `<binary> <version_args>` against the
//!   fresh-session environment (`process::fresh_session_env`), probes the gateway when
//!   requested and scans terminals — all three concurrently — then derives [`Symptom`]s
//!   ([`symptoms_from`]) and asks `diagnose::diagnose` for the matching guide faults.
//!   `CliCheck.path` is the executable a *fresh* terminal resolves; when the CLI is missing there
//!   but present on the current `PATH` / npm global bin, `path` still names that location while
//!   `error_class` stays `CommandNotFound` (guide fault E: PATH not refreshed).
//!
//! Nothing is persisted and nothing outside the app process is written (ADR-0003).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use regex::Regex;
use serde_json::{json, Value};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::diagnose;
use crate::error::AppError;
use crate::models::{
    AppConfig, CliCheck, DiagnoseRequest, ErrorClass, GatewayCheck, GatewayProbeRequest, Platform,
    Protocol, Symptom, TerminalProcess, ToolId, ToolSpec, VerifyRequest, VerifyResult,
};
use crate::net;
use crate::platform;
use crate::process::{self, CommandOutput, CommandSpec};
use crate::redact::{redact_secrets, tail_redacted};

/// Time allowed for `<binary> --version` in the fresh session.
pub const CLI_TIMEOUT: Duration = Duration::from_secs(12);
/// Overall timeout of the gateway probe (connect + headers + body).
pub const GATEWAY_TIMEOUT: Duration = Duration::from_secs(15);
/// Upper bound on reported terminal processes.
pub const MAX_TERMINALS: usize = 50;
/// Maximum characters of a server message kept in `GatewayCheck::message`.
pub const MAX_MESSAGE_CHARS: usize = 300;
/// Maximum bytes read from a gateway response body (error pages can be large).
const MAX_BODY_BYTES: usize = 16 * 1024;
/// Lines of CLI output kept (redacted) when the CLI check fails.
const OUTPUT_TAIL_LINES: usize = 5;
/// Smallest `max_output_tokens` accepted by OpenAI-compatible Responses endpoints.
const RESPONSES_MIN_OUTPUT_TOKENS: u32 = 16;
/// Secrets shorter than this are not scrubbed literally (they would match too much).
const MIN_SCRUB_LEN: usize = 4;

// ---------------------------------------------------------------------------
// Running terminals
// ---------------------------------------------------------------------------

/// A running process as seen by the scanner — the pure input of [`collect_terminals`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessEntry {
    pub pid: u32,
    pub parent: Option<u32>,
    /// Process name as reported by the OS (`cmd.exe`, `Terminal`, `Electron`, …).
    pub name: String,
    /// Full executable path when the platform provides it cheaply (macOS only).
    pub exe: Option<String>,
}

/// Lower-case executable stems (no `.exe`) that count as a terminal per platform.
fn terminal_stems(platform: Platform) -> &'static [&'static str] {
    match platform {
        Platform::Windows => &[
            "windowsterminal",
            "wt",
            "cmd",
            "powershell",
            "pwsh",
            "mintty",
            "git-bash",
            "alacritty",
            "hyper",
            "wezterm-gui",
            "code",
            "cursor",
        ],
        Platform::Macos => &[
            "terminal",
            "iterm2",
            "warp",
            "hyper",
            "alacritty",
            "kitty",
            "wezterm",
            "wezterm-gui",
            "ghostty",
            "code",
            "cursor",
        ],
        Platform::Linux => &[
            "gnome-terminal-server",
            "konsole",
            "xterm",
            "tilix",
            "xfce4-terminal",
            "alacritty",
            "kitty",
            "wezterm-gui",
            "ghostty",
            "code",
            "cursor",
        ],
        Platform::Unknown => &[],
    }
}

/// macOS app bundles whose *main* executable is a terminal (or an editor with an integrated
/// terminal), with the name shown to the user.
const MACOS_BUNDLES: &[(&str, &str)] = &[
    ("Terminal.app", "Terminal"),
    ("iTerm.app", "iTerm2"),
    ("Warp.app", "Warp"),
    ("Hyper.app", "Hyper"),
    ("Alacritty.app", "Alacritty"),
    ("kitty.app", "kitty"),
    ("WezTerm.app", "WezTerm"),
    ("Ghostty.app", "Ghostty"),
    ("Visual Studio Code.app", "Visual Studio Code"),
    ("Cursor.app", "Cursor"),
];

/// Display name for VS Code's process (`Code.exe` / `Code`).
const VS_CODE: &str = "Visual Studio Code";

/// Lower-cases a process name and strips a trailing `.exe`.
fn normalize_process_name(name: &str) -> String {
    let lower = name.trim().to_ascii_lowercase();
    match lower.strip_suffix(".exe") {
        Some(stem) => stem.to_owned(),
        None => lower,
    }
}

/// `true` when `name` is a terminal (or terminal host) process on `platform`. Pure.
pub fn is_terminal_process(name: &str, platform: Platform) -> bool {
    terminal_display_name(name, platform).is_some()
}

/// Name shown for a terminal process (verbatim, except VS Code which is spelled out), or
/// `None` when `name` is not a terminal on `platform`. Pure.
pub fn terminal_display_name(name: &str, platform: Platform) -> Option<String> {
    let stem = normalize_process_name(name);
    if !terminal_stems(platform).contains(&stem.as_str()) {
        return None;
    }
    Some(if stem == "code" {
        VS_CODE.to_owned()
    } else {
        name.trim().to_owned()
    })
}

/// Display name when `exe_path` is the *main* executable of a known macOS terminal bundle
/// (`/<Bundle>.app/Contents/MacOS/<exe>`); helper apps nested inside the bundle do not match.
pub fn macos_bundle_display_name(exe_path: &str) -> Option<&'static str> {
    MACOS_BUNDLES
        .iter()
        .find(|(bundle, _)| is_main_executable_of(exe_path, bundle))
        .map(|(_, name)| *name)
}

fn is_main_executable_of(exe_path: &str, bundle: &str) -> bool {
    let marker = format!("/{bundle}/Contents/MacOS/");
    exe_path
        .find(&marker)
        .is_some_and(|i| !exe_path[i + marker.len()..].contains('/'))
}

/// Combines bundle (macOS) and name matching. Pure.
pub fn classify_process(name: &str, exe: Option<&str>, platform: Platform) -> Option<String> {
    if platform == Platform::Macos {
        if let Some(display) = exe.and_then(macos_bundle_display_name) {
            return Some(display.to_owned());
        }
    }
    terminal_display_name(name, platform)
}

/// Pids of `own_pid` and all its descendants (processes we spawned are not user terminals).
fn own_process_tree(entries: &[ProcessEntry], own_pid: Option<u32>) -> BTreeSet<u32> {
    let mut tree = BTreeSet::new();
    let Some(own) = own_pid else {
        return tree;
    };
    tree.insert(own);
    loop {
        let before = tree.len();
        for e in entries {
            if e.parent.is_some_and(|p| tree.contains(&p)) {
                tree.insert(e.pid);
            }
        }
        if tree.len() == before {
            return tree;
        }
    }
}

/// Filters, names, deduplicates (by pid), sorts and caps the terminal list. A process whose
/// parent is the same app (VS Code renderers / extension hosts, nested `cmd.exe`) is folded
/// into its parent so one window shows up once. Pure.
pub fn collect_terminals(
    entries: &[ProcessEntry],
    own_pid: Option<u32>,
    platform: Platform,
) -> Vec<TerminalProcess> {
    let excluded = own_process_tree(entries, own_pid);
    // keyed by pid → duplicate entries collapse here
    let named: BTreeMap<u32, (Option<u32>, String)> = entries
        .iter()
        .filter(|e| !excluded.contains(&e.pid))
        .filter_map(|e| {
            classify_process(&e.name, e.exe.as_deref(), platform)
                .map(|name| (e.pid, (e.parent, name)))
        })
        .collect();
    let same_app_as_parent = |parent: Option<u32>, name: &str| {
        parent
            .and_then(|p| named.get(&p))
            .is_some_and(|(_, parent_name)| parent_name == name)
    };
    let mut out: Vec<TerminalProcess> = named
        .iter()
        .filter(|(_, (parent, name))| !same_app_as_parent(*parent, name))
        .map(|(pid, (_, name))| TerminalProcess {
            pid: *pid,
            name: name.clone(),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name).then(a.pid.cmp(&b.pid)));
    out.truncate(MAX_TERMINALS);
    out
}

/// Refresh kind: pid/parent/name only; the executable path is fetched on macOS, where it is
/// cheap and needed for bundle matching.
fn process_refresh_kind() -> ProcessRefreshKind {
    let kind = ProcessRefreshKind::nothing();
    if cfg!(target_os = "macos") {
        kind.with_exe(UpdateKind::OnlyIfNotSet)
    } else {
        kind
    }
}

/// Snapshot of running processes (blocking; ~tens of milliseconds).
fn scan_processes() -> Vec<ProcessEntry> {
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, process_refresh_kind());
    sys.processes()
        .values()
        .map(|p| ProcessEntry {
            pid: p.pid().as_u32(),
            parent: p.parent().map(sysinfo::Pid::as_u32),
            name: p.name().to_string_lossy().into_owned(),
            exe: p.exe().map(|e| e.to_string_lossy().into_owned()),
        })
        .collect()
}

/// Terminal (and terminal-hosting editor) processes currently running, excluding this app and
/// its children. Blocking — call from a sync command or `spawn_blocking`.
pub fn running_terminals() -> Vec<TerminalProcess> {
    let entries = scan_processes();
    collect_terminals(&entries, Some(std::process::id()), platform::platform())
}

// ---------------------------------------------------------------------------
// Gateway probe
// ---------------------------------------------------------------------------

/// Endpoint of the probe: `base_url` trimmed of whitespace, a literal `#` suffix and trailing
/// slashes, plus the protocol path — unless the base already ends with that path. Pure.
pub fn probe_url(base_url: &str, protocol: Protocol) -> String {
    let base = base_url.trim().trim_end_matches('#').trim_end_matches('/');
    let path = match protocol {
        Protocol::Responses => "/responses",
        Protocol::ChatCompletions => "/chat/completions",
    };
    if base.ends_with(path) {
        base.to_owned()
    } else {
        format!("{base}{path}")
    }
}

/// Minimal request body per protocol (`model` sent as given, empty allowed). Pure.
pub fn probe_body(model: &str, protocol: Protocol) -> Value {
    match protocol {
        Protocol::Responses => json!({
            "model": model,
            "input": "ping",
            "max_output_tokens": RESPONSES_MIN_OUTPUT_TOKENS,
        }),
        Protocol::ChatCompletions => json!({
            "model": model,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 1,
        }),
    }
}

/// Replaces occurrences of a known secret with `[REDACTED]` — literal ones and *partially
/// masked echoes* the server produces itself (`sk-abcde***6789`: head + `*`s + tail of the
/// key), which pattern-based redaction cannot recognise. Belt and braces on top of
/// [`redact_secrets`]: company keys may not match any known prefix. Pure.
pub fn scrub_known_secret(text: &str, secret: &str) -> String {
    let secret = secret.trim();
    if secret.chars().count() < MIN_SCRUB_LEN {
        return text.to_owned();
    }
    let mut out = text.replace(secret, "[REDACTED]");
    let echoes: BTreeSet<&str> = out
        .split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '*')))
        .filter(|token| is_masked_echo_of(token, secret))
        .collect();
    let echoes: Vec<String> = echoes.into_iter().map(str::to_owned).collect();
    for echo in echoes {
        out = out.replace(&echo, "[REDACTED]");
    }
    out
}

/// `true` when `token` looks like `<head>***<tail>` where `head`/`tail` are the beginning and
/// end of `secret` (at least [`MIN_SCRUB_LEN`] known characters in total, head non-empty).
fn is_masked_echo_of(token: &str, secret: &str) -> bool {
    let (Some(first), Some(last)) = (token.find('*'), token.rfind('*')) else {
        return false;
    };
    let head = &token[..first];
    let tail = &token[last + 1..];
    !head.is_empty()
        && head.len() + tail.len() >= MIN_SCRUB_LEN
        && secret.starts_with(head)
        && secret.ends_with(tail)
}

/// `true` when a 400/422 body talks about the model (unknown/invalid model ⇒ auth and URL are
/// fine, only the model hint is wrong).
fn mentions_model(body: &str) -> bool {
    body.to_ascii_lowercase().contains("model")
}

/// `error.message` / `error` / `message` / `detail` from a JSON error body, if any.
fn extract_error_text(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    ["/error/message", "/error", "/message", "/detail"]
        .iter()
        .filter_map(|pointer| value.pointer(pointer))
        .find_map(|v| v.as_str().map(str::to_owned))
        .filter(|s| !s.trim().is_empty())
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

/// Redacted, whitespace-collapsed, truncated message derived from a response body. Pure.
pub fn server_message(body: &str) -> Option<String> {
    let text = extract_error_text(body).unwrap_or_else(|| body.to_owned());
    let redacted = redact_secrets(&text);
    let compact = redacted.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        None
    } else {
        Some(truncate_chars(&compact, MAX_MESSAGE_CHARS))
    }
}

/// Maps an HTTP status + body to a [`GatewayCheck`] (`latency_ms` left `None`). Pure.
pub fn classify_response(status: u16, body: &str) -> GatewayCheck {
    let (ok, error_class) = match status {
        200..=299 => (true, None),
        400 | 422 if mentions_model(body) => (true, None),
        400 | 422 => (false, Some(ErrorClass::ProtocolMismatch)),
        401 | 403 => (false, Some(ErrorClass::Auth)),
        404 | 405 | 410 => (false, Some(ErrorClass::NotFound)),
        407 => (false, Some(ErrorClass::Network)),
        408 => (false, Some(ErrorClass::Timeout)),
        429 => (false, Some(ErrorClass::RateLimited)),
        500..=599 => (false, Some(ErrorClass::ServerError)),
        _ => (false, Some(ErrorClass::Unknown)),
    };
    let message = if (200..300).contains(&status) {
        None
    } else {
        server_message(body)
    };
    GatewayCheck {
        ok,
        http_status: Some(status),
        latency_ms: None,
        error_class,
        message,
    }
}

/// Classifies a transport-level failure (no HTTP response) from reqwest's timeout flag and its
/// (redacted) detail: TLS-ish wording wins, then timeout, everything else (connect, DNS,
/// redirect loops, resets) is `Network`. Pure.
pub fn transport_error_class(timed_out: bool, detail: &str) -> ErrorClass {
    let lower = detail.to_ascii_lowercase();
    let tls = ["certificate", "tls", "ssl", "handshake"]
        .iter()
        .any(|needle| lower.contains(needle));
    if tls {
        ErrorClass::Tls
    } else if timed_out {
        ErrorClass::Timeout
    } else {
        ErrorClass::Network
    }
}

fn classify_transport_error(e: &reqwest::Error) -> GatewayCheck {
    let detail = net::short_error(e);
    let error_class = if e.is_builder() {
        ErrorClass::Unknown
    } else {
        transport_error_class(e.is_timeout(), &detail)
    };
    GatewayCheck {
        ok: false,
        http_status: None,
        latency_ms: None,
        error_class: Some(error_class),
        message: Some(detail),
    }
}

/// Reads at most [`MAX_BODY_BYTES`] of the body (lossy UTF-8); read errors end the body early.
async fn read_body_prefix(resp: reqwest::Response) -> String {
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::with_capacity(1024);
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else { break };
        buf.extend_from_slice(&chunk);
        if buf.len() >= MAX_BODY_BYTES {
            buf.truncate(MAX_BODY_BYTES);
            break;
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

fn elapsed_ms(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// One minimal request against the gateway. The key travels in the `Authorization` header
/// only and is never logged; the response body is scrubbed before it becomes `message`.
pub async fn probe_gateway(client: &reqwest::Client, req: &GatewayProbeRequest) -> GatewayCheck {
    let url = probe_url(&req.base_url, req.protocol);
    let body = probe_body(&req.model, req.protocol);
    let started = Instant::now();
    let sent = client
        .post(&url)
        .bearer_auth(req.api_key.trim())
        .timeout(GATEWAY_TIMEOUT)
        .json(&body)
        .send()
        .await;
    let mut check = match sent {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let text = scrub_known_secret(&read_body_prefix(resp).await, &req.api_key);
            classify_response(status, &text)
        }
        Err(e) => classify_transport_error(&e),
    };
    check.latency_ms = Some(elapsed_ms(started));
    log::debug!(
        "gateway probe: status={:?} class={:?} latency={:?}ms",
        check.http_status,
        check.error_class,
        check.latency_ms
    );
    check
}

// ---------------------------------------------------------------------------
// CLI check (fresh session)
// ---------------------------------------------------------------------------

/// First `x.y[.z][-pre]` token in CLI output (`codex-cli 0.42.0`, `1.0.5 (Claude Code)`,
/// `v22.1.0`). Pure.
pub fn parse_version(output: &str) -> Option<String> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    let re = RE
        .get_or_init(|| {
            Regex::new(r"(?:^|[^0-9A-Za-z.])[vV]?(\d+\.\d+(?:\.\d+)?(?:[-+][0-9A-Za-z.+\-]+)?)")
                .ok()
        })
        .as_ref()?;
    re.captures(output)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_owned())
}

fn cli_check(ok: bool, error_class: Option<ErrorClass>) -> CliCheck {
    CliCheck {
        ok,
        version: None,
        path: None,
        output_tail: None,
        error_class,
    }
}

/// Redacted tail of stdout + stderr (stderr last), `None` when there is no output.
fn output_tail(out: &CommandOutput) -> Option<String> {
    let combined = format!("{}\n{}", out.stdout.trim_end(), out.stderr.trim_end());
    let tail = tail_redacted(combined.trim(), OUTPUT_TAIL_LINES);
    if tail.trim().is_empty() {
        None
    } else {
        Some(tail)
    }
}

/// [`CliCheck`] from a finished `<binary> --version` run at `path`. Pure.
pub fn cli_check_from_output(path: &str, out: &CommandOutput) -> CliCheck {
    let mut check = if out.timed_out {
        cli_check(false, Some(ErrorClass::Timeout))
    } else if out.success() {
        cli_check(true, None)
    } else {
        cli_check(false, Some(ErrorClass::CommandFailed))
    };
    check.path = Some(path.to_owned());
    if check.ok {
        check.version = parse_version(&out.stdout).or_else(|| parse_version(&out.stderr));
    } else {
        check.output_tail = output_tail(out);
    }
    check
}

/// Fresh-session environment, falling back to the current process environment (logged) when
/// it cannot be built — a verification against the current session is still worth more than
/// nothing.
async fn fresh_env_or_process() -> BTreeMap<String, String> {
    match process::fresh_session_env().await {
        Ok(env) => env,
        Err(e) => {
            log::warn!(
                "verify: cannot build fresh-session environment ({}); using process environment",
                redact_secrets(&e.to_string())
            );
            std::env::vars().collect()
        }
    }
}

/// `CommandNotFound` result. `path` names the binary when it exists on the *current* `PATH` or
/// in the npm global bin directory (installed, but a fresh terminal cannot see it — fault E).
fn cli_not_found(binary: &str) -> CliCheck {
    let extra: Vec<_> = platform::npm_global_prefix_guess()
        .map(|prefix| platform::npm_global_bin_dir(&prefix))
        .into_iter()
        .collect();
    let mut check = cli_check(false, Some(ErrorClass::CommandNotFound));
    check.path =
        platform::find_binary(binary, &extra).map(|(p, _)| p.to_string_lossy().into_owned());
    check
}

/// Runs `<binary> <version_args>` the way a freshly opened terminal would.
async fn check_cli(spec: &ToolSpec) -> CliCheck {
    let env = fresh_env_or_process().await;
    let fresh_path = process::get_env_var(&env, "PATH").unwrap_or_default();
    let Some(program) = process::resolve_program_in(&spec.binary, fresh_path) else {
        return cli_not_found(&spec.binary);
    };
    let path = program.to_string_lossy().into_owned();
    let command = CommandSpec {
        program: path.clone(),
        args: spec.version_args.clone(),
        env,
        clear_env: true,
        cwd: None,
        timeout: CLI_TIMEOUT,
    };
    match process::run(&command).await {
        Ok(out) => cli_check_from_output(&path, &out),
        Err(AppError::CommandNotFound { .. }) => {
            let mut check = cli_check(false, Some(ErrorClass::CommandNotFound));
            check.path = Some(path);
            check
        }
        Err(e) => {
            let mut check = cli_check(false, Some(ErrorClass::CommandFailed));
            check.path = Some(path);
            check.output_tail = Some(redact_secrets(&e.to_string()));
            check
        }
    }
}

// ---------------------------------------------------------------------------
// verify
// ---------------------------------------------------------------------------

fn find_tool(config: &AppConfig, tool: ToolId) -> Option<&ToolSpec> {
    config.tools.iter().find(|t| t.id == tool)
}

/// Overall verdict: CLI ok and (when probed) gateway ok. Running terminals only warn. Pure.
pub fn overall_ok(cli: &CliCheck, gateway: Option<&GatewayCheck>) -> bool {
    cli.ok && gateway.is_none_or(|g| g.ok)
}

/// Facts for the rule engine. `target` labels network symptoms (redacted gateway URL). Pure.
pub fn symptoms_from(
    tool: ToolId,
    cli: &CliCheck,
    gateway: Option<&GatewayCheck>,
    target: &str,
) -> Vec<Symptom> {
    let mut symptoms = Vec::new();
    match cli.error_class {
        Some(ErrorClass::CommandNotFound) => symptoms.push(Symptom::CommandNotFound { tool }),
        Some(ErrorClass::CommandFailed | ErrorClass::Timeout) => {
            symptoms.push(Symptom::CommandFailed {
                tool,
                output_tail: cli.output_tail.clone().unwrap_or_default(),
            });
        }
        _ => {}
    }
    if let Some(g) = gateway.filter(|g| !g.ok) {
        let target = target.to_owned();
        match (g.error_class, g.http_status) {
            (Some(ErrorClass::ProtocolMismatch), _) => {
                symptoms.push(Symptom::ProtocolMismatch { tool });
            }
            (Some(ErrorClass::Network | ErrorClass::Tls), _) => {
                symptoms.push(Symptom::NetworkError { target });
            }
            (Some(ErrorClass::Timeout), None) => symptoms.push(Symptom::Timeout { target }),
            (_, Some(status)) => symptoms.push(Symptom::HttpStatus { status, tool }),
            (_, None) => {}
        }
    }
    symptoms
}

/// Full M4 verification for one tool (see module docs). Never fails: every problem is data.
pub async fn verify(
    client: &reqwest::Client,
    config: &AppConfig,
    req: VerifyRequest,
) -> VerifyResult {
    let tool = req.tool;
    let cli_fut = async {
        if let Some(spec) = find_tool(config, tool) {
            check_cli(spec).await
        } else {
            log::warn!("verify: no ToolSpec for {tool:?} in app config");
            cli_check(false, Some(ErrorClass::Unknown))
        }
    };
    let gateway_fut = async {
        match &req.gateway {
            Some(g) => Some(probe_gateway(client, g).await),
            None => None,
        }
    };
    let terminals_fut = tokio::task::spawn_blocking(running_terminals);
    let (cli, gateway, terminals) = tokio::join!(cli_fut, gateway_fut, terminals_fut);
    let running_terminals = terminals.unwrap_or_else(|e| {
        log::warn!("verify: terminal scan panicked: {e}");
        Vec::new()
    });

    let target = req
        .gateway
        .as_ref()
        .map(|g| redact_secrets(g.base_url.trim()))
        .unwrap_or_default();
    let symptoms = symptoms_from(tool, &cli, gateway.as_ref(), &target);
    let diagnoses = if symptoms.is_empty() {
        Vec::new()
    } else {
        diagnose::diagnose(
            &DiagnoseRequest {
                symptoms,
                snapshot: None,
            },
            config,
        )
    };
    let ok = overall_ok(&cli, gateway.as_ref());
    VerifyResult {
        tool,
        cli,
        gateway,
        running_terminals,
        ok,
        diagnoses,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread::JoinHandle;

    use super::*;

    const KEY: &str = "sk-abcdefghijklmnopqrstuvwxyz0123456789";

    // ----- gateway: classification -----------------------------------------------------

    #[test]
    fn classify_response_table() {
        let cases: &[(u16, &str, bool, Option<ErrorClass>)] = &[
            (200, r#"{"id":"resp_1","output":[]}"#, true, None),
            (201, "", true, None),
            (
                400,
                r#"{"error":{"message":"The model `x` does not exist"}}"#,
                true,
                None,
            ),
            (
                422,
                r#"{"error":{"message":"Unsupported model"}}"#,
                true,
                None,
            ),
            (
                400,
                r#"{"error":{"message":"you must provide messages"}}"#,
                false,
                Some(ErrorClass::ProtocolMismatch),
            ),
            (
                422,
                "Unprocessable Entity",
                false,
                Some(ErrorClass::ProtocolMismatch),
            ),
            (
                401,
                r#"{"error":{"message":"Incorrect API key"}}"#,
                false,
                Some(ErrorClass::Auth),
            ),
            (403, "forbidden", false, Some(ErrorClass::Auth)),
            (
                404,
                "<html>Not Found</html>",
                false,
                Some(ErrorClass::NotFound),
            ),
            (405, "", false, Some(ErrorClass::NotFound)),
            (407, "", false, Some(ErrorClass::Network)),
            (408, "", false, Some(ErrorClass::Timeout)),
            (
                429,
                r#"{"error":{"message":"Rate limit"}}"#,
                false,
                Some(ErrorClass::RateLimited),
            ),
            (500, "", false, Some(ErrorClass::ServerError)),
            (502, "Bad Gateway", false, Some(ErrorClass::ServerError)),
            (503, "", false, Some(ErrorClass::ServerError)),
            (418, "teapot", false, Some(ErrorClass::Unknown)),
            (302, "", false, Some(ErrorClass::Unknown)),
        ];
        for (status, body, ok, class) in cases {
            let check = classify_response(*status, body);
            assert_eq!(check.ok, *ok, "status {status} ok");
            assert_eq!(check.error_class, *class, "status {status} class");
            assert_eq!(check.http_status, Some(*status));
            assert_eq!(check.latency_ms, None);
        }
    }

    #[test]
    fn success_has_no_message_but_model_400_does() {
        assert_eq!(classify_response(200, "hello").message, None);
        let check = classify_response(400, r#"{"error":{"message":"Unknown model gpt-x"}}"#);
        assert!(check.ok);
        assert_eq!(check.message.as_deref(), Some("Unknown model gpt-x"));
    }

    #[test]
    fn message_extracts_json_error_text_and_collapses_whitespace() {
        let body = "{\n  \"error\": {\n    \"message\": \"Incorrect   API\\nkey\",\n    \"type\": \"x\"\n  }\n}";
        assert_eq!(server_message(body).as_deref(), Some("Incorrect API key"));
        assert_eq!(
            server_message(r#"{"detail":"nope"}"#).as_deref(),
            Some("nope")
        );
        assert_eq!(
            server_message(r#"{"error":"plain"}"#).as_deref(),
            Some("plain")
        );
        assert_eq!(
            server_message("  raw   text\n\tmore ").as_deref(),
            Some("raw text more")
        );
        assert_eq!(server_message(""), None);
        assert_eq!(server_message("   \n "), None);
        assert_eq!(
            server_message(r#"{"error":{"message":"   "}}"#).as_deref(),
            Some(r#"{"error":{"message":" "}}"#)
        );
    }

    #[test]
    fn message_is_truncated_to_max_chars() {
        let body = "é".repeat(MAX_MESSAGE_CHARS + 50);
        let msg = server_message(&body).unwrap();
        assert_eq!(msg.chars().count(), MAX_MESSAGE_CHARS + 1);
        assert!(msg.ends_with('…'));
    }

    #[test]
    fn body_echoing_key_is_redacted() {
        let body = format!(
            r#"{{"error":{{"message":"Incorrect API key provided: {KEY}. You can find..."}}}}"#
        );
        let check = classify_response(401, &body);
        let msg = check.message.unwrap();
        assert!(!msg.contains(KEY), "{msg}");
        assert!(msg.contains("[REDACTED]"), "{msg}");
        // raw (non-JSON) body echoing the key
        let raw = classify_response(500, &format!("upstream rejected {KEY} for tenant"));
        assert!(!raw.message.unwrap().contains(KEY));
    }

    #[test]
    fn scrub_known_secret_removes_arbitrary_keys() {
        let key = "company-key-XYZ-9876";
        let body = format!("bad key {key} used twice {key}");
        let scrubbed = scrub_known_secret(&body, &format!("  {key} "));
        assert!(!scrubbed.contains(key));
        assert_eq!(scrubbed.matches("[REDACTED]").count(), 2);
        // too short to scrub safely — left alone
        assert_eq!(scrub_known_secret("abc abc", "abc"), "abc abc");
        assert_eq!(scrub_known_secret("text", ""), "text");
        // scrubbed text survives classification with no trace of the key
        let check = classify_response(401, &scrubbed);
        assert!(!check.message.unwrap().contains(key));
    }

    #[test]
    fn scrub_known_secret_masks_partial_echoes() {
        // OpenAI style: "Incorrect API key provided: sk-abcde***************6789."
        let body = format!(
            "Incorrect API key provided: {}****************{}. Check it.",
            &KEY[..8],
            &KEY[KEY.len() - 4..]
        );
        let scrubbed = scrub_known_secret(&body, KEY);
        assert_eq!(
            scrubbed,
            "Incorrect API key provided: [REDACTED]. Check it."
        );
        // mask_value-style echo (3 head + 4 tail) inside JSON quotes
        let json = format!(
            r#"{{"error":"bad key \"{}****{}\""}}"#,
            &KEY[..3],
            &KEY[KEY.len() - 4..]
        );
        assert!(!scrub_known_secret(&json, KEY).contains(&KEY[KEY.len() - 4..]));
        // not echoes of this key: different head/tail, too few known chars, all stars
        for keep in [
            "sk-zzzzz****6789",
            "sk-****",
            "*****",
            "abc***",
            "sk-a***9999",
        ] {
            assert_eq!(scrub_known_secret(keep, KEY), keep, "{keep}");
        }
        assert!(!is_masked_echo_of("nostars", KEY));
        // end to end: the message never contains head or tail fragments
        let msg = classify_response(401, &scrub_known_secret(&body, KEY))
            .message
            .unwrap();
        assert!(!msg.contains(&KEY[..8]) && !msg.contains("6789"), "{msg}");
    }

    #[test]
    fn transport_error_class_table() {
        let cases: &[(bool, &str, ErrorClass)] = &[
            (true, "timeout: operation timed out", ErrorClass::Timeout),
            (true, "timeout", ErrorClass::Timeout),
            (
                false,
                "connect: dns error: failed to lookup",
                ErrorClass::Network,
            ),
            (
                false,
                "connect: Connection refused (os error 10061)",
                ErrorClass::Network,
            ),
            (
                false,
                "connect: invalid peer certificate: UnknownIssuer",
                ErrorClass::Tls,
            ),
            (false, "request: TLS handshake failed", ErrorClass::Tls),
            (false, "connect: SSL routines", ErrorClass::Tls),
            (true, "timeout during handshake", ErrorClass::Tls),
            (false, "redirect: too many redirects", ErrorClass::Network),
            (false, "unknown", ErrorClass::Network),
            (false, "", ErrorClass::Network),
        ];
        for (timed_out, detail, class) in cases {
            assert_eq!(
                transport_error_class(*timed_out, detail),
                *class,
                "{detail}"
            );
        }
    }

    #[test]
    fn probe_url_rules() {
        let r = Protocol::Responses;
        let c = Protocol::ChatCompletions;
        assert_eq!(
            probe_url("https://g.example.com/v1", r),
            "https://g.example.com/v1/responses"
        );
        assert_eq!(
            probe_url("https://g.example.com/v1/", r),
            "https://g.example.com/v1/responses"
        );
        assert_eq!(
            probe_url("https://g.example.com/v1///", c),
            "https://g.example.com/v1/chat/completions"
        );
        assert_eq!(
            probe_url("  https://g.example.com/v1#  ", r),
            "https://g.example.com/v1/responses"
        );
        assert_eq!(
            probe_url("https://g.example.com/v1/responses", r),
            "https://g.example.com/v1/responses"
        );
        assert_eq!(
            probe_url("https://g.example.com/v1/chat/completions/", c),
            "https://g.example.com/v1/chat/completions"
        );
        assert_eq!(
            probe_url("https://g.example.com", r),
            "https://g.example.com/responses"
        );
    }

    #[test]
    fn probe_body_per_protocol() {
        let b = probe_body("gpt-5", Protocol::Responses);
        assert_eq!(b["model"], "gpt-5");
        assert_eq!(b["input"], "ping");
        assert_eq!(b["max_output_tokens"], 16);
        assert!(b.get("messages").is_none());

        let b = probe_body("", Protocol::ChatCompletions);
        assert_eq!(b["model"], "");
        assert_eq!(b["max_tokens"], 1);
        assert_eq!(b["messages"][0]["role"], "user");
        assert_eq!(b["messages"][0]["content"], "ping");
        assert!(b.get("input").is_none());
    }

    // ----- gateway: end-to-end against a loopback server -------------------------------

    /// One-shot HTTP/1.1 server on 127.0.0.1; returns the base URL and the raw request.
    fn serve_once(status_line: &'static str, body: &'static str) -> (String, JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut raw = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = stream.read(&mut chunk).expect("read");
                if n == 0 {
                    break;
                }
                raw.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&raw);
                if let Some(head_end) = text.find("\r\n\r\n") {
                    let head = &text[..head_end];
                    let len = head
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if raw.len() >= head_end + 4 + len {
                        break;
                    }
                }
            }
            let response = format!(
                "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).expect("write");
            stream.flush().expect("flush");
            String::from_utf8_lossy(&raw).into_owned()
        });
        (format!("http://{addr}/v1"), handle)
    }

    fn test_client() -> reqwest::Client {
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("client")
    }

    #[tokio::test]
    async fn probe_sends_bearer_header_and_classifies_401() {
        let body: &'static str = Box::leak(
            format!(r#"{{"error":{{"message":"Incorrect API key provided: {KEY}"}}}}"#)
                .into_boxed_str(),
        );
        let (base, server) = serve_once("401 Unauthorized", body);
        let req = GatewayProbeRequest {
            base_url: format!("{base}/"),
            api_key: format!(" {KEY} "),
            model: "gpt-5".into(),
            protocol: Protocol::Responses,
        };
        let check = probe_gateway(&test_client(), &req).await;
        let request = server.join().expect("server thread");

        assert!(
            request.starts_with("POST /v1/responses HTTP/1.1"),
            "{request}"
        );
        let lower = request.to_ascii_lowercase();
        assert!(
            lower.contains(&format!("authorization: bearer {KEY}")),
            "{request}"
        );
        assert!(request.contains(r#""max_output_tokens":16"#), "{request}");
        assert!(
            !request.contains(&format!("/{KEY}")),
            "key must never be in the URL"
        );

        assert!(!check.ok);
        assert_eq!(check.http_status, Some(401));
        assert_eq!(check.error_class, Some(ErrorClass::Auth));
        assert!(check.latency_ms.is_some());
        let msg = check.message.expect("message");
        assert!(!msg.contains(KEY), "{msg}");
        assert!(
            msg.starts_with("Incorrect API key provided: [REDACTED]"),
            "{msg}"
        );
    }

    #[tokio::test]
    async fn probe_chat_completions_success() {
        let (base, server) = serve_once("200 OK", r#"{"id":"chatcmpl-1","choices":[]}"#);
        let req = GatewayProbeRequest {
            base_url: base,
            api_key: KEY.into(),
            model: String::new(),
            protocol: Protocol::ChatCompletions,
        };
        let check = probe_gateway(&test_client(), &req).await;
        let request = server.join().expect("server thread");
        assert!(
            request.starts_with("POST /v1/chat/completions HTTP/1.1"),
            "{request}"
        );
        assert!(request.contains(r#""max_tokens":1"#), "{request}");
        assert!(check.ok);
        assert_eq!(check.http_status, Some(200));
        assert_eq!(check.error_class, None);
        assert_eq!(check.message, None);
    }

    #[tokio::test]
    async fn probe_connection_refused_is_network_error() {
        // Bind and immediately drop → nothing listens on that port.
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").expect("bind");
            l.local_addr().expect("addr").port()
        };
        let req = GatewayProbeRequest {
            base_url: format!("http://127.0.0.1:{port}/v1"),
            api_key: KEY.into(),
            model: "m".into(),
            protocol: Protocol::Responses,
        };
        let check = probe_gateway(&test_client(), &req).await;
        assert!(!check.ok);
        assert_eq!(check.http_status, None);
        assert_eq!(check.error_class, Some(ErrorClass::Network));
        assert!(!check.message.unwrap_or_default().contains(KEY));
    }

    #[tokio::test]
    async fn probe_invalid_url_is_unknown_without_status() {
        let req = GatewayProbeRequest {
            base_url: "not a url".into(),
            api_key: KEY.into(),
            model: "m".into(),
            protocol: Protocol::Responses,
        };
        let check = probe_gateway(&test_client(), &req).await;
        assert!(!check.ok);
        assert_eq!(check.http_status, None);
        assert_eq!(check.error_class, Some(ErrorClass::Unknown));
    }

    // ----- terminals -------------------------------------------------------------------

    #[test]
    fn terminal_matcher_windows() {
        let w = Platform::Windows;
        for name in [
            "WindowsTerminal.exe",
            "wt.exe",
            "CMD.EXE",
            "cmd",
            "powershell.exe",
            "pwsh.exe",
            "mintty.exe",
            "git-bash.exe",
            "alacritty.exe",
            "Hyper.exe",
            "Cursor.exe",
            "Code.exe",
        ] {
            assert!(is_terminal_process(name, w), "{name}");
        }
        for name in [
            "conhost.exe",
            "OpenConsole.exe",
            "explorer.exe",
            "node.exe",
            "Code Helper.exe",
            "codex.exe",
            "",
            "Terminal",
        ] {
            assert!(!is_terminal_process(name, w), "{name}");
        }
        assert_eq!(
            terminal_display_name("Code.exe", w).as_deref(),
            Some(VS_CODE)
        );
        assert_eq!(
            terminal_display_name("cmd.exe", w).as_deref(),
            Some("cmd.exe")
        );
        assert_eq!(
            terminal_display_name(" WindowsTerminal.exe ", w).as_deref(),
            Some("WindowsTerminal.exe")
        );
    }

    #[test]
    fn terminal_matcher_macos_and_others() {
        let m = Platform::Macos;
        for name in [
            "Terminal",
            "iTerm2",
            "Warp",
            "Hyper",
            "alacritty",
            "kitty",
            "wezterm-gui",
            "Ghostty",
            "Cursor",
            "Code",
        ] {
            assert!(is_terminal_process(name, m), "{name}");
        }
        for name in [
            "Code Helper",
            "Code Helper (Renderer)",
            "Electron",
            "stable",
            "zsh",
            "login",
            "cmd.exe",
        ] {
            assert!(!is_terminal_process(name, m), "{name}");
        }
        assert!(is_terminal_process("konsole", Platform::Linux));
        assert!(!is_terminal_process("cmd.exe", Platform::Unknown));
    }

    #[test]
    fn macos_bundle_matching() {
        assert_eq!(
            macos_bundle_display_name("/Applications/Warp.app/Contents/MacOS/stable"),
            Some("Warp")
        );
        assert_eq!(
            macos_bundle_display_name(
                "/Applications/Visual Studio Code.app/Contents/MacOS/Electron"
            ),
            Some(VS_CODE)
        );
        assert_eq!(
            macos_bundle_display_name(
                "/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal"
            ),
            Some("Terminal")
        );
        assert_eq!(
            macos_bundle_display_name("/Applications/iTerm.app/Contents/MacOS/iTerm2"),
            Some("iTerm2")
        );
        assert_eq!(
            macos_bundle_display_name("/Applications/Visual Studio Code.app/Contents/Frameworks/Code Helper (Renderer).app/Contents/MacOS/Code Helper (Renderer)"),
            None
        );
        assert_eq!(macos_bundle_display_name("/bin/zsh"), None);
        assert_eq!(macos_bundle_display_name(""), None);

        // classify_process prefers the bundle, falls back to the name
        assert_eq!(
            classify_process(
                "Electron",
                Some("/Applications/Visual Studio Code.app/Contents/MacOS/Electron"),
                Platform::Macos
            )
            .as_deref(),
            Some(VS_CODE)
        );
        assert_eq!(
            classify_process(
                "Electron",
                Some("/Applications/Slack.app/Contents/MacOS/Electron"),
                Platform::Macos
            ),
            None
        );
        assert_eq!(
            classify_process("kitty", None, Platform::Macos).as_deref(),
            Some("kitty")
        );
        // bundle matching is macOS-only
        assert_eq!(
            classify_process(
                "Electron",
                Some("/Applications/Warp.app/Contents/MacOS/stable"),
                Platform::Windows
            ),
            None
        );
    }

    fn entry(pid: u32, parent: Option<u32>, name: &str) -> ProcessEntry {
        ProcessEntry {
            pid,
            parent,
            name: name.to_owned(),
            exe: None,
        }
    }

    #[test]
    fn collect_terminals_filters_dedupes_sorts_and_excludes_own_tree() {
        let own = 100;
        let entries = vec![
            entry(1, None, "explorer.exe"),
            entry(20, Some(1), "WindowsTerminal.exe"),
            entry(21, Some(20), "powershell.exe"),
            entry(21, Some(20), "powershell.exe"), // duplicate pid+name
            entry(22, Some(21), "cmd.exe"),        // shell inside a shell of another kind → listed
            entry(23, Some(22), "cmd.exe"),        // nested same app → folded into 22
            entry(30, Some(1), "Code.exe"),
            entry(31, Some(30), "Code.exe"), // renderer → folded into the main window
            entry(32, Some(31), "Code.exe"), // extension host (grandchild) → folded
            entry(35, Some(1), "Code.exe"),  // second VS Code window → listed
            entry(own, Some(21), "codex-onboarding.exe"),
            entry(101, Some(own), "cmd.exe"), // our npm shim host
            entry(102, Some(101), "node.exe"),
            entry(103, Some(102), "cmd.exe"), // grandchild
            entry(40, Some(1), "conhost.exe"),
        ];
        let got = collect_terminals(&entries, Some(own), Platform::Windows);
        let names: Vec<(u32, &str)> = got.iter().map(|t| (t.pid, t.name.as_str())).collect();
        assert_eq!(
            names,
            vec![
                (30, VS_CODE),
                (35, VS_CODE),
                (20, "WindowsTerminal.exe"),
                (22, "cmd.exe"),
                (21, "powershell.exe"),
            ]
        );
        // without a known own pid nothing is excluded: our shim cmd (101) is listed, its
        // grandchild cmd (103) has a node.exe parent and is listed too
        let all = collect_terminals(&entries, None, Platform::Windows);
        let pids: Vec<u32> = all.iter().map(|t| t.pid).collect();
        assert_eq!(pids, vec![30, 35, 20, 22, 101, 103, 21]);
    }

    #[test]
    fn collect_terminals_caps_entries() {
        let entries: Vec<ProcessEntry> = (1..=(u32::try_from(MAX_TERMINALS).unwrap() + 20))
            .map(|pid| entry(pid, None, "cmd.exe"))
            .collect();
        assert_eq!(
            collect_terminals(&entries, Some(0), Platform::Windows).len(),
            MAX_TERMINALS
        );
    }

    #[test]
    fn running_terminals_does_not_panic_and_excludes_self() {
        let own = std::process::id();
        let list = running_terminals();
        assert!(list.len() <= MAX_TERMINALS);
        assert!(list.iter().all(|t| t.pid != own));
    }

    // ----- CLI check -------------------------------------------------------------------

    #[test]
    fn parse_version_table() {
        let cases = [
            ("codex-cli 0.42.0", Some("0.42.0")),
            ("1.0.5 (Claude Code)", Some("1.0.5")),
            ("v22.1.0\n", Some("22.1.0")),
            ("OpenAI Codex v0.1.2504", Some("0.1.2504")),
            ("2.0.0-beta.3+build7", Some("2.0.0-beta.3+build7")),
            ("version: 10.4", Some("10.4")),
            ("no version here", None),
            ("", None),
            ("hash a1b2.3c4", None),
        ];
        for (input, expected) in cases {
            assert_eq!(parse_version(input).as_deref(), expected, "{input:?}");
        }
    }

    fn output(
        stdout: &str,
        stderr: &str,
        exit_code: Option<i32>,
        timed_out: bool,
    ) -> CommandOutput {
        CommandOutput {
            stdout: stdout.to_owned(),
            stderr: stderr.to_owned(),
            exit_code,
            duration_ms: 1,
            timed_out,
        }
    }

    #[test]
    fn cli_check_from_output_cases() {
        let ok = cli_check_from_output(
            "/usr/local/bin/codex",
            &output("codex-cli 0.42.0\n", "", Some(0), false),
        );
        assert!(ok.ok);
        assert_eq!(ok.version.as_deref(), Some("0.42.0"));
        assert_eq!(ok.path.as_deref(), Some("/usr/local/bin/codex"));
        assert_eq!(ok.output_tail, None);
        assert_eq!(ok.error_class, None);

        let stderr_version =
            cli_check_from_output("p", &output("", "claude 1.2.3", Some(0), false));
        assert_eq!(stderr_version.version.as_deref(), Some("1.2.3"));

        let failed = cli_check_from_output(
            "p",
            &output(
                "l1\nl2\nl3\nl4\nl5\nl6",
                &format!("Error: bad key {KEY}"),
                Some(1),
                false,
            ),
        );
        assert!(!failed.ok);
        assert_eq!(failed.error_class, Some(ErrorClass::CommandFailed));
        let tail = failed.output_tail.expect("tail");
        assert_eq!(tail.lines().count(), OUTPUT_TAIL_LINES);
        assert!(tail.ends_with("Error: bad key [REDACTED]"), "{tail}");
        assert!(!tail.contains("l1"), "{tail}");

        let timed_out = cli_check_from_output("p", &output("", "", None, true));
        assert_eq!(timed_out.error_class, Some(ErrorClass::Timeout));
        assert_eq!(timed_out.output_tail, None);
        assert!(!timed_out.ok);
    }

    // ----- symptoms / verdict -----------------------------------------------------------

    fn cli(ok: bool, class: Option<ErrorClass>, tail: Option<&str>) -> CliCheck {
        CliCheck {
            ok,
            version: None,
            path: None,
            output_tail: tail.map(str::to_owned),
            error_class: class,
        }
    }

    fn gw(ok: bool, status: Option<u16>, class: Option<ErrorClass>) -> GatewayCheck {
        GatewayCheck {
            ok,
            http_status: status,
            latency_ms: Some(1),
            error_class: class,
            message: None,
        }
    }

    #[test]
    fn symptoms_from_cli_states() {
        let t = ToolId::Codex;
        assert!(symptoms_from(t, &cli(true, None, None), None, "").is_empty());
        assert!(
            symptoms_from(t, &cli(false, Some(ErrorClass::Unknown), None), None, "").is_empty()
        );

        let s = symptoms_from(
            t,
            &cli(false, Some(ErrorClass::CommandNotFound), None),
            None,
            "",
        );
        assert!(matches!(
            s.as_slice(),
            [Symptom::CommandNotFound {
                tool: ToolId::Codex
            }]
        ));

        let s = symptoms_from(
            t,
            &cli(false, Some(ErrorClass::CommandFailed), Some("boom")),
            None,
            "",
        );
        assert!(
            matches!(s.as_slice(), [Symptom::CommandFailed { tool: ToolId::Codex, output_tail }] if output_tail == "boom")
        );

        let s = symptoms_from(
            ToolId::ClaudeCode,
            &cli(false, Some(ErrorClass::Timeout), None),
            None,
            "",
        );
        assert!(
            matches!(s.as_slice(), [Symptom::CommandFailed { tool: ToolId::ClaudeCode, output_tail }] if output_tail.is_empty())
        );
    }

    type Expect = fn(&[Symptom]) -> bool;

    #[test]
    fn symptoms_from_gateway_states() {
        let t = ToolId::Codex;
        let ok_cli = cli(true, None, None);
        let cases: Vec<(GatewayCheck, Expect)> = vec![
            (gw(true, Some(200), None), |s| s.is_empty()),
            (gw(true, Some(400), None), |s| s.is_empty()),
            (gw(false, Some(401), Some(ErrorClass::Auth)), |s| {
                matches!(
                    s,
                    [Symptom::HttpStatus {
                        status: 401,
                        tool: ToolId::Codex
                    }]
                )
            }),
            (gw(false, Some(404), Some(ErrorClass::NotFound)), |s| {
                matches!(s, [Symptom::HttpStatus { status: 404, .. }])
            }),
            (gw(false, Some(429), Some(ErrorClass::RateLimited)), |s| {
                matches!(s, [Symptom::HttpStatus { status: 429, .. }])
            }),
            (gw(false, Some(503), Some(ErrorClass::ServerError)), |s| {
                matches!(s, [Symptom::HttpStatus { status: 503, .. }])
            }),
            (gw(false, Some(418), Some(ErrorClass::Unknown)), |s| {
                matches!(s, [Symptom::HttpStatus { status: 418, .. }])
            }),
            (
                gw(false, Some(400), Some(ErrorClass::ProtocolMismatch)),
                |s| {
                    matches!(
                        s,
                        [Symptom::ProtocolMismatch {
                            tool: ToolId::Codex
                        }]
                    )
                },
            ),
            (
                gw(false, None, Some(ErrorClass::Network)),
                |s| matches!(s, [Symptom::NetworkError { target }] if target == "https://g.example.com/v1"),
            ),
            (gw(false, None, Some(ErrorClass::Tls)), |s| {
                matches!(s, [Symptom::NetworkError { .. }])
            }),
            (
                gw(false, None, Some(ErrorClass::Timeout)),
                |s| matches!(s, [Symptom::Timeout { target }] if target == "https://g.example.com/v1"),
            ),
            (gw(false, Some(408), Some(ErrorClass::Timeout)), |s| {
                matches!(s, [Symptom::HttpStatus { status: 408, .. }])
            }),
            (gw(false, None, Some(ErrorClass::Unknown)), |s| s.is_empty()),
        ];
        for (i, (gateway, check)) in cases.iter().enumerate() {
            let s = symptoms_from(t, &ok_cli, Some(gateway), "https://g.example.com/v1");
            assert!(check(&s), "case {i}: {s:?}");
        }
        // CLI and gateway symptoms combine, CLI first
        let both = symptoms_from(
            t,
            &cli(false, Some(ErrorClass::CommandNotFound), None),
            Some(&gw(false, Some(401), Some(ErrorClass::Auth))),
            "x",
        );
        assert!(matches!(
            both.as_slice(),
            [
                Symptom::CommandNotFound { .. },
                Symptom::HttpStatus { status: 401, .. }
            ]
        ));
    }

    #[test]
    fn overall_ok_rules() {
        assert!(overall_ok(&cli(true, None, None), None));
        assert!(overall_ok(
            &cli(true, None, None),
            Some(&gw(true, Some(200), None))
        ));
        assert!(!overall_ok(
            &cli(true, None, None),
            Some(&gw(false, Some(401), Some(ErrorClass::Auth)))
        ));
        assert!(!overall_ok(
            &cli(false, Some(ErrorClass::CommandNotFound), None),
            None
        ));
        assert!(!overall_ok(
            &cli(false, Some(ErrorClass::CommandNotFound), None),
            Some(&gw(true, Some(200), None))
        ));
    }

    #[tokio::test]
    async fn verify_unknown_tool_spec_is_reported_not_panicked() {
        let mut config = crate::config::embedded().expect("embedded config");
        config.tools.clear();
        let result = verify(
            &test_client(),
            &config,
            VerifyRequest {
                tool: ToolId::Codex,
                gateway: None,
            },
        )
        .await;
        assert_eq!(result.tool, ToolId::Codex);
        assert!(!result.ok);
        assert_eq!(result.cli.error_class, Some(ErrorClass::Unknown));
        assert!(result.gateway.is_none());
        assert!(result.diagnoses.is_empty());
    }
}
