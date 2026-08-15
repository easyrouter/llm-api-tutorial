//! Telemetry (PRD #16): aggregate funnel events (step reached / passed / failed, error class,
//! diagnosis rule hit, durations) posted to an intranet endpoint. Disabled until an endpoint is
//! configured **and** the user has not opted out. Payloads never include keys, URLs typed by the
//! user, file contents, hostnames or usernames — only the fields of `TelemetryEvent` plus
//! `session_id` (random per launch), `app_version`, `platform`, `arch`.
//!
//! TODO(impl): in-memory queue, `flush` posting `{ "events": [...] }` as JSON with a short
//! timeout, best-effort (drop on failure, cap queue at 200), periodic flush every
//! `flush_interval_seconds` and on `track` when queue >= 20.

use std::sync::Mutex;

use crate::models::{TelemetryConfig, TelemetryEvent, TelemetryStatus};

#[derive(Debug)]
pub struct Telemetry {
    enabled: Mutex<bool>,
    endpoint: String,
    session_id: String,
    queue: Mutex<Vec<TelemetryEvent>>,
    http: reqwest::Client,
}

impl Telemetry {
    pub fn new(cfg: &TelemetryConfig, http: reqwest::Client) -> Self {
        Self {
            enabled: Mutex::new(cfg.enabled && !cfg.endpoint.is_empty()),
            endpoint: cfg.endpoint.clone(),
            session_id: uuid::Uuid::new_v4().to_string(),
            queue: Mutex::new(Vec::new()),
            http,
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.endpoint.is_empty()
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.lock().is_ok_and(|g| *g)
    }

    pub fn set_enabled(&self, on: bool) {
        if let Ok(mut g) = self.enabled.lock() {
            *g = on && self.is_configured();
        }
    }

    pub fn track(&self, event: TelemetryEvent) {
        if !self.is_enabled() {
            return;
        }
        if let Ok(mut q) = self.queue.lock() {
            if q.len() < 200 {
                q.push(event);
            }
        }
        // TODO(impl): flush when q.len() >= 20 (spawn task using self.http)
    }

    pub async fn flush(&self) {
        // TODO(impl)
        let _ = &self.http;
    }

    pub fn status(&self) -> TelemetryStatus {
        TelemetryStatus {
            enabled: self.is_enabled(),
            configured: self.is_configured(),
            queued: self.queue.lock().map_or(0, |q| q.len()),
            session_id: self.session_id.clone(),
        }
    }
}
