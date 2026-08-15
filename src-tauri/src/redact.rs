//! Secret redaction helpers. **Every** string that originates from the user's machine or a
//! remote server and is about to be shown, logged, reported or sent anywhere must pass
//! through [`redact_secrets`]. Values known to be secrets are masked with [`mask_value`].

use std::sync::OnceLock;

use regex::Regex;

/// Replaces API keys, bearer tokens and credentials embedded in URLs with `[REDACTED]`.
pub fn redact_secrets(input: &str) -> String {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| {
        [
            // OpenAI / Anthropic style keys: sk-..., sk-ant-..., sk-proj-...
            r"sk-[A-Za-z0-9_\-]{8,}",
            // Bearer tokens
            r"(?i)bearer\s+[A-Za-z0-9._\-]{8,}",
            // api-key style headers / query params
            r#"(?i)(api[_-]?key|authorization|x-api-key|token)(["']?\s*[:=]\s*["']?)[^\s"'&,]{6,}"#,
            // credentials in URLs  https://user:pass@host
            r"://[^/\s:@]+:[^/\s@]+@",
        ]
        .into_iter()
        .filter_map(|p| Regex::new(p).ok())
        .collect()
    });

    let mut out = input.to_owned();
    for (i, re) in patterns.iter().enumerate() {
        out = match i {
            2 => re.replace_all(&out, "$1$2[REDACTED]").into_owned(),
            3 => re.replace_all(&out, "://[REDACTED]@").into_owned(),
            _ => re.replace_all(&out, "[REDACTED]").into_owned(),
        };
    }
    out
}

/// Masks a known-secret value keeping only a short prefix/suffix, e.g. `sk-****abcd`.
pub fn mask_value(value: &str) -> String {
    let trimmed = value.trim();
    let n = trimmed.chars().count();
    if n == 0 {
        return String::new();
    }
    if n <= 8 {
        return "*".repeat(n);
    }
    let prefix: String = trimmed.chars().take(3).collect();
    let suffix: String = trimmed.chars().skip(n - 4).collect();
    format!("{prefix}****{suffix}")
}

/// Returns the last `max_lines` lines of `text`, redacted.
pub fn tail_redacted(text: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    redact_secrets(&lines[start..].join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_openai_style_keys() {
        let s = "OPENAI_API_KEY=sk-abcdefghijklmnop1234 in env";
        assert_eq!(redact_secrets(s), "OPENAI_API_KEY=[REDACTED] in env");
    }

    #[test]
    fn redacts_bearer_and_url_credentials() {
        let s = "Authorization: Bearer abcdefghijklmnop https://user:secret@host/v1";
        let r = redact_secrets(s);
        assert!(!r.contains("abcdefghijklmnop"), "{r}");
        assert!(!r.contains("secret"), "{r}");
    }

    #[test]
    fn masks_values() {
        assert_eq!(mask_value("sk-abcdefghijkl"), "sk-****ijkl");
        assert_eq!(mask_value("short"), "*****");
        assert_eq!(mask_value(""), "");
    }

    #[test]
    fn tail_keeps_last_lines() {
        assert_eq!(tail_redacted("a\nb\nc\nd", 2), "c\nd");
    }
}
