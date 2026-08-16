//! M3 — configuration walkthrough (positioning A: the user configures inside CC Switch;
//! this app supplies the values, validates input and verifies each step — it never writes
//! `~/.codex` / `~/.claude` itself, ADR-0003).
//!
//! Everything in this module is pure and unit-tested. Rust returns *codes + params*; the
//! wording (including tool-specific phrasing) is the UI's job through i18n.
//!
//! # `build_guide(tool, config)`
//!
//! Ordered [`GuideStep`]s. `id` is a stable snake_case identifier, `code` is the i18n prefix
//! (frontend key `guide:<code>.title` / `guide:<code>.body`). Codes emitted, in order:
//!
//! | id / code           | params                                | copy_value          | verify_check |
//! |---------------------|---------------------------------------|---------------------|--------------|
//! | `open_cc_switch`    | —                                     | —                   | `cc_switch`  |
//! | `login_chatgpt`     | — (codex only, optional)              | —                   | —            |
//! | `select_tool_tab`   | `tool`                                | —                   | —            |
//! | `add_provider`      | `provider_name`                       | provider name       | —            |
//! | `paste_base_url`    | `base_url`                            | preset base URL     | —            |
//! | `choose_protocol`   | `protocol` (codex only)               | —                   | —            |
//! | `paste_api_key`     | —                                     | —                   | —            |
//! | `set_model`         | `model_hint`, `reasoning_effort_hint` | model hint if set   | —            |
//! | `save_and_activate` | —                                     | —                   | —            |
//! | `close_terminals`   | —                                     | —                   | —            |
//! | `verify`            | `tool`                                | —                   | —            |
//!
//! `tool` is the wire form of [`ToolId`] (`codex` / `claude-code`); `protocol` is the wire
//! form of [`Protocol`] (`responses` / `chat_completions`).
//!
//! # `validate_api_key(key)`
//!
//! Format check only — the key is never logged, stored or returned (only its trimmed length).
//! Blocking issues (make `valid = false`): `empty`, `leading_or_trailing_whitespace`,
//! `contains_whitespace` (inner spaces/tabs), `contains_newline`, `looks_like_placeholder`
//! (`<`, `>`, `your`, `xxx`, `…`, `sk-xxxx`, `example`; case-insensitive), `non_ascii`.
//! Non-blocking hints: `unexpected_prefix` (does not start with `sk-` — gateways vary),
//! `too_short` (fewer than 20 characters after trimming). Frontend key: `guide:key.issue.<issue>`.
//!
//! # `preview_url(input, config)`
//!
//! Implements the guide's "API 地址处理规则" (assumption Q-U1 in docs/OPEN_QUESTIONS.md).
//! Input is trimmed, then:
//!
//! | input                                   | rule                | effective URL                  |
//! |-----------------------------------------|---------------------|--------------------------------|
//! | empty / unparsable / not http(s)        | `invalid`           | `""`                           |
//! | ends with `#`                           | `literal_hash`      | as typed, without `#`          |
//! | bare origin (`https://host[:port]`)     | `appended_v1`       | origin + `/v1`                 |
//! | path ends with `/v1`                    | `already_versioned` | as typed                       |
//! | any other path                          | `custom_path`       | as typed                       |
//!
//! Trailing slashes are always removed (`trailing_slash_removed`), credentials and fragments
//! are never part of the effective URL. Frontend keys: `guide:url.rule.<rule>` and
//! `guide:url.warning.<warning>`. Warnings: `not_https`,
//! `trailing_slash_removed`, `contains_whitespace`, `contains_credentials`,
//! `looks_like_chat_completions_endpoint` (path ends with `/chat/completions`),
//! `differs_from_company_gateway` (host[:port] + path differ from `gateway.base_url`).
//!
//! # `build_import_url` / `masked_import_url` (ADR-0006)
//!
//! Render CC Switch's provider-import deep link (`ccswitch://v1/import?resource=provider&…`)
//! for the one-click hand-off. The endpoint is the *effective* URL from `preview_url`, the
//! `app` parameter is `codex` / `claude`, and the model is skipped when empty. The masked
//! variant is what the UI shows before the user confirms; the real link is opened by the
//! `open_cc_switch_import` command and never logged. CC Switch confirms and writes its own
//! data — this module never touches `~/.cc-switch`.

use url::{form_urlencoded, Position, Url};

use crate::error::{AppError, AppResult};
use crate::models::{
    AppConfig, AutoCompactScope, CcSwitchImportRequest, CheckId, CodexConfigRequest, ConfigGuide,
    GuideStep, KeyIssue, KeyValidation, Params, Protocol, ProviderPreset, ToolId, UrlPreview,
    UrlRule, UrlWarning,
};
use crate::redact::{mask_value, redact_secrets};

/// Placeholder fragments (lower-case) that indicate the user pasted a template, not a key.
const PLACEHOLDER_PATTERNS: [&str; 7] = ["<", ">", "your", "xxx", "\u{2026}", "sk-xxxx", "example"];

/// Expected key prefix; only a hint (`UnexpectedPrefix` is non-blocking).
const EXPECTED_KEY_PREFIX: &str = "sk-";

/// Keys shorter than this (after trimming) get the non-blocking `TooShort` hint.
const MIN_KEY_LENGTH: usize = 20;

/// Wire form of a [`ToolId`] (`codex` / `claude-code`), used as an i18n param.
pub(crate) fn tool_key(tool: ToolId) -> &'static str {
    match tool {
        ToolId::Codex => "codex",
        ToolId::ClaudeCode => "claude-code",
    }
}

