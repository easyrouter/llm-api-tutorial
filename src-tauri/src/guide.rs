//! M3 — configuration walkthrough (positioning A: the user configures inside CC Switch;
//! this app supplies the values, validates input and verifies each step — it never writes
//! `~/.codex` / `~/.claude` itself, ADR-0003).
//!
//! TODO(impl):
//! - `build_guide(tool)`: ordered `GuideStep`s with codes (frontend key = `guide.<code>.title|body`):
//!     open_cc_switch, select_tool_tab(codex|claude-code), add_provider, paste_base_url
//!     (copy_value = preset base_url), choose_protocol (responses), paste_api_key,
//!     set_model (hint), save_and_activate, close_terminals, verify
//!   `verify_check` links steps to M1 checks where auto-verification is possible.
//! - `validate_api_key`: never logs/returns the key. Rules → `KeyIssue`s:
//!     empty; leading/trailing whitespace; inner whitespace; newline; non-ASCII;
//!     prefix not `sk-` (UnexpectedPrefix — warning only, gateways vary); length < 20 (TooShort);
//!     placeholder text such as `<your-key>` / `sk-xxxx` (LooksLikePlaceholder).
//!   `valid` = no blocking issue (whitespace/newline/empty/placeholder are blocking; prefix is not).
//! - `preview_url`: implements the guide's "API 地址处理规则" (assumption — see docs/OPEN_QUESTIONS.md):
//!     trailing `/` removed (warning TrailingSlashRemoved); ends with `#` → LiteralHash (strip `#`,
//!     append nothing); path ends with `/v1` → AlreadyVersioned; empty path → AppendedV1;
//!     other path → CustomPath. Warnings: NotHttps, ContainsWhitespace, ContainsCredentials,
//!     LooksLikeChatCompletionsEndpoint (`/chat/completions` suffix), DiffersFromCompanyGateway.

use crate::models::{AppConfig, ConfigGuide, KeyValidation, ToolId, UrlPreview, UrlRule};

pub fn build_guide(tool: ToolId, config: &AppConfig) -> ConfigGuide {
    let g = &config.gateway;
    ConfigGuide {
        tool,
        preset: crate::models::ProviderPreset {
            provider_name: g.preset_provider_name.clone(),
            base_url: g.base_url.clone(),
            protocol: g.protocol,
            model_hint: g.default_model.clone(),
            reasoning_effort_hint: g.default_reasoning_effort.clone(),
        },
        steps: Vec::new(), // TODO(impl)
    }
}

pub fn validate_api_key(key: &str) -> KeyValidation {
    // TODO(impl)
    KeyValidation {
        valid: !key.trim().is_empty(),
        issues: Vec::new(),
        length: key.trim().len(),
    }
}

pub fn preview_url(input: &str, config: &AppConfig) -> UrlPreview {
    // TODO(impl)
    let _ = config;
    UrlPreview {
        input: input.to_owned(),
        effective_url: input.trim().trim_end_matches('/').to_owned(),
        rule: UrlRule::CustomPath,
        warnings: Vec::new(),
    }
}
