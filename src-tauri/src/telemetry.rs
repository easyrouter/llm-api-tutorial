//! Telemetry (PRD #16): aggregate funnel events (step reached / passed / failed, error class,
//! diagnosis rule hit, durations) posted to an intranet endpoint. Disabled until an endpoint is
//! configured **and** the user has not opted out. Payloads never include keys, URLs typed by the
//! user, file contents, hostnames or usernames — only the fields of `TelemetryEvent` plus
//! `sessionId` (random per launch), `appVersion`, `platform`, `arch`
//! (schema: docs/ARCHITECTURE.md §7).
//!
//! Behaviour
//! - `track` enqueues (queue capped at [`QUEUE_CAP`]); once [`FLUSH_THRESHOLD`] events are
//!   queued a background flush is spawned.
//! - `flush` posts batches of at most [`BATCH_SIZE`] events with a 5 s timeout. A batch is
//!   retried **once** on a transport error (timeout / connect); on any other failure — or a
//!   second transport failure — it is dropped with a `debug` log. Nothing is ever re-queued.
//! - Concurrent flushes are collapsed: while one is running, others return immediately.
//! - Disabling telemetry drops everything queued.
//! - [`Telemetry::spawn_periodic_flush`] starts a background loop (call from `lib.rs` after the
//!   state is managed; a final `flush().await` on exit is optional and best-effort).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;

use crate::models::{TelemetryConfig, TelemetryEvent, TelemetryStatus};

/// Maximum number of events held in memory; newer events are dropped when full.
pub const QUEUE_CAP: usize = 200;
/// Queue length at which `track` spawns a background flush.
pub const FLUSH_THRESHOLD: usize = 20;
/// Maximum events per POST.
pub const BATCH_SIZE: usize = 50;
/// Timeout for one POST.
pub const POST_TIMEOUT: Duration = Duration::from_secs(5);
/// Lower bound for the periodic flush interval.
pub const MIN_FLUSH_INTERVAL: Duration = Duration::from_secs(5);
/// Upper bound on batches drained by one `flush` call (`QUEUE_CAP / BATCH_SIZE`).
const MAX_BATCHES_PER_FLUSH: usize = QUEUE_CAP.div_ceil(BATCH_SIZE);

/// Opt-in, best-effort event sink. Cheap to share: all state lives behind an `Arc`.
#[derive(Debug)]
pub struct Telemetry {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    enabled: AtomicBool,
    endpoint: String,
    session_id: String,
    flush_interval: Duration,
    queue: Mutex<Vec<TelemetryEvent>>,
    /// Held for the duration of a flush so concurrent flushes collapse into one.
    flush_lock: tokio::sync::Mutex<()>,
    http: reqwest::Client,
}

/// Wire payload — mirrors docs/ARCHITECTURE.md §7.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Payload<'a> {
    session_id: &'a str,
    app_version: &'static str,
    platform: &'static str,
    arch: &'static str,
    events: &'a [TelemetryEvent],
}

impl Telemetry {
    /// Builds the sink from the company preset. Enabled only when the preset says so **and**
    /// an endpoint is configured; the user can still opt out via [`Telemetry::set_enabled`].
    pub fn new(cfg: &TelemetryConfig, http: reqwest::Client) -> Self {
        let endpoint = cfg.endpoint.trim().to_owned();
        Self {
            inner: Arc::new(Inner {
                enabled: AtomicBool::new(cfg.enabled && !endpoint.is_empty()),
                endpoint,
                session_id: uuid::Uuid::new_v4().to_string(),
                flush_interval: Duration::from_secs(cfg.flush_interval_seconds)
                    .max(MIN_FLUSH_INTERVAL),
                queue: Mutex::new(Vec::new()),
                flush_lock: tokio::sync::Mutex::new(()),
                http,
            }),
        }
    }

    /// `true` when an endpoint is configured (regardless of the user's opt-in state).
    pub fn is_configured(&self) -> bool {
        !self.inner.endpoint.is_empty()
    }

    /// `true` when events are currently being collected.
    pub fn is_enabled(&self) -> bool {
        self.inner.enabled.load(Ordering::Relaxed)
    }

    /// Opts in or out. Opting out discards everything still queued.
    pub fn set_enabled(&self, on: bool) {
        let effective = on && self.is_configured();
        self.inner.enabled.store(effective, Ordering::Relaxed);
        if !effective {
            self.inner.clear();
        }
    }

    /// Enqueues `event` (no-op when disabled) and spawns a background flush once the queue
    /// reaches [`FLUSH_THRESHOLD`].
    pub fn track(&self, event: TelemetryEvent) {
        if self.inner.enqueue(event) {
            let inner = Arc::clone(&self.inner);
            tauri::async_runtime::spawn(async move { inner.flush().await });
        }
    }

