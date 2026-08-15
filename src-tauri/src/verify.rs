//! M4 — verification. Runs the CLI in a *fresh session* environment (as a newly opened
//! terminal would see it), optionally probes the gateway with the user's key (held in memory
//! only for the duration of the request; never logged), lists still-running terminal processes
//! (guide fault C: "close all terminal windows"), and feeds any failure straight into M5.
//!
//! TODO(impl):
//! - `running_terminals`: sysinfo process scan — Windows: WindowsTerminal.exe, cmd.exe,
//!   powershell.exe, pwsh.exe, Code.exe (integrated terminal — report as `code`), conhost.exe
//!   is *not* counted; macOS: Terminal, iTerm2, Warp, Hyper, Alacritty, kitty, WezTerm, Code.
//! - `probe_gateway`: minimal request per protocol —
//!     responses:        POST {base_url}/responses        {"model": m, "input": "ping", "max_output_tokens": 1}
//!     chat_completions: POST {base_url}/chat/completions {"model": m, "messages":[{"role":"user","content":"ping"}], "max_tokens": 1}
//!   `Authorization: Bearer <key>`; 10 s timeout. Map: 200/201 → ok; 401/403 → Auth;
//!   404 → NotFound; 400/422 with protocol-ish message → ProtocolMismatch; 429 → RateLimited;
//!   5xx → ServerError; connect/dns → Network; timeout → Timeout; tls → Tls. `message` is the
//!   redacted first 300 chars of the body.
//! - `verify`: fresh-session `<binary> --version` (CommandNotFound → fault E), gateway probe,
//!   running terminals, then `diagnose::diagnose` on collected symptoms.

use crate::models::{
    AppConfig, CliCheck, GatewayCheck, GatewayProbeRequest, TerminalProcess, VerifyRequest,
    VerifyResult,
};

pub fn running_terminals() -> Vec<TerminalProcess> {
    // TODO(impl)
    Vec::new()
}

pub async fn probe_gateway(client: &reqwest::Client, req: &GatewayProbeRequest) -> GatewayCheck {
    // TODO(impl)
    let _ = (client, req);
    GatewayCheck {
        ok: false,
        http_status: None,
        latency_ms: None,
        error_class: None,
        message: None,
    }
}

pub async fn verify(
    client: &reqwest::Client,
    config: &AppConfig,
    req: VerifyRequest,
) -> VerifyResult {
    // TODO(impl)
    let _ = (client, config);
    VerifyResult {
        tool: req.tool,
        cli: CliCheck {
            ok: false,
            version: None,
            path: None,
            output_tail: None,
            error_class: None,
        },
        gateway: None,
        running_terminals: running_terminals(),
        ok: false,
        diagnoses: Vec::new(),
    }
}
