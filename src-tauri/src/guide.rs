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

use url::{Position, Url};

use crate::models::{
    AppConfig, CheckId, ConfigGuide, GuideStep, KeyIssue, KeyValidation, Params, Protocol,
    ProviderPreset, ToolId, UrlPreview, UrlRule, UrlWarning,
};
use crate::redact::redact_secrets;

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

/// Wire form of a [`Protocol`] (`responses` / `chat_completions`), used as an i18n param.
pub(crate) fn protocol_key(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Responses => "responses",
        Protocol::ChatCompletions => "chat_completions",
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
    let mut steps = vec![
        step(
            "open_cc_switch",
            Params::new(),
            None,
            Some(CheckId::CcSwitch),
        ),
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
    ];
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

/// Result of applying the URL rules to one input (before the company-gateway comparison).
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

/// `host[:port]` + normalised path, used to compare against the company gateway.
fn comparable(url: &Url) -> String {
    let authority = &url[Position::BeforeHost..Position::AfterPort];
    format!(
        "{}{}",
        authority.to_ascii_lowercase(),
        url.path().trim_end_matches('/')
    )
}

/// `true` when `url` points somewhere else than the company gateway. An empty or unparsable
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;

    fn cfg() -> AppConfig {
        let mut cfg = config::embedded().expect("embedded config");
        cfg.gateway.base_url = "https://gateway.example.com/v1".into();
        cfg.gateway.preset_provider_name = "Company Gateway".into();
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
    fn claude_code_guide_skips_protocol_step() {
        let guide = build_guide(ToolId::ClaudeCode, &cfg());
        assert!(guide.steps.iter().all(|s| s.id != "choose_protocol"));
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
        assert_eq!(provider.copy_value.as_deref(), Some("Company Gateway"));

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
