//! Loads the company preset (`resources/app-config.json`) and merges an optional user-level
//! override (`<app_config_dir>/app-config.json`). Load order:
//!   1. runtime resource file `<resource_dir>/resources/app-config.json` (lets IT patch
//!      presets post-build) when it exists and parses;
//!   2. otherwise the compile-time embedded copy (`include_str!`);
//!   3. then, if the override file exists and parses, its **top-level** keys replace the
//!      corresponding keys of the base (shallow merge: e.g. the whole `gateway` object).
//!
//! `ConfigSource` tells the UI what happened: `Bundled` (1 or 2, no override),
//! `BundledWithOverride` (override applied), `Fallback` (a present-but-broken resource or
//! override file was ignored — the shipped defaults are in use; details are logged).
//! A broken override never prevents start-up.

use std::path::{Path, PathBuf};

use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::models::{AppConfig, ConfigSource};

/// Embedded copy of the shipped preset — guarantees the app always has a config.
pub const EMBEDDED_CONFIG: &str = include_str!("../resources/app-config.json");

/// File name of both the bundled resource and the user override.
pub const CONFIG_FILE_NAME: &str = "app-config.json";

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
    from_value(value)
}

fn from_value(value: Value) -> AppResult<AppConfig> {
    serde_json::from_value(value).map_err(|e| AppError::Config(e.to_string()))
}

/// Path of the optional user override file.
pub fn override_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join(CONFIG_FILE_NAME))
}

/// Path of the runtime copy of the bundled preset (`bundle.resources` in `tauri.conf.json`).
pub fn resource_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .resource_dir()
        .ok()
        .map(|d| d.join("resources").join(CONFIG_FILE_NAME))
}

/// Shallow merge: every top-level key of `over` replaces the same key in `base`. Non-object
/// inputs: an object `base` with a non-object `over` is returned unchanged; a non-object
/// `base` is replaced by `over` when that is an object.
pub fn merge_override(base: Value, over: Value) -> Value {
    match (base, over) {
        (Value::Object(mut b), Value::Object(o)) => {
            for (k, v) in o {
                b.insert(k, v);
            }
            Value::Object(b)
        }
        (b, Value::Object(o)) if !b.is_object() => Value::Object(o),
        (b, _) => b,
    }
}

/// Reads and parses a JSON file that may be absent. `Ok(None)` = absent, `Err` = present but
/// unreadable/invalid.
fn read_json_file(path: &Path) -> AppResult<Option<Value>> {
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&text)?;
    Ok(Some(value))
}

/// Pure resolution of the load order (see module docs) — inputs are already-read files.
/// Returns the effective config and its source.
pub fn resolve(
    resource: Option<AppResult<Value>>,
    embedded_json: &str,
    override_file: Option<AppResult<Value>>,
) -> AppResult<(AppConfig, ConfigSource)> {
    let mut degraded = false;

    let base = match resource {
        Some(Ok(value)) => {
            log::info!("config: using runtime resource file");
            value
        }
        Some(Err(e)) => {
            log::warn!("config: resource file ignored ({e}); using embedded preset");
            degraded = true;
            serde_json::from_str(embedded_json)?
        }
        None => {
            log::info!("config: using embedded preset");
            serde_json::from_str(embedded_json)?
        }
    };

    let (merged, has_override) = match override_file {
        Some(Ok(over)) => (merge_override(base.clone(), over), true),
        Some(Err(e)) => {
            log::warn!("config: override file ignored ({e})");
            degraded = true;
            (base.clone(), false)
        }
        None => (base.clone(), false),
    };

    match from_value(merged) {
        Ok(config) => {
            let source = if degraded {
                ConfigSource::Fallback
            } else if has_override {
                ConfigSource::BundledWithOverride
            } else {
                ConfigSource::Bundled
            };
            log::info!("config: source = {source:?}");
            Ok((config, source))
        }
        Err(e) if has_override => {
            log::warn!("config: override produced an invalid config ({e}); using base preset");
            let config = from_value(base)?;
            Ok((config, ConfigSource::Fallback))
        }
        Err(e) => Err(e),
    }
}