/// Wire form of a [`Protocol`] (`responses` / `chat_completions` / `anthropic_messages`), used
/// as an i18n param.
pub(crate) fn protocol_key(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Responses => "responses",
        Protocol::ChatCompletions => "chat_completions",
        Protocol::AnthropicMessages => "anthropic_messages",
    }
}

// ---------------------------------------------------------------------------
// Walkthrough steps
// ---------------------------------------------------------------------------

/// Builds the CC Switch walkthrough for `tool` from the company preset.
pub fn build_guide(tool: ToolId, config: &AppConfig) -> ConfigGuide {
    let g = &config.gateway;
    let preset = ProviderPreset {
        provider_name: g.preset_provider_name.clone(),
        base_url: g.base_url.clone(),
        protocol: g.protocol,
        model_hint: g.default_model.clone(),
        reasoning_effort_hint: g.default_reasoning_effort.clone(),
    };
    let steps = build_steps(tool, &preset);
    ConfigGuide {
        tool,
        preset,
        steps,
    }
}

/// Ordered steps for `tool` (see module docs for the table).
fn build_steps(tool: ToolId, preset: &ProviderPreset) -> Vec<GuideStep> {
    let tool_params = params([("tool", tool_key(tool))]);
    let mut steps = vec![step(
        "open_cc_switch",
        Params::new(),
        None,
        Some(CheckId::CcSwitch),
    )];
    if tool == ToolId::Codex {
        // Optional: a ChatGPT login right after installing CC Switch keeps the official
        // login-gated features (e.g. the speed/tier option) available alongside the gateway.
        steps.push(step("login_chatgpt", Params::new(), None, None));
    }
    steps.extend([
        step("select_tool_tab", tool_params.clone(), None, None),
        step(
            "add_provider",
            params([("provider_name", preset.provider_name.as_str())]),
            non_empty(&preset.provider_name),
            None,
        ),
        step(
            "paste_base_url",
            params([("base_url", preset.base_url.as_str())]),
            non_empty(&preset.base_url),
            None,
        ),
    ]);
    if tool == ToolId::Codex {
        steps.push(step(
            "choose_protocol",
            params([("protocol", protocol_key(preset.protocol))]),
            None,
            None,
        ));
    }
    steps.extend([
        step("paste_api_key", Params::new(), None, None),
        step(
            "set_model",
            params([
                ("model_hint", preset.model_hint.as_str()),
                (
                    "reasoning_effort_hint",
                    preset.reasoning_effort_hint.as_str(),
                ),
            ]),
            non_empty(&preset.model_hint),
            None,
        ),
        step("save_and_activate", Params::new(), None, None),
        step("close_terminals", Params::new(), None, None),
        step("verify", tool_params, None, None),
    ]);
    steps
}

fn step(
    id: &str,
    params: Params,
    copy_value: Option<String>,
    verify_check: Option<CheckId>,
) -> GuideStep {
    GuideStep {
        id: id.to_owned(),
        code: id.to_owned(),
        params,
        copy_value,
        verify_check,
    }
}

fn params<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Params {
    pairs
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

fn non_empty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_owned())
}

// ---------------------------------------------------------------------------
// API key format validation
// ---------------------------------------------------------------------------

/// Validates the *format* of an API key. The key itself never leaves this function: the
/// result carries only issue codes and the trimmed length.
pub fn validate_api_key(key: &str) -> KeyValidation {
    let trimmed = key.trim();
    let length = trimmed.chars().count();
    if trimmed.is_empty() {
        return KeyValidation {
            valid: false,
            issues: vec![KeyIssue::Empty],
            length,
        };
    }

    let lower = trimmed.to_ascii_lowercase();
    let checks: [(KeyIssue, bool); 7] = [
        (
            KeyIssue::LeadingOrTrailingWhitespace,
            trimmed.len() != key.len(),
        ),
        (
            KeyIssue::ContainsWhitespace,
            trimmed.chars().any(|c| c.is_whitespace() && !is_newline(c)),
        ),
        (KeyIssue::ContainsNewline, key.chars().any(is_newline)),
        (
            KeyIssue::LooksLikePlaceholder,
            PLACEHOLDER_PATTERNS.iter().any(|p| lower.contains(p)),
        ),
        (KeyIssue::NonAscii, !trimmed.is_ascii()),
        (
            KeyIssue::UnexpectedPrefix,
            !trimmed.starts_with(EXPECTED_KEY_PREFIX),
        ),
        (KeyIssue::TooShort, length < MIN_KEY_LENGTH),
    ];
    let issues: Vec<KeyIssue> = checks
        .into_iter()
        .filter_map(|(issue, hit)| hit.then_some(issue))
        .collect();
    let valid = !issues.iter().any(|i| is_blocking(*i));
    KeyValidation {
        valid,
        issues,
        length,
    }
}

fn is_newline(c: char) -> bool {
    c == '\n' || c == '\r'
}

/// Blocking issues make the key unusable; hints only inform.
pub fn is_blocking(issue: KeyIssue) -> bool {
    !matches!(issue, KeyIssue::UnexpectedPrefix | KeyIssue::TooShort)
}

// ---------------------------------------------------------------------------
// URL rule preview
// ---------------------------------------------------------------------------

