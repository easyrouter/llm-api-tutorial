//! Process-wide state managed by Tauri (`app.state::<AppState>()`).

use std::sync::RwLock;
use std::time::Instant;

use tauri::AppHandle;

use crate::config::{self, ConfigState};
use crate::error::AppResult;
use crate::install::JobRegistry;
use crate::telemetry::Telemetry;

#[derive(Debug)]
pub struct AppState {
    pub config: RwLock<ConfigState>,
    pub jobs: JobRegistry,
    pub telemetry: Telemetry,
    pub http: reqwest::Client,
    /// Client for the gateway probes only (`verify::GATEWAY_TIMEOUT` read + overall timeout), so
    /// a slow-thinking model is not cut off by the shared client's 30 s read timeout.
    pub http_gateway: reqwest::Client,
    pub started_at: Instant,
}

impl AppState {
    pub fn init(app: &AppHandle) -> AppResult<Self> {
        let cfg = config::load(app)?;
        let http = crate::net::build_client()?;
        let http_gateway = crate::net::gateway_client(crate::verify::GATEWAY_TIMEOUT)?;
        let telemetry = Telemetry::new(&cfg.config.telemetry, http.clone());
        Ok(Self {
            config: RwLock::new(cfg),
            jobs: JobRegistry::default(),
            telemetry,
            http,
            http_gateway,
            started_at: Instant::now(),
        })
    }

    /// Cloned snapshot of the current configuration (cheap; config is small).
    pub fn config_snapshot(&self) -> ConfigState {
        match self.config.read() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}
