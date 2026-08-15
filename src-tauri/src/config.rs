//! Loads the company preset (`resources/app-config.json`) and merges an optional user-level
//! override (`<app_config_dir>/app-config.json`). Load order:
//!   1. runtime resource file (lets IT patch presets post-build)
//!   2. compile-time embedded copy (`include_str!`) as fallback
//!   3. shallow-merge override file on top, if present
//!
//! TODO(impl): implement `load` per the above; the stub below only uses the embedded copy.

use std::path::PathBuf;

use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::models::{AppConfig, ConfigSource};

/// Embedded copy of the shipped preset — guarantees the app always has a config.
pub const EMBEDDED_CONFIG: &str = include_str!("../resources/app-config.json");

#[derive(Debug, Clone)]
pub struct ConfigState {
    pub config: AppConfig,
    pub source: ConfigSource,
    pub override_path: Option<PathBuf>,
}

pub fn embedded() -> AppResult<AppConfig> {
    parse(EMBEDDED_CONFIG)
}

pub fn parse(json: &str) -> AppResult<AppConfig> {
    let value: Value = serde_json::from_str(json)?;
    serde_json::from_value(value).map_err(|e| AppError::Config(e.to_string()))
}

/// Path of the optional user override file.
pub fn override_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join("app-config.json"))
}

pub fn load(app: &AppHandle) -> AppResult<ConfigState> {
    let _ = app;
    // TODO(impl): resource file -> embedded -> merge override (shallow merge of top-level objects)
    Ok(ConfigState {
        config: embedded()?,
        source: ConfigSource::Fallback,
        override_path: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_config_parses() {
        let cfg = embedded().expect("embedded config must parse");
        assert_eq!(cfg.schema_version, 1);
        assert_eq!(cfg.tools.len(), 2);
    }
}
