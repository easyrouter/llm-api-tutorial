//! HTTP client, reachability probes, mirror selection and verified downloads.
//!
//! TODO(impl): `probe`, `choose_mirrors`, `download`. Rules:
//! - honour system/env proxies (reqwest default), rustls only, sane timeouts, custom UA
//! - `probe` = HEAD (fallback GET on 405) with `probe_timeout_ms`; reachable when any HTTP
//!   response arrives (even 4xx) — we test *reachability*, not authorisation
//! - `choose_mirrors` probes all entries concurrently and picks the lowest-latency reachable
//!   one per category, preferring `official` on ties; falls back to first entry when none reachable
//! - `download` streams to `dest_dir/<file_name>`, calls `on_progress(downloaded, total)`,
//!   computes SHA-256, and compares to `expected_sha256` when provided (`HashMismatch` on failure).

use std::path::PathBuf;
use std::time::Duration;

use crate::error::{AppError, AppResult};
use crate::models::{DownloadRequest, DownloadResult, MirrorChoice, Mirrors, ProbeResult};

pub const USER_AGENT: &str = concat!("codex-onboarding/", env!("CARGO_PKG_VERSION"));

pub fn build_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| AppError::Network(e.to_string()))
}

pub async fn probe(
    client: &reqwest::Client,
    id: &str,
    url: &str,
    timeout: Duration,
) -> ProbeResult {
    let _ = (client, timeout);
    ProbeResult {
        id: id.to_owned(),
        url: url.to_owned(),
        reachable: false,
        latency_ms: None,
        http_status: None,
        error: Some("not implemented".into()),
    }
}

pub async fn choose_mirrors(client: &reqwest::Client, mirrors: &Mirrors) -> MirrorChoice {
    let _ = client;
    MirrorChoice {
        npm_registry: mirrors.npm_registries[0].clone(),
        node_dist: mirrors.node_dist[0].clone(),
        probes: Vec::new(),
    }
}

pub async fn download<F>(
    client: &reqwest::Client,
    req: &DownloadRequest,
    dest_dir: PathBuf,
    on_progress: F,
) -> AppResult<DownloadResult>
where
    F: FnMut(u64, Option<u64>) + Send,
{
    let _ = (client, req, dest_dir, on_progress);
    Err(AppError::Other("net::download not implemented".into()))
}
