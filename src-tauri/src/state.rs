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
    pub started_at: Instant,
}

impl AppState {
    pub fn init(app: &AppHandle) -> AppResult<Self> {
        let cfg = config::load(app)?;
        let http = crate::net::build_client()?;
        let telemetry = Telemetry::new(&cfg.config.telemetry, http.clone());
        Ok(Self {
            config: RwLock::new(cfg),
            jobs: JobRegistry::default(),
            telemetry,
            http,
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
