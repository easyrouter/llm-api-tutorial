//! Application error type. Serialised to the frontend as `{ code, message, params }`
//! so the UI can show a localised message (`errors.<code>`) and log the detail.

use serde::Serialize;

use crate::models::Params;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("command `{program}` not found")]
    CommandNotFound { program: String },
    #[error("command `{program}` failed with exit code {code:?}: {stderr_tail}")]
    CommandFailed {
        program: String,
        code: Option<i32>,
        stderr_tail: String,
    },
    #[error("command `{program}` timed out after {timeout_ms} ms")]
    CommandTimeout { program: String, timeout_ms: u64 },
    #[error("network error: {0}")]
    Network(String),
    #[error("http {status} from {url}")]
    Http { status: u16, url: String },
    #[error("hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("not supported on this platform: {0}")]
    Unsupported(String),
    #[error("job {0} not found")]
    JobNotFound(String),
    #[error("serialization error: {0}")]
    Serde(String),
    #[error("{0}")]
    Other(String),
}

impl AppError {
    /// Stable, machine-readable code used as an i18n key suffix on the frontend.
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Config(_) => "config",
            AppError::Io(_) => "io",
            AppError::CommandNotFound { .. } => "command_not_found",
            AppError::CommandFailed { .. } => "command_failed",
            AppError::CommandTimeout { .. } => "command_timeout",
            AppError::Network(_) => "network",
            AppError::Http { .. } => "http",
            AppError::HashMismatch { .. } => "hash_mismatch",
            AppError::InvalidInput(_) => "invalid_input",
            AppError::Unsupported(_) => "unsupported",
            AppError::JobNotFound(_) => "job_not_found",
            AppError::Serde(_) => "serde",
            AppError::Other(_) => "other",
        }
    }

    fn params(&self) -> Params {
        let mut p = Params::new();
        match self {
            AppError::CommandNotFound { program } => {
                p.insert("program".into(), program.clone());
            }
            AppError::CommandFailed { program, code, .. } => {
                p.insert("program".into(), program.clone());
                if let Some(c) = code {
                    p.insert("code".into(), c.to_string());
                }
            }
            AppError::CommandTimeout {
                program,
                timeout_ms,
            } => {
                p.insert("program".into(), program.clone());
                p.insert("timeoutMs".into(), timeout_ms.to_string());
            }
            AppError::Http { status, url } => {
                p.insert("status".into(), status.to_string());
                p.insert("url".into(), url.clone());
            }
            // The payload is a fully qualified i18n key naming *why* it is unsupported;
            // without it the UI can only show the generic "not supported on this platform".
            AppError::Unsupported(reason) => {
                p.insert("reason".into(), reason.clone());
            }
            _ => {}
        }
        p
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        // reqwest errors can embed the full URL (which may carry a key in a query string);
        // keep only the error kind description.
        let kind = if e.is_timeout() {
            "timeout"
        } else if e.is_connect() {
            "connect"
        } else if e.is_request() {
            "request"
        } else if e.is_body() || e.is_decode() {
            "body"
        } else {
            "unknown"
        };
        AppError::Network(kind.to_owned())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Serde(e.to_string())
    }
}

/// Wire format for errors crossing the IPC boundary.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WireError<'a> {
    code: &'a str,
    message: String,
    params: Params,
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WireError {
            code: self.code(),
            message: self.to_string(),
            params: self.params(),
        }
        .serialize(serializer)
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    /// `Unsupported` names *why* something is unavailable with a fully qualified i18n key.
    /// If that key stops at the IPC boundary the frontend can only render the generic
    /// `errors.unsupported` sentence, which is what made a timed-out AppX probe look like
    /// "not supported on this platform".
    #[test]
    fn unsupported_carries_its_reason_to_the_frontend() {
        let json = serde_json::to_value(AppError::Unsupported("common:errors.network".to_owned()))
            .expect("serialize");
        assert_eq!(json["code"], "unsupported");
        assert_eq!(json["params"]["reason"], "common:errors.network");
    }
}