    /// Posts everything queued (in batches). Best-effort; never fails.
    pub async fn flush(&self) {
        self.inner.flush().await;
    }

    /// Snapshot for the UI.
    pub fn status(&self) -> TelemetryStatus {
        TelemetryStatus {
            enabled: self.is_enabled(),
            configured: self.is_configured(),
            queued: self.inner.len(),
            session_id: self.inner.session_id.clone(),
        }
    }

    /// Configured periodic flush interval (clamped to at least [`MIN_FLUSH_INTERVAL`]).
    pub fn flush_interval(&self) -> Duration {
        self.inner.flush_interval
    }

    /// Starts a background loop that flushes every `interval` (clamped to
    /// [`MIN_FLUSH_INTERVAL`]). Returns `None` when no endpoint is configured. The handle can
    /// be aborted on shutdown; the loop is otherwise detached.
    pub fn spawn_periodic_flush(
        &self,
        interval: Duration,
    ) -> Option<tauri::async_runtime::JoinHandle<()>> {
        if !self.is_configured() {
            return None;
        }
        let interval = interval.max(MIN_FLUSH_INTERVAL);
        let inner = Arc::clone(&self.inner);
        Some(tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                inner.flush().await;
            }
        }))
    }
}

impl Inner {
    fn len(&self) -> usize {
        self.queue.lock().map_or(0, |q| q.len())
    }

    fn clear(&self) {
        if let Ok(mut q) = self.queue.lock() {
            q.clear();
        }
    }

    /// Pushes `event` unless disabled or full. Returns `true` when a flush is due.
    fn enqueue(&self, event: TelemetryEvent) -> bool {
        if !self.enabled.load(Ordering::Relaxed) {
            return false;
        }
        let Ok(mut q) = self.queue.lock() else {
            return false;
        };
        if q.len() < QUEUE_CAP {
            q.push(event);
        }
        q.len() >= FLUSH_THRESHOLD
    }

    /// Removes and returns the oldest `BATCH_SIZE` events.
    fn take_batch(&self) -> Vec<TelemetryEvent> {
        let Ok(mut q) = self.queue.lock() else {
            return Vec::new();
        };
        let n = q.len().min(BATCH_SIZE);
        q.drain(..n).collect()
    }

    async fn flush(&self) {
        // A flush already in progress will pick up whatever is queued; nothing to do here.
        let Ok(_guard) = self.flush_lock.try_lock() else {
            return;
        };
        if self.endpoint.is_empty() {
            self.clear();
            return;
        }
        for _ in 0..MAX_BATCHES_PER_FLUSH {
            let batch = self.take_batch();
            if batch.is_empty() {
                break;
            }
            self.post(&batch).await;
        }
    }

    fn payload<'a>(&'a self, events: &'a [TelemetryEvent]) -> Payload<'a> {
        Payload {
            session_id: &self.session_id,
            app_version: env!("CARGO_PKG_VERSION"),
            platform: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            events,
        }
    }

    /// One attempt plus at most one retry on a transport error; the batch is dropped after that.
    async fn post(&self, events: &[TelemetryEvent]) {
        let payload = self.payload(events);
        match self.send_once(&payload).await {
            Ok(()) => {}
            Err(SendError::Transport(first)) => {
                if let Err(second) = self.send_once(&payload).await {
                    log::debug!(
                        "telemetry: dropped {} events after retry ({first}; {second})",
                        events.len()
                    );
                }
            }
            Err(SendError::Status(status)) => {
                log::debug!(
                    "telemetry: dropped {} events, endpoint answered {status}",
                    events.len()
                );
            }
        }
    }

    async fn send_once(&self, payload: &Payload<'_>) -> Result<(), SendError> {
        let resp = self
            .http
            .post(&self.endpoint)
            .timeout(POST_TIMEOUT)
            .json(payload)
            .send()
            .await
            .map_err(|e| SendError::Transport(crate::error::AppError::from(e).to_string()))?;
        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            Err(SendError::Status(status.as_u16()))
        }
    }
}

#[derive(Debug)]
enum SendError {
    /// Timeout / connect / request build failure (description only — never the URL).
    Transport(String),
    /// Endpoint reachable but answered a non-2xx status.
    Status(u16),
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SendError::Transport(s) => write!(f, "transport: {s}"),
            SendError::Status(code) => write!(f, "http {code}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::WizardStep;

    fn cfg(enabled: bool, endpoint: &str) -> TelemetryConfig {
        TelemetryConfig {
            enabled,
            endpoint: endpoint.into(),
            flush_interval_seconds: 30,
        }
    }

    fn event(name: &str) -> TelemetryEvent {
        TelemetryEvent {
            name: name.into(),
            step: Some(WizardStep::Verify),
            status: Some("fail".into()),
            duration_ms: Some(12),
            error_class: None,
            rule_id: Some("A".into()),
        }
    }

    /// Endpoint on a closed local port: transport failures only.
    const DEAD_ENDPOINT: &str = "http://127.0.0.1:1/telemetry";

    /// Short connect timeout so the closed-port endpoint fails fast on every platform.
    fn test_client() -> reqwest::Client {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_millis(300))
            .build()
            .expect("client")
    }