/// Previews the URL the tool will effectively use for `input` (see module docs).
pub fn preview_url(input: &str, config: &AppConfig) -> UrlPreview {
    let echoed = redact_secrets(input);
    let trimmed = input.trim();
    match analyse_url(trimmed) {
        Some(analysis) => {
            let mut warnings = analysis.warnings;
            if differs_from_gateway(&analysis.url, &config.gateway.base_url) {
                warnings.push(UrlWarning::DiffersFromCompanyGateway);
            }
            UrlPreview {
                input: echoed,
                effective_url: analysis.effective_url,
                rule: analysis.rule,
                warnings,
            }
        }
        None => UrlPreview {
            input: echoed,
            effective_url: String::new(),
            rule: UrlRule::Invalid,
            warnings: Vec::new(),
        },
    }
}

/// Result of applying the URL rules to one input (before the service-gateway comparison).
struct UrlAnalysis {
    /// Parsed effective URL (credentials and fragment removed, path normalised).
    url: Url,
    effective_url: String,
    rule: UrlRule,
    warnings: Vec<UrlWarning>,
}

/// Applies the rule table to an already-trimmed input; `None` when the input is invalid.
fn analyse_url(trimmed: &str) -> Option<UrlAnalysis> {
    if trimmed.is_empty() {
        return None;
    }
    let mut warnings = Vec::new();
    if trimmed.chars().any(char::is_whitespace) {
        warnings.push(UrlWarning::ContainsWhitespace);
    }

    let literal_hash = trimmed.ends_with('#');
    let without_hash = trimmed.trim_end_matches('#');
    let stripped = without_hash.trim_end_matches('/');
    if stripped.len() != without_hash.len() {
        warnings.push(UrlWarning::TrailingSlashRemoved);
    }

    let mut url = Url::parse(stripped).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return None;
    }
    if url.scheme() == "http" {
        warnings.push(UrlWarning::NotHttps);
    }
    if !url.username().is_empty() || url.password().is_some() {
        warnings.push(UrlWarning::ContainsCredentials);
    }
    // Credentials and fragments are never part of the effective URL (they would also leak into
    // the UI / report otherwise). `set_*` only fail for cannot-be-a-base URLs, excluded above.
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_fragment(None);

    let path = url.path().trim_end_matches('/').to_owned();
    let rule = if literal_hash {
        UrlRule::LiteralHash
    } else if path.is_empty() {
        UrlRule::AppendedV1
    } else if path.ends_with("/v1") {
        UrlRule::AlreadyVersioned
    } else {
        UrlRule::CustomPath
    };
    let effective_path = if rule == UrlRule::AppendedV1 {
        "/v1".to_owned()
    } else {
        path
    };
    if effective_path.ends_with("/chat/completions") {
        warnings.push(UrlWarning::LooksLikeChatCompletionsEndpoint);
    }
    url.set_path(&effective_path);

    let effective_url = render(&url);
    Some(UrlAnalysis {
        url,
        effective_url,
        rule,
        warnings,
    })
}

/// `scheme://host[:port]<path>[?query]` without a trailing slash for an empty path.
fn render(url: &Url) -> String {
    let origin = &url[..Position::AfterPort];
    let path = url.path().trim_end_matches('/');
    let query = url.query().map(|q| format!("?{q}")).unwrap_or_default();
    format!("{origin}{path}{query}")
}

/// `host[:port]` + normalised path, used to compare against the service gateway.
fn comparable(url: &Url) -> String {
    let authority = &url[Position::BeforeHost..Position::AfterPort];
    format!(
        "{}{}",
        authority.to_ascii_lowercase(),
        url.path().trim_end_matches('/')
    )
}

/// `true` when `url` points somewhere else than the service gateway. An empty or unparsable
/// gateway preset never triggers the warning.
fn differs_from_gateway(url: &Url, gateway_base_url: &str) -> bool {
    let Ok(gateway) = Url::parse(gateway_base_url.trim()) else {
        return false;
    };
    if gateway.host_str().is_none() {
        return false;
    }
    comparable(url) != comparable(&gateway)
}

// ---------------------------------------------------------------------------
// CC Switch deep-link import (ADR-0006)
// ---------------------------------------------------------------------------

/// Base of CC Switch's provider-import deep link (`ccswitch://v1/import?resource=provider&…`).
/// Opening it hands the values to CC Switch, which shows its own confirmation dialog and does
/// its own writing — this app still never writes `~/.cc-switch` (ADR-0003).
pub const CC_SWITCH_IMPORT_BASE: &str = "ccswitch://v1/import";

/// CC Switch `app` parameter for a tool (`codex` / `claude`).
pub fn cc_switch_app(tool: ToolId) -> &'static str {
    match tool {
        ToolId::Codex => "codex",
        ToolId::ClaudeCode => "claude",
    }
}

/// The real import deep link (contains the trimmed API key — never log or display it).
/// Fails with `InvalidInput` when the provider name is empty, the key has blocking format
/// issues, or the base URL does not survive [`preview_url`]; the endpoint sent to CC Switch is
/// the *effective* URL so both flows configure the same address.
pub fn build_import_url(req: &CcSwitchImportRequest, config: &AppConfig) -> AppResult<String> {
    import_url(req, config, req.api_key.trim())
}

/// The same deep link with the key masked (`sk-****abcd`) — the "show before run" preview.
pub fn masked_import_url(req: &CcSwitchImportRequest, config: &AppConfig) -> AppResult<String> {
    import_url(req, config, &mask_value(&req.api_key))
}