/// Loads the effective configuration for this app instance (see module docs).
pub fn load(app: &AppHandle) -> AppResult<ConfigState> {
    let resource = resource_path(app).and_then(|p| read_json_file(&p).transpose());
    let override_path = override_path(app);
    let override_file = override_path
        .as_deref()
        .and_then(|p| read_json_file(p).transpose());
    let (config, source) = resolve(resource, EMBEDDED_CONFIG, override_file)?;
    Ok(ConfigState {
        config,
        source,
        override_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn embedded_config_parses() {
        let cfg = embedded().expect("embedded config must parse");
        assert_eq!(cfg.schema_version, 1);
        assert_eq!(cfg.tools.len(), 2);
    }

    #[test]
    fn merge_override_replaces_top_level_keys_only() {
        let base = json!({"a": {"x": 1, "y": 2}, "b": 1, "c": true});
        let over = json!({"a": {"x": 9}, "d": "new"});
        let merged = merge_override(base, over);
        assert_eq!(
            merged,
            json!({"a": {"x": 9}, "b": 1, "c": true, "d": "new"})
        );
    }

    #[test]
    fn merge_override_ignores_non_object_override() {
        let base = json!({"a": 1});
        assert_eq!(merge_override(base.clone(), json!(null)), base);
        assert_eq!(merge_override(base.clone(), json!([1, 2])), base);
        assert_eq!(
            merge_override(json!(null), json!({"a": 1})),
            json!({"a": 1})
        );
        assert_eq!(merge_override(json!(3), json!("x")), json!(3));
    }

    #[test]
    fn partial_override_replaces_gateway_object() {
        let over = json!({
            "gateway": {
                "baseUrl": "https://gw.corp.example/v1",
                "protocol": "responses",
                "presetProviderName": "Corp"
            }
        });
        let (cfg, source) = resolve(None, EMBEDDED_CONFIG, Some(Ok(over))).expect("resolve");
        assert_eq!(source, ConfigSource::BundledWithOverride);
        assert_eq!(cfg.gateway.base_url, "https://gw.corp.example/v1");
        assert_eq!(cfg.gateway.preset_provider_name, "Corp");
        // shallow merge: fields omitted from the overriding object take serde defaults
        assert_eq!(cfg.gateway.default_model, "");
        // untouched sections survive
        assert_eq!(cfg.tools.len(), 2);
    }

    #[test]
    fn resource_file_wins_over_embedded() {
        let mut resource: Value = serde_json::from_str(EMBEDDED_CONFIG).expect("json");
        resource["company"]["name"] = json!("Resource Corp");
        let (cfg, source) = resolve(Some(Ok(resource)), EMBEDDED_CONFIG, None).expect("resolve");
        assert_eq!(source, ConfigSource::Bundled);
        assert_eq!(cfg.company.name, "Resource Corp");
    }

    #[test]
    fn broken_resource_falls_back_to_embedded_and_reports_fallback() {
        let broken = Some(Err(AppError::Serde("bad json".into())));
        let (cfg, source) = resolve(broken, EMBEDDED_CONFIG, None).expect("resolve");
        assert_eq!(source, ConfigSource::Fallback);
        assert_eq!(cfg.schema_version, 1);
    }

    #[test]
    fn broken_override_is_ignored() {
        let broken = Some(Err(AppError::Serde("bad json".into())));
        let (cfg, source) = resolve(None, EMBEDDED_CONFIG, broken).expect("resolve");
        assert_eq!(source, ConfigSource::Fallback);
        assert_eq!(cfg.gateway.base_url, "https://gateway.example.com/v1");
    }

    #[test]
    fn override_that_breaks_the_schema_is_ignored() {
        let over = json!({"gateway": {"baseUrl": 42}});
        let (cfg, source) = resolve(None, EMBEDDED_CONFIG, Some(Ok(over))).expect("resolve");
        assert_eq!(source, ConfigSource::Fallback);
        assert_eq!(cfg.gateway.base_url, "https://gateway.example.com/v1");
    }

    #[test]
    fn no_files_means_bundled() {
        let (_, source) = resolve(None, EMBEDDED_CONFIG, None).expect("resolve");
        assert_eq!(source, ConfigSource::Bundled);
    }

    #[test]
    fn unparsable_embedded_is_an_error() {
        assert!(resolve(None, "{ not json", None).is_err());
    }

    #[test]
    fn read_json_file_distinguishes_absent_from_broken() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("none.json");
        assert!(matches!(read_json_file(&missing), Ok(None)));
        let broken = dir.path().join("broken.json");
        std::fs::write(&broken, "{").expect("write");
        assert!(read_json_file(&broken).is_err());
        let good = dir.path().join("good.json");
        std::fs::write(&good, "{\"a\":1}").expect("write");
        assert_eq!(read_json_file(&good).expect("ok"), Some(json!({"a": 1})));
    }
}