    #[test]
    fn disabled_without_endpoint_and_nothing_is_queued() {
        let t = Telemetry::new(&cfg(true, ""), test_client());
        assert!(!t.is_configured());
        assert!(!t.is_enabled());
        t.track(event("step_result"));
        assert_eq!(t.status().queued, 0);
        // opting in cannot enable an unconfigured sink
        t.set_enabled(true);
        assert!(!t.is_enabled());
    }

    #[test]
    fn opt_out_disables_and_drops_queue() {
        let t = Telemetry::new(&cfg(true, DEAD_ENDPOINT), test_client());
        assert!(t.is_enabled());
        assert!(!t.inner.enqueue(event("a")));
        assert_eq!(t.status().queued, 1);
        t.set_enabled(false);
        assert!(!t.is_enabled());
        assert_eq!(t.status().queued, 0);
        assert!(!t.inner.enqueue(event("b")));
        assert_eq!(t.status().queued, 0);
        t.set_enabled(true);
        assert!(t.is_enabled());
    }

    #[test]
    fn queue_is_capped_and_signals_flush_at_threshold() {
        let t = Telemetry::new(&cfg(true, DEAD_ENDPOINT), test_client());
        for i in 1..=(QUEUE_CAP + 50) {
            let due = t.inner.enqueue(event("e"));
            assert_eq!(due, i >= FLUSH_THRESHOLD, "event #{i}");
        }
        assert_eq!(t.status().queued, QUEUE_CAP);
    }

    #[test]
    fn status_reports_all_fields() {
        let t = Telemetry::new(&cfg(false, DEAD_ENDPOINT), test_client());
        let s = t.status();
        assert!(!s.enabled);
        assert!(s.configured);
        assert_eq!(s.queued, 0);
        assert_eq!(s.session_id.len(), 36, "uuid v4 string");
        assert!(uuid::Uuid::parse_str(&s.session_id).is_ok());
        assert_eq!(t.flush_interval(), Duration::from_secs(30));
    }

    #[test]
    fn payload_matches_documented_schema() {
        let t = Telemetry::new(&cfg(true, DEAD_ENDPOINT), test_client());
        let events = [event("step_result")];
        let value = serde_json::to_value(t.inner.payload(&events)).expect("serialise");
        let obj = value.as_object().expect("object");
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["appVersion", "arch", "events", "platform", "sessionId"]
        );
        assert_eq!(obj["appVersion"], env!("CARGO_PKG_VERSION"));
        assert_eq!(obj["platform"], std::env::consts::OS);
        let ev = &obj["events"][0];
        assert_eq!(ev["name"], "step_result");
        assert_eq!(ev["step"], "verify");
        assert_eq!(ev["ruleId"], "A");
        assert_eq!(ev["durationMs"], 12);
    }

    #[test]
    fn take_batch_drains_oldest_first_up_to_batch_size() {
        let t = Telemetry::new(&cfg(true, DEAD_ENDPOINT), test_client());
        for i in 0..(BATCH_SIZE + 5) {
            t.inner.enqueue(event(&format!("e{i}")));
        }
        let batch = t.inner.take_batch();
        assert_eq!(batch.len(), BATCH_SIZE);
        assert_eq!(batch[0].name, "e0");
        assert_eq!(t.status().queued, 5);
        assert_eq!(t.inner.take_batch().len(), 5);
        assert!(t.inner.take_batch().is_empty());
    }

    #[tokio::test]
    async fn flush_drops_batches_when_endpoint_is_unreachable() {
        let t = Telemetry::new(&cfg(true, DEAD_ENDPOINT), test_client());
        for _ in 0..(BATCH_SIZE + 3) {
            t.inner.enqueue(event("e"));
        }
        t.flush().await;
        assert_eq!(
            t.status().queued,
            0,
            "failed batches are dropped, never re-queued"
        );
        // flushing an empty queue is a no-op
        t.flush().await;
        assert_eq!(t.status().queued, 0);
    }

    #[test]
    fn periodic_flush_needs_an_endpoint() {
        let t = Telemetry::new(&cfg(true, ""), test_client());
        assert!(t.spawn_periodic_flush(Duration::from_secs(1)).is_none());
    }
}