fn import_url(req: &CcSwitchImportRequest, config: &AppConfig, key: &str) -> AppResult<String> {
    let name = req.provider_name.trim();
    if name.is_empty() {
        return Err(AppError::InvalidInput(
            "the provider name must not be empty".into(),
        ));
    }
    if !validate_api_key(&req.api_key).valid {
        return Err(AppError::InvalidInput(
            "the API key has blocking format issues".into(),
        ));
    }
    let preview = preview_url(&req.base_url, config);
    if preview.rule == UrlRule::Invalid || preview.effective_url.is_empty() {
        return Err(AppError::InvalidInput("the base URL is not valid".into()));
    }
    let mut query = form_urlencoded::Serializer::new(String::new());
    query
        .append_pair("resource", "provider")
        .append_pair("app", cc_switch_app(req.tool))
        .append_pair("name", name)
        .append_pair("endpoint", &preview.effective_url)
        .append_pair("apiKey", key);
    let model = req.model.trim();
    if !model.is_empty() {
        query.append_pair("model", model);
    }
    Ok(format!("{CC_SWITCH_IMPORT_BASE}?{}", query.finish()))
}

// ---------------------------------------------------------------------------
// Codex config.toml template (keeps the official features alongside the gateway)
// ---------------------------------------------------------------------------

/// Literal the template carries instead of the API key. The UI substitutes the real key at
/// copy time (`CodexConfigCard`), so the key is never displayed and never sent over IPC for
/// template generation. Keep in sync with `CODEX_CONFIG_KEY_PLACEHOLDER` in the frontend.
pub const CODEX_CONFIG_KEY_PLACEHOLDER: &str = "<API-KEY>";

/// Provider table key in the template (`[model_providers.cliproxyapi]`).
const CODEX_PROVIDER_ID: &str = "cliproxyapi";
const CODEX_SANDBOX_MODE: &str = "workspace-write";
const CODEX_MODEL_CONTEXT_WINDOW: u64 = 372_000;
const CODEX_AUTO_COMPACT_TOKEN_LIMIT: u64 = 300_000;
const CODEX_SERVICE_TIER: &str = "priority";

/// Wire form of an [`AutoCompactScope`] (`body_after_prefix` / `total`).
pub(crate) fn auto_compact_scope_key(scope: AutoCompactScope) -> &'static str {
    match scope {
        AutoCompactScope::BodyAfterPrefix => "body_after_prefix",
        AutoCompactScope::Total => "total",
    }
}

/// TOML basic string with the required escapes. Pure.
fn toml_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Renders the recommended Codex `config.toml` (per the IT/dev-team spec): the gateway as a
/// `cliproxyapi` provider with `experimental_bearer_token`, while keeping the official
/// features (priority service tier, large context window, auto-compact). The base URL is the
/// *effective* URL from [`preview_url`] (raw input when unparsable); `model` /
/// `model_reasoning_effort` lines are omitted when empty; the bearer token is always the
/// [`CODEX_CONFIG_KEY_PLACEHOLDER`] literal. The output is meant for an editable text box —
/// users may tune any value (e.g. a stricter auto-compact limit) before pasting it into the
/// CC Switch provider's config editor. Pure.
pub fn codex_config_template(req: &CodexConfigRequest, config: &AppConfig) -> String {
    let preview = preview_url(&req.base_url, config);
    let base_url = if preview.rule == UrlRule::Invalid || preview.effective_url.is_empty() {
        req.base_url.trim().to_owned()
    } else {
        preview.effective_url
    };
    let name = req.provider_name.trim();
    let name = if name.is_empty() {
        config.gateway.preset_provider_name.trim()
    } else {
        name
    };
    let model = req.model.trim();
    let effort = req.reasoning_effort.trim();
    let scope = auto_compact_scope_key(req.auto_compact_scope);

    let mut out = String::new();
    if !model.is_empty() {
        out.push_str(&format!("model = {}\n", toml_quote(model)));
    }
    out.push_str(&format!(
        "model_provider = {}\n",
        toml_quote(CODEX_PROVIDER_ID)
    ));
    if !effort.is_empty() {
        out.push_str(&format!(
            "model_reasoning_effort = {}\n",
            toml_quote(effort)
        ));
    }
    out.push_str(&format!(
        "sandbox_mode = {}\n",
        toml_quote(CODEX_SANDBOX_MODE)
    ));
    out.push_str(&format!(
        "model_context_window = {CODEX_MODEL_CONTEXT_WINDOW}\n"
    ));
    out.push_str(&format!(
        "model_auto_compact_token_limit = {CODEX_AUTO_COMPACT_TOKEN_LIMIT}\n"
    ));
    out.push_str(&format!(
        "model_auto_compact_token_limit_scope = {}\n",
        toml_quote(scope)
    ));
    out.push('\n');
    out.push_str(&format!(
        "service_tier = {}\n",
        toml_quote(CODEX_SERVICE_TIER)
    ));
    out.push('\n');
    out.push_str(&format!("[model_providers.{CODEX_PROVIDER_ID}]\n"));
    if !name.is_empty() {
        out.push_str(&format!("name = {}\n", toml_quote(name)));
    }
    out.push_str(&format!("base_url = {}\n", toml_quote(&base_url)));
    out.push_str("wire_api = \"responses\"\n");
    out.push_str("requires_openai_auth = true\n");
    out.push_str(&format!(
        "experimental_bearer_token = {}\n",
        toml_quote(CODEX_CONFIG_KEY_PLACEHOLDER)
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;

    fn cfg() -> AppConfig {
        let mut cfg = config::embedded().expect("embedded config");
        cfg.gateway.base_url = "https://gateway.example.com/v1".into();
        cfg.gateway.preset_provider_name = "Service Gateway".into();
        cfg.gateway.default_model = String::new();
        cfg
    }

    // ---- build_guide ---------------------------------------------------------------

    #[test]
    fn codex_guide_has_expected_step_order() {
        let guide = build_guide(ToolId::Codex, &cfg());
        let ids: Vec<&str> = guide.steps.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "open_cc_switch",
                "login_chatgpt",
                "select_tool_tab",
                "add_provider",
                "paste_base_url",
                "choose_protocol",
                "paste_api_key",
                "set_model",
                "save_and_activate",
                "close_terminals",
                "verify",
            ]
        );
        assert!(guide.steps.iter().all(|s| s.id == s.code));
        assert_eq!(guide.tool, ToolId::Codex);
        assert_eq!(guide.preset.base_url, "https://gateway.example.com/v1");
    }

    #[test]
    fn claude_code_guide_skips_codex_only_steps() {
        let guide = build_guide(ToolId::ClaudeCode, &cfg());
        assert!(guide.steps.iter().all(|s| s.id != "choose_protocol"));
        assert!(guide.steps.iter().all(|s| s.id != "login_chatgpt"));
        assert_eq!(guide.steps.len(), 9);
        let tab = guide
            .steps
            .iter()
            .find(|s| s.id == "select_tool_tab")
            .expect("select_tool_tab");
        assert_eq!(
            tab.params.get("tool").map(String::as_str),
            Some("claude-code")
        );
        let verify = guide.steps.last().expect("verify");
        assert_eq!(verify.id, "verify");
        assert_eq!(
            verify.params.get("tool").map(String::as_str),
            Some("claude-code")
        );
    }

    #[test]
    fn steps_carry_copy_values_params_and_verify_check() {
        let mut c = cfg();
        c.gateway.default_model = "gpt-5".into();
        c.gateway.default_reasoning_effort = "medium".into();
        let guide = build_guide(ToolId::Codex, &c);
        let by_id = |id: &str| guide.steps.iter().find(|s| s.id == id).expect(id);

        assert_eq!(
            by_id("open_cc_switch").verify_check,
            Some(CheckId::CcSwitch)
        );
        assert!(guide
            .steps
            .iter()
            .filter(|s| s.id != "open_cc_switch")
            .all(|s| s.verify_check.is_none()));

        let url = by_id("paste_base_url");
        assert_eq!(
            url.copy_value.as_deref(),
            Some("https://gateway.example.com/v1")
        );
        assert_eq!(
            url.params.get("base_url").map(String::as_str),
            Some("https://gateway.example.com/v1")
        );

        let provider = by_id("add_provider");
        assert_eq!(provider.copy_value.as_deref(), Some("Service Gateway"));

        let protocol = by_id("choose_protocol");
        assert_eq!(
            protocol.params.get("protocol").map(String::as_str),
            Some("responses")
        );

        let model = by_id("set_model");
        assert_eq!(model.copy_value.as_deref(), Some("gpt-5"));
        assert_eq!(
            model.params.get("model_hint").map(String::as_str),
            Some("gpt-5")
        );
        assert_eq!(
            model
                .params
                .get("reasoning_effort_hint")
                .map(String::as_str),
            Some("medium")
        );
        assert!(by_id("paste_api_key").copy_value.is_none());
    }

    #[test]
    fn empty_model_hint_has_no_copy_value() {
        let guide = build_guide(ToolId::Codex, &cfg());
        let model = guide
            .steps
            .iter()
            .find(|s| s.id == "set_model")
            .expect("set_model");
        assert!(model.copy_value.is_none());
        assert_eq!(model.params.get("model_hint").map(String::as_str), Some(""));
    }

    #[test]
    fn chat_completions_preset_is_reflected_in_protocol_step() {
        let mut c = cfg();
        c.gateway.protocol = Protocol::ChatCompletions;
        let guide = build_guide(ToolId::Codex, &c);
        let protocol = guide
            .steps
            .iter()
            .find(|s| s.id == "choose_protocol")
            .expect("choose_protocol");
        assert_eq!(
            protocol.params.get("protocol").map(String::as_str),
            Some("chat_completions")
        );
    }

    // ---- validate_api_key ----------------------------------------------------------

    #[test]
    fn key_validation_table() {
        use KeyIssue::{
            ContainsNewline, ContainsWhitespace, Empty, LeadingOrTrailingWhitespace,
            LooksLikePlaceholder, NonAscii, TooShort, UnexpectedPrefix,
        };
        let cases: Vec<(&str, bool, Vec<KeyIssue>)> = vec![
            ("sk-abcdefghijklmnopqrstuvwxyz0123", true, vec![]),
            ("", false, vec![Empty]),
            ("   ", false, vec![Empty]),
            ("\n\t", false, vec![Empty]),
            (
                " sk-abcdefghijklmnopqrstuvwxyz0123",
                false,
                vec![LeadingOrTrailingWhitespace],
            ),
            (
                "sk-abcdefghijklmnopqrstuvwxyz0123 ",
                false,
                vec![LeadingOrTrailingWhitespace],
            ),
            (
                "sk-abcdefghijklmnopqrstuvwxyz0123\n",
                false,
                vec![LeadingOrTrailingWhitespace, ContainsNewline],
            ),
            (
                "sk-abcdefghijkl mnopqrstuvwxyz0123",
                false,
                vec![ContainsWhitespace],
            ),
            (
                "sk-abcdefghijkl\tmnopqrstuvwxyz0123",
                false,
                vec![ContainsWhitespace],
            ),
            (
                "sk-abcdefghijkl\nmnopqrstuvwxyz0123",
                false,
                vec![ContainsNewline],
            ),
            (
                "sk-abcdefghijkl\r\nmnopqrstuvwxyz0123",
                false,
                vec![ContainsNewline],
            ),
            (
                "<your-api-key-goes-here-1234>",
                false,
                vec![LooksLikePlaceholder, UnexpectedPrefix],
            ),
            (
                "sk-xxxxxxxxxxxxxxxxxxxxxxxxxxxx",
                false,
                vec![LooksLikePlaceholder],
            ),
            (
                "sk-YOURKEYabcdefghijklmnopqrstuv",
                false,
                vec![LooksLikePlaceholder],
            ),
            (
                "sk-example-abcdefghijklmnopqrstu",
                false,
                vec![LooksLikePlaceholder],
            ),
            (
                "sk-abcdefghijklmnopqrstuvwxyz\u{2026}",
                false,
                vec![LooksLikePlaceholder, NonAscii],
            ),
            ("sk-abcdefghijklmnopqrstuvwxyz密钥", false, vec![NonAscii]),
            (
                "sk-\u{ff41}bcdefghijklmnopqrstuvwxyz0123",
                false,
                vec![NonAscii],
            ),
            (
                "gw-abcdefghijklmnopqrstuvwxyz0123",
                true,
                vec![UnexpectedPrefix],
            ),
            ("sk-short", true, vec![TooShort]),
            ("abc", true, vec![UnexpectedPrefix, TooShort]),
            (
                " your key ",
                false,
                vec![
                    LeadingOrTrailingWhitespace,
                    ContainsWhitespace,
                    LooksLikePlaceholder,
                    UnexpectedPrefix,
                    TooShort,
                ],
            ),
        ];
        for (input, valid, issues) in cases {
            let result = validate_api_key(input);
            assert_eq!(result.valid, valid, "valid for {input:?}");
            assert_eq!(result.issues, issues, "issues for {input:?}");
        }
    }

    #[test]
    fn key_validation_reports_trimmed_char_length_only() {
        let result = validate_api_key("  sk-abcdefghijklmnopqrstuvwxyz0123  ");
        assert_eq!(result.length, 33);
        let unicode = validate_api_key("sk-密钥");
        assert_eq!(unicode.length, 5);
        assert_eq!(validate_api_key("").length, 0);
        let json = serde_json::to_string(&result).expect("json");
        assert!(
            !json.contains("abcdefghijklmnop"),
            "key must never be serialised: {json}"
        );
    }

    #[test]
    fn blocking_classification() {
        assert!(!is_blocking(KeyIssue::UnexpectedPrefix));
        assert!(!is_blocking(KeyIssue::TooShort));
        for issue in [
            KeyIssue::Empty,
            KeyIssue::LeadingOrTrailingWhitespace,
            KeyIssue::ContainsWhitespace,
            KeyIssue::ContainsNewline,
            KeyIssue::NonAscii,
            KeyIssue::LooksLikePlaceholder,
        ] {
            assert!(is_blocking(issue), "{issue:?}");
        }
    }

    // ---- preview_url ---------------------------------------------------------------

    #[test]
    fn url_preview_table() {
        use UrlWarning::{
            ContainsCredentials, ContainsWhitespace, DiffersFromCompanyGateway,
            LooksLikeChatCompletionsEndpoint, NotHttps, TrailingSlashRemoved,
        };
        let c = cfg();
        // (input, effective_url, rule, warnings)
        let cases: Vec<(&str, &str, UrlRule, Vec<UrlWarning>)> = vec![
            (
                "https://gateway.example.com/v1",
                "https://gateway.example.com/v1",
                UrlRule::AlreadyVersioned,
                vec![],
            ),
            (
                "  https://gateway.example.com/v1  ",
                "https://gateway.example.com/v1",
                UrlRule::AlreadyVersioned,
                vec![],
            ),
            (
                "https://gateway.example.com/v1/",
                "https://gateway.example.com/v1",
                UrlRule::AlreadyVersioned,
                vec![TrailingSlashRemoved],
            ),
            (
                "https://gateway.example.com",
                "https://gateway.example.com/v1",
                UrlRule::AppendedV1,
                vec![],
            ),
            (
                "https://gateway.example.com/",
                "https://gateway.example.com/v1",
                UrlRule::AppendedV1,
                vec![TrailingSlashRemoved],
            ),
            (
                "https://GATEWAY.example.com/v1",
                "https://gateway.example.com/v1",
                UrlRule::AlreadyVersioned,
                vec![],
            ),
            (
                "https://gateway.example.com/v1#",
                "https://gateway.example.com/v1",
                UrlRule::LiteralHash,
                vec![],
            ),
            (
                "https://gateway.example.com#",
                "https://gateway.example.com",
                UrlRule::LiteralHash,
                vec![DiffersFromCompanyGateway],
            ),
            (
                "https://gateway.example.com/openai/v1",
                "https://gateway.example.com/openai/v1",
                UrlRule::AlreadyVersioned,
                vec![DiffersFromCompanyGateway],
            ),
            (
                "https://gateway.example.com/api",
                "https://gateway.example.com/api",
                UrlRule::CustomPath,
                vec![DiffersFromCompanyGateway],
            ),
            (
                "https://gateway.example.com/v1/chat/completions",
                "https://gateway.example.com/v1/chat/completions",
                UrlRule::CustomPath,
                vec![LooksLikeChatCompletionsEndpoint, DiffersFromCompanyGateway],
            ),
            (
                "http://gateway.example.com/v1",
                "http://gateway.example.com/v1",
                UrlRule::AlreadyVersioned,
                vec![NotHttps],
            ),
            (
                "https://user:secret@gateway.example.com/v1",
                "https://gateway.example.com/v1",
                UrlRule::AlreadyVersioned,
                vec![ContainsCredentials],
            ),
            (
                "https://other.example.org/v1",
                "https://other.example.org/v1",
                UrlRule::AlreadyVersioned,
                vec![DiffersFromCompanyGateway],
            ),
            (
                "https://gateway.example.com:8443/v1",
                "https://gateway.example.com:8443/v1",
                UrlRule::AlreadyVersioned,
                vec![DiffersFromCompanyGateway],
            ),
            (
                "https://gateway.example.com/v 1",
                "https://gateway.example.com/v%201",
                UrlRule::CustomPath,
                vec![ContainsWhitespace, DiffersFromCompanyGateway],
            ),
            (
                "https://gateway.example.com/v1?team=a",
                "https://gateway.example.com/v1?team=a",
                UrlRule::AlreadyVersioned,
                vec![],
            ),
            (
                "https://gateway.example.com/v1#section",
                "https://gateway.example.com/v1",
                UrlRule::AlreadyVersioned,
                vec![],
            ),
            ("", "", UrlRule::Invalid, vec![]),
            ("   ", "", UrlRule::Invalid, vec![]),
            ("gateway.example.com/v1", "", UrlRule::Invalid, vec![]),
            ("ftp://gateway.example.com/v1", "", UrlRule::Invalid, vec![]),
            ("https://", "", UrlRule::Invalid, vec![]),
            ("not a url", "", UrlRule::Invalid, vec![]),
        ];
        for (input, effective, rule, warnings) in cases {
            let preview = preview_url(input, &c);
            assert_eq!(preview.rule, rule, "rule for {input:?}");
            assert_eq!(preview.effective_url, effective, "effective for {input:?}");
            assert_eq!(preview.warnings, warnings, "warnings for {input:?}");
        }
    }

    #[test]
    fn preview_never_echoes_credentials() {
        let preview = preview_url(
            "https://user:sk-abcdefghijklmnop@gateway.example.com/v1",
            &cfg(),
        );
        assert!(
            !preview.input.contains("sk-abcdefghijklmnop"),
            "{}",
            preview.input
        );
        assert!(
            !preview.effective_url.contains("user"),
            "{}",
            preview.effective_url
        );
        assert!(preview.warnings.contains(&UrlWarning::ContainsCredentials));
    }

    #[test]
    fn gateway_comparison_ignores_scheme_case_and_trailing_slash() {
        let mut c = cfg();
        c.gateway.base_url = "HTTPS://Gateway.Example.com/v1/".into();
        let preview = preview_url("http://gateway.example.com/v1", &c);
        assert_eq!(preview.warnings, vec![UrlWarning::NotHttps]);
    }

    // ---- CC Switch import deep link ---------------------------------------------------

    const IMPORT_KEY: &str = "sk-abcdefghijklmnopqrstuvwxyz0123";

    fn import_request() -> CcSwitchImportRequest {
        CcSwitchImportRequest {
            tool: ToolId::Codex,
            provider_name: "Service Gateway".into(),
            base_url: "https://gateway.example.com/v1".into(),
            api_key: IMPORT_KEY.into(),
            model: "gpt-5".into(),
        }
    }

    #[test]
    fn import_url_encodes_all_provider_fields() {
        let url = build_import_url(&import_request(), &cfg()).expect("url");
        assert!(url.starts_with("ccswitch://v1/import?"), "{url}");
        assert!(url.contains("resource=provider"), "{url}");
        assert!(url.contains("app=codex"), "{url}");
        assert!(url.contains("name=Service+Gateway"), "{url}");
        assert!(
            url.contains("endpoint=https%3A%2F%2Fgateway.example.com%2Fv1"),
            "{url}"
        );
        assert!(url.contains(&format!("apiKey={IMPORT_KEY}")), "{url}");
        assert!(url.contains("model=gpt-5"), "{url}");
    }

    #[test]
    fn import_url_maps_claude_code_and_skips_empty_model() {
        let mut req = import_request();
        req.tool = ToolId::ClaudeCode;
        req.model = "   ".into();
        let url = build_import_url(&req, &cfg()).expect("url");
        assert!(url.contains("app=claude"), "{url}");
        assert!(!url.contains("model="), "{url}");
    }

    #[test]
    fn import_url_uses_the_effective_url() {
        let mut req = import_request();
        req.base_url = "https://gateway.example.com/".into();
        let url = build_import_url(&req, &cfg()).expect("url");
        assert!(
            url.contains("endpoint=https%3A%2F%2Fgateway.example.com%2Fv1"),
            "trailing slash normalised and /v1 appended: {url}"
        );
    }

    #[test]
    fn masked_import_url_never_contains_the_key() {
        let masked = masked_import_url(&import_request(), &cfg()).expect("url");
        assert!(!masked.contains(IMPORT_KEY), "{masked}");
        // `form_urlencoded` leaves `*` unescaped, so the mask survives verbatim.
        assert!(masked.contains("apiKey=sk-****0123"), "{masked}");
        // apart from the key, both links are identical
        let real = build_import_url(&import_request(), &cfg()).expect("url");
        assert_eq!(real.replace(IMPORT_KEY, "sk-****0123"), masked);
    }

    #[test]
    fn import_url_rejects_invalid_input() {
        let c = cfg();
        let mut empty_name = import_request();
        empty_name.provider_name = "  ".into();
        let mut bad_key = import_request();
        bad_key.api_key = "sk-xxxx placeholder".into();
        let mut empty_key = import_request();
        empty_key.api_key = String::new();
        let mut bad_url = import_request();
        bad_url.base_url = "not a url".into();
        for req in [empty_name, bad_key, empty_key, bad_url] {
            let err = build_import_url(&req, &c).expect_err("must fail");
            assert_eq!(err.code(), "invalid_input");
            let masked = masked_import_url(&req, &c).expect_err("must fail");
            assert_eq!(masked.code(), "invalid_input");
        }
    }

    // ---- Codex config.toml template ---------------------------------------------------

    fn codex_config_request() -> CodexConfigRequest {
        CodexConfigRequest {
            provider_name: "SeedRouter".into(),
            base_url: "https://seedrouter.net/v1".into(),
            model: "gpt-5.6-sol".into(),
            reasoning_effort: "medium".into(),
            auto_compact_scope: AutoCompactScope::BodyAfterPrefix,
        }
    }

    #[test]
    fn codex_config_template_matches_the_spec() {
        let rendered = codex_config_template(&codex_config_request(), &cfg());
        let expected = "\
model = \"gpt-5.6-sol\"
model_provider = \"cliproxyapi\"
model_reasoning_effort = \"medium\"
sandbox_mode = \"workspace-write\"
model_context_window = 372000
model_auto_compact_token_limit = 300000
model_auto_compact_token_limit_scope = \"body_after_prefix\"

service_tier = \"priority\"

[model_providers.cliproxyapi]
name = \"SeedRouter\"
base_url = \"https://seedrouter.net/v1\"
wire_api = \"responses\"
requires_openai_auth = true
experimental_bearer_token = \"<API-KEY>\"
";
        assert_eq!(rendered, expected);
        // the template is valid TOML as-is
        let parsed: toml::Value = toml::from_str(&rendered).expect("valid TOML");
        assert_eq!(
            parsed
                .get("model_providers")
                .and_then(|p| p.get("cliproxyapi"))
                .and_then(|p| p.get("experimental_bearer_token"))
                .and_then(toml::Value::as_str),
            Some(CODEX_CONFIG_KEY_PLACEHOLDER)
        );
    }

    #[test]
    fn codex_config_template_scope_and_optional_fields() {
        let mut req = codex_config_request();
        req.auto_compact_scope = AutoCompactScope::Total;
        req.model = "  ".into();
        req.reasoning_effort = String::new();
        req.provider_name = String::new();
        req.base_url = "https://seedrouter.net/".into();
        let rendered = codex_config_template(&req, &cfg());
        assert!(
            rendered.contains("model_auto_compact_token_limit_scope = \"total\""),
            "{rendered}"
        );
        assert!(!rendered.contains("\nmodel = "), "{rendered}");
        assert!(!rendered.starts_with("model = "), "{rendered}");
        assert!(!rendered.contains("model_reasoning_effort"), "{rendered}");
        // empty provider name falls back to the company preset
        assert!(
            rendered.contains("name = \"Service Gateway\""),
            "{rendered}"
        );
        // the effective URL is used (trailing slash normalised, /v1 appended)
        assert!(
            rendered.contains("base_url = \"https://seedrouter.net/v1\""),
            "{rendered}"
        );
        toml::from_str::<toml::Value>(&rendered).expect("valid TOML");
    }

    #[test]
    fn codex_config_template_escapes_and_survives_broken_urls() {
        let mut req = codex_config_request();
        req.provider_name = "My \"Router\"\\x".into();
        req.base_url = "not a url".into();
        let rendered = codex_config_template(&req, &cfg());
        assert!(
            rendered.contains(r#"name = "My \"Router\"\\x""#),
            "{rendered}"
        );
        assert!(rendered.contains("base_url = \"not a url\""), "{rendered}");
        toml::from_str::<toml::Value>(&rendered).expect("valid TOML");
    }

    #[test]
    fn toml_quote_escapes_control_characters() {
        assert_eq!(toml_quote("plain"), "\"plain\"");
        assert_eq!(toml_quote("a\"b\\c"), r#""a\"b\\c""#);
        assert_eq!(toml_quote("a\nb\tc"), r#""a\nb\tc""#);
        assert_eq!(toml_quote("\u{1}"), "\"\\u0001\"");
    }

    #[test]
    fn import_url_accepts_hint_only_key_issues() {
        // UnexpectedPrefix / TooShort are hints, not blockers — gateways vary.
        let mut req = import_request();
        req.api_key = "gw-shortkey".into();
        let url = build_import_url(&req, &cfg()).expect("hints do not block");
        assert!(url.contains("apiKey=gw-shortkey"), "{url}");
    }

    #[test]
    fn empty_or_broken_gateway_preset_never_warns_about_difference() {
        for base in ["", "   ", "not-a-url", "https://"] {
            let mut c = cfg();
            c.gateway.base_url = base.into();
            let preview = preview_url("https://anything.example.net/v1", &c);
            assert!(
                !preview
                    .warnings
                    .contains(&UrlWarning::DiffersFromCompanyGateway),
                "gateway {base:?}"
            );
        }
    }
}
