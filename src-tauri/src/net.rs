//! HTTP client, reachability probes, mirror selection and verified downloads.
//!
//! Rules
//! - One shared `reqwest::Client` (rustls, system/env proxies honoured, custom UA); every HTTP
//!   request in the crate goes through a client built here. Two variants differ only in their
//!   timeouts: [`download_client`] (no overall deadline) and [`gateway_client`] (long read
//!   timeout, because an LLM gateway may hold the response for tens of seconds).
//! - `probe` = HEAD (GET on 405/501) with a per-probe timeout; *reachable* means any HTTP
//!   response arrived, even 4xx — we test reachability, not authorisation. Error strings are
//!   short, redacted and never contain the URL.
//! - `choose_mirrors` probes every entry concurrently and picks the lowest-latency reachable one
//!   per category, preferring `official` when it is within [`PREFER_OFFICIAL_MARGIN_MS`];
//!   falls back to the first entry when nothing is reachable. `choose_mirrors_avoiding` is the
//!   retry variant: the npm registry an install just failed on is left out of the choice (PRD
//!   M2 "switch mirror when the network is slow"), unless it is the only one configured.
//! - `download` accepts https only (and refuses a redirect that leaves https), writes to
//!   `dest_dir/<file_name>.part` and renames on success, streams SHA-256, reports progress
//!   (throttled) and rejects on hash mismatch. Installer downloads use [`download_client`],
//!   which additionally lets reqwest refuse any non-https hop (`https_only`).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures_util::future::join_all;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::error::{AppError, AppResult};
use crate::models::{
    DownloadRequest, DownloadResult, MirrorChoice, MirrorEntry, Mirrors, ProbeResult,
};
use crate::redact::redact_secrets;

pub const USER_AGENT: &str = concat!("seedrouter-onboarding/", env!("CARGO_PKG_VERSION"));

/// Default overall request timeout of the shared client (downloads use streaming and are
/// bounded per chunk by the read timeout instead).
pub const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// Time allowed for the TCP + TLS handshake on every client.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Default read timeout: a transfer (or a server) that goes quiet for this long is treated as
/// stalled. It also bounds the wait for the *response headers*, so a client that must tolerate a
/// slow-thinking upstream needs its own value — see [`gateway_client`].
const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// The `official` mirror wins over a faster one when it is at most this much slower.
pub const PREFER_OFFICIAL_MARGIN_MS: u64 = 150;

/// Probe timeout used when the config value is `0` (misconfiguration guard).
const FALLBACK_PROBE_TIMEOUT: Duration = Duration::from_secs(4);

/// Progress callbacks are throttled to at most one per this interval …
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
/// … unless this many bytes arrived since the last report.
const PROGRESS_BYTES: u64 = 256 * 1024;

/// Maximum characters kept from an upstream error message.
const MAX_ERROR_LEN: usize = 120;

/// Maximum accepted download file name length.
const MAX_FILE_NAME_LEN: usize = 200;

/// Builds the shared client (60 s request timeout).
pub fn build_client() -> AppResult<reqwest::Client> {
    client_with_timeout(DEFAULT_TIMEOUT_SECS)
}

/// Builds a client with a custom overall request timeout (seconds); `0` disables it (used
/// for long downloads, which are still bounded by the 30 s read timeout).
pub fn client_with_timeout(secs: u64) -> AppResult<reqwest::Client> {
    client_builder(secs)
        .build()
        .map_err(|e| AppError::Network(short_error(&e)))
}

/// Client for installer downloads: no overall timeout (installers are tens of MB on slow
/// links; the read timeout still bounds stalls) and `https_only`, so an `https → http`
/// redirect from a mirror is refused by reqwest instead of silently followed (hard rule 7).
pub fn download_client() -> AppResult<reqwest::Client> {
    client_builder(0)
        .https_only(true)
        .build()
        .map_err(|e| AppError::Network(short_error(&e)))
}

/// Client for gateway probes: an LLM gateway can think for tens of seconds before the first
/// response byte, and reqwest applies the read timeout while waiting for the response headers
/// too — so a 30 s read timeout would cut a probe short well before its own deadline. Both
/// bounds are therefore `timeout` (callers pass `verify::GATEWAY_TIMEOUT`).
pub fn gateway_client(timeout: Duration) -> AppResult<reqwest::Client> {
    client_builder_with(Some(timeout), timeout)
        .build()
        .map_err(|e| AppError::Network(short_error(&e)))
}

/// Common builder: rustls, system/env proxies, custom UA, connect/read timeouts.
fn client_builder(overall_timeout_secs: u64) -> reqwest::ClientBuilder {
    let overall = if overall_timeout_secs > 0 {
        Some(Duration::from_secs(overall_timeout_secs))
    } else {
        None
    };
    client_builder_with(overall, DEFAULT_READ_TIMEOUT)
}

fn client_builder_with(overall: Option<Duration>, read: Duration) -> reqwest::ClientBuilder {
    let mut builder = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(read);
    if let Some(overall) = overall {
        builder = builder.timeout(overall);
    }
    builder
}

// ---------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------

/// Short, redacted description of a reqwest error without the URL
/// (`kind: innermost cause`).
pub fn short_error(e: &reqwest::Error) -> String {
    let kind = if e.is_timeout() {
        "timeout"
    } else if e.is_connect() {
        "connect"
    } else if e.is_redirect() {
        "redirect"
    } else if e.is_status() {
        "status"
    } else if e.is_body() || e.is_decode() {
        "body"
    } else if e.is_request() {
        "request"
    } else {
        "unknown"
    };
    match innermost_cause(e) {
        Some(detail) if !detail.is_empty() => {
            format!(
                "{kind}: {}",
                truncate_chars(&redact_secrets(&detail), MAX_ERROR_LEN)
            )
        }
        _ => kind.to_owned(),
    }
}

/// Message of the deepest error in the `source()` chain (`None` when there is none).
fn innermost_cause(e: &reqwest::Error) -> Option<String> {
    let mut cause = std::error::Error::source(e)?;
    while let Some(next) = cause.source() {
        cause = next;
    }
    Some(cause.to_string())
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

fn elapsed_ms(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// HEAD (then GET when the server answers 405/501) with `timeout`. Any HTTP response counts as
/// reachable; the latency covers the whole exchange.
pub async fn probe(
    client: &reqwest::Client,
    id: &str,
    url: &str,
    timeout: Duration,
) -> ProbeResult {
    let started = Instant::now();
    let mut response = client.head(url).timeout(timeout).send().await;
    if let Ok(r) = &response {
        if matches!(r.status().as_u16(), 405 | 501) {
            response = client.get(url).timeout(timeout).send().await;
        }
    }
    let latency = elapsed_ms(started);
    match response {
        Ok(r) => ProbeResult {
            id: id.to_owned(),
            url: url.to_owned(),
            reachable: true,
            latency_ms: Some(latency),
            http_status: Some(r.status().as_u16()),
            error: None,
        },
        Err(e) => ProbeResult {
            id: id.to_owned(),
            url: url.to_owned(),
            reachable: false,
            latency_ms: None,
            http_status: None,
            error: Some(short_error(&e)),
        },
    }
}

/// Picks the mirror to use from `entries` given their probe results (matched by URL):
/// lowest latency among reachable entries, `official` when within
/// [`PREFER_OFFICIAL_MARGIN_MS`] of the fastest, first entry when none is reachable.
/// Returns `None` only for an empty list.
pub fn select_mirror(entries: &[MirrorEntry], probes: &[ProbeResult]) -> Option<MirrorEntry> {
    let latency_of = |entry: &MirrorEntry| {
        probes
            .iter()
            .find(|p| p.url == entry.url && p.reachable)
            .and_then(|p| p.latency_ms)
    };
    let reachable: Vec<(&MirrorEntry, u64)> = entries
        .iter()
        .filter_map(|e| latency_of(e).map(|l| (e, l)))
        .collect();
    let fastest = reachable.iter().min_by_key(|(_, l)| *l);
    let Some((fastest_entry, fastest_latency)) = fastest else {
        return entries.first().cloned();
    };
    let official = reachable
        .iter()
        .find(|(e, _)| e.id == "official")
        .filter(|(_, l)| *l <= fastest_latency.saturating_add(PREFER_OFFICIAL_MARGIN_MS));
    Some(official.map_or_else(|| (*fastest_entry).clone(), |(e, _)| (*e).clone()))
}

fn probe_timeout(mirrors: &Mirrors) -> Duration {
    if mirrors.probe_timeout_ms == 0 {
        FALLBACK_PROBE_TIMEOUT
    } else {
        Duration::from_millis(mirrors.probe_timeout_ms)
    }
}

fn default_npm_registry() -> MirrorEntry {
    MirrorEntry {
        id: "official".to_owned(),
        url: "https://registry.npmjs.org/".to_owned(),
        download_page: String::new(),
    }
}

fn default_node_dist() -> MirrorEntry {
    MirrorEntry {
        id: "official".to_owned(),
        url: "https://nodejs.org/dist/".to_owned(),
        download_page: "https://nodejs.org/en/download".to_owned(),
    }
}

/// Probes every entry concurrently; results are in input order.
async fn probe_all(
    client: &reqwest::Client,
    entries: &[MirrorEntry],
    timeout: Duration,
) -> Vec<ProbeResult> {
    join_all(
        entries
            .iter()
            .map(|m| probe(client, &m.id, &m.url, timeout)),
    )
    .await
}

/// Probes all npm registries and Node dist mirrors concurrently and picks one per category
/// (see [`select_mirror`]). All probe results are returned for display.
pub async fn choose_mirrors(client: &reqwest::Client, mirrors: &Mirrors) -> MirrorChoice {
    choose_mirrors_avoiding(client, mirrors, None).await
}

/// [`choose_mirrors`] that leaves the npm registry `avoid_npm_registry` (an entry id) out of the
/// npm choice — used when re-planning after an install failed on that registry. When it is the
/// only registry configured it is still chosen (there is nothing to switch to).
pub async fn choose_mirrors_avoiding(
    client: &reqwest::Client,
    mirrors: &Mirrors,
    avoid_npm_registry: Option<&str>,
) -> MirrorChoice {
    let timeout = probe_timeout(mirrors);
    let (npm_probes, node_probes) = tokio::join!(
        probe_all(client, &mirrors.npm_registries, timeout),
        probe_all(client, &mirrors.node_dist, timeout)
    );
    let npm_candidates = registries_avoiding(&mirrors.npm_registries, avoid_npm_registry);
    let npm_registry =
        select_mirror(&npm_candidates, &npm_probes).unwrap_or_else(default_npm_registry);
    let node_dist =
        select_mirror(&mirrors.node_dist, &node_probes).unwrap_or_else(default_node_dist);
    log::info!(
        "mirror choice: npm={} node={} (avoiding {:?})",
        npm_registry.id,
        node_dist.id,
        avoid_npm_registry
    );
    let mut probes = npm_probes;
    probes.extend(node_probes);
    MirrorChoice {
        npm_registry,
        node_dist,
        probes,
    }
}

/// `entries` without the one whose id is `avoid` — unless that would leave nothing. Pure.
pub fn registries_avoiding(entries: &[MirrorEntry], avoid: Option<&str>) -> Vec<MirrorEntry> {
    let Some(avoid) = avoid.map(str::trim).filter(|a| !a.is_empty()) else {
        return entries.to_vec();
    };
    let remaining: Vec<MirrorEntry> = entries.iter().filter(|e| e.id != avoid).cloned().collect();
    if remaining.is_empty() {
        entries.to_vec()
    } else {
        remaining
    }
}

// ---------------------------------------------------------------------------
// Downloads
// ---------------------------------------------------------------------------

/// `true` when `url` parses and uses the `https` scheme.
pub fn is_https(url: &str) -> bool {
    reqwest::Url::parse(url).is_ok_and(|u| u.scheme() == "https")
}

/// Validates a download file name: non-empty, no path separators, no `..`, no NUL/control
/// characters, no characters Windows forbids, at most [`MAX_FILE_NAME_LEN`] chars.
pub fn sanitize_file_name(name: &str) -> AppResult<String> {
    let trimmed = name.trim();
    let invalid = |why: &str| AppError::InvalidInput(format!("invalid file name: {why}"));
    if trimmed.is_empty() {
        return Err(invalid("empty"));
    }
    if trimmed.chars().count() > MAX_FILE_NAME_LEN {
        return Err(invalid("too long"));
    }
    if trimmed.contains(['/', '\\']) {
        return Err(invalid("path separators are not allowed"));
    }
    if trimmed == "." || trimmed.contains("..") {
        return Err(invalid("relative components are not allowed"));
    }
    if trimmed
        .chars()
        .any(|c| c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
    {
        return Err(invalid("forbidden characters"));
    }
    Ok(trimmed.to_owned())
}

/// Hex SHA-256 of an in-memory buffer (lower-case).
pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// Compares two hex digests case-insensitively, ignoring whitespace and an optional
/// `sha256:` prefix on either side.
pub fn hashes_match(expected: &str, actual: &str) -> bool {
    let norm = |s: &str| {
        let s = s.trim();
        let s = s
            .strip_prefix("sha256:")
            .or_else(|| s.strip_prefix("SHA256:"))
            .unwrap_or(s);
        s.trim().to_ascii_lowercase()
    };
    let (e, a) = (norm(expected), norm(actual));
    !e.is_empty() && e == a
}

/// Decides when a progress callback is due (time- or byte-based).
#[derive(Debug, Clone, Copy)]
struct ProgressThrottle {
    last_at: Instant,
    last_bytes: u64,
}

impl ProgressThrottle {
    fn new(now: Instant) -> Self {
        Self {
            last_at: now,
            last_bytes: 0,
        }
    }

    /// `true` (and resets) when at least [`PROGRESS_INTERVAL`] passed or [`PROGRESS_BYTES`]
    /// arrived since the previous report.
    fn due(&mut self, now: Instant, downloaded: u64) -> bool {
        let by_time = now.duration_since(self.last_at) >= PROGRESS_INTERVAL;
        let by_bytes = downloaded.saturating_sub(self.last_bytes) >= PROGRESS_BYTES;
        if by_time || by_bytes {
            self.last_at = now;
            self.last_bytes = downloaded;
            true
        } else {
            false
        }
    }
}

/// Streams the response body into `part_path`, hashing on the fly. Returns (bytes, sha256).
async fn stream_to_file<F>(
    response: reqwest::Response,
    part_path: &Path,
    on_progress: &mut F,
) -> AppResult<(u64, String)>
where
    F: FnMut(u64, Option<u64>) + Send,
{
    let total = response.content_length();
    let mut file = tokio::fs::File::create(part_path).await?;
    let mut stream = response.bytes_stream();
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut throttle = ProgressThrottle::new(Instant::now());
    on_progress(0, total);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        downloaded = downloaded.saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
        if throttle.due(Instant::now(), downloaded) {
            on_progress(downloaded, total);
        }
    }
    file.flush().await?;
    file.sync_all().await?;
    drop(file);
    on_progress(downloaded, total);
    Ok((downloaded, hex::encode(hasher.finalize())))
}

async fn discard(path: &Path) {
    if let Err(e) = tokio::fs::remove_file(path).await {
        if e.kind() != std::io::ErrorKind::NotFound {
            log::warn!("cannot remove partial download {}: {e}", path.display());
        }
    }
}

/// Downloads `req.url` (https only) to `dest_dir/<file_name>` via a `.part` temp file,
/// reporting progress and verifying SHA-256 against `req.expected_sha256` when given
/// (`AppError::HashMismatch` — the file is deleted). `on_progress(downloaded, total)` is
/// called first with `0`, then at most ~10×/s or every 256 KiB, and once more at the end.
pub async fn download<F>(
    client: &reqwest::Client,
    req: &DownloadRequest,
    dest_dir: PathBuf,
    on_progress: F,
) -> AppResult<DownloadResult>
where
    F: FnMut(u64, Option<u64>) + Send,
{
    if !is_https(&req.url) {
        return Err(AppError::InvalidInput(
            "downloads must use https".to_owned(),
        ));
    }
    download_verified(client, req, dest_dir, on_progress).await
}

/// `download` after the https gate (kept separate so the streaming/hash/rename logic can be
/// exercised against a loopback plain-HTTP server in tests).
async fn download_verified<F>(
    client: &reqwest::Client,
    req: &DownloadRequest,
    dest_dir: PathBuf,
    mut on_progress: F,
) -> AppResult<DownloadResult>
where
    F: FnMut(u64, Option<u64>) + Send,
{
    let file_name = sanitize_file_name(&req.file_name)?;
    tokio::fs::create_dir_all(&dest_dir).await?;
    let final_path = dest_dir.join(&file_name);
    let part_path = dest_dir.join(format!("{file_name}.part"));

    let response = client.get(&req.url).send().await?;
    if is_https(&req.url) && response.url().scheme() != "https" {
        // Belt and braces next to `download_client`'s `https_only`: never stream a body that
        // arrived over a downgraded redirect.
        return Err(AppError::InvalidInput(
            "download was redirected to a non-https URL".to_owned(),
        ));
    }
    let status = response.status();
    if !status.is_success() {
        return Err(AppError::Http {
            status: status.as_u16(),
            url: redact_secrets(&req.url),
        });
    }

    let (bytes, sha256) = match stream_to_file(response, &part_path, &mut on_progress).await {
        Ok(v) => v,
        Err(e) => {
            discard(&part_path).await;
            return Err(e);
        }
    };

    let verified = match req.expected_sha256.as_deref().map(str::trim) {
        Some(expected) if !expected.is_empty() => {
            if !hashes_match(expected, &sha256) {
                discard(&part_path).await;
                return Err(AppError::HashMismatch {
                    expected: expected.to_ascii_lowercase(),
                    actual: sha256,
                });
            }
            Some(true)
        }
        _ => None,
    };

    if let Err(e) = tokio::fs::rename(&part_path, &final_path).await {
        discard(&part_path).await;
        return Err(e.into());
    }
    log::info!(
        "downloaded {} ({bytes} bytes, verified: {verified:?})",
        final_path.display()
    );
    Ok(DownloadResult {
        path: final_path.to_string_lossy().into_owned(),
        bytes,
        sha256,
        verified,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, url: &str) -> MirrorEntry {
        MirrorEntry {
            id: id.to_owned(),
            url: url.to_owned(),
            download_page: String::new(),
        }
    }

    fn probe_result(url: &str, latency: Option<u64>) -> ProbeResult {
        ProbeResult {
            id: String::new(),
            url: url.to_owned(),
            reachable: latency.is_some(),
            latency_ms: latency,
            http_status: latency.map(|_| 200),
            error: None,
        }
    }

    #[test]
    fn select_mirror_table() {
        let entries = [
            entry("official", "https://official/"),
            entry("npmmirror", "https://mirror/"),
        ];
        let cases: [(&str, Option<u64>, Option<u64>, &str); 6] = [
            (
                "both reachable, official faster",
                Some(50),
                Some(80),
                "official",
            ),
            ("official within margin", Some(200), Some(80), "official"),
            ("official too slow", Some(400), Some(80), "npmmirror"),
            ("only mirror reachable", None, Some(80), "npmmirror"),
            ("only official reachable", Some(900), None, "official"),
            ("nothing reachable -> first entry", None, None, "official"),
        ];
        for (name, official, mirror, expected) in cases {
            let probes = [
                probe_result("https://official/", official),
                probe_result("https://mirror/", mirror),
            ];
            let chosen = select_mirror(&entries, &probes).expect("non-empty");
            assert_eq!(chosen.id, expected, "case: {name}");
        }
    }

    #[test]
    fn registries_avoiding_drops_the_failed_one_unless_it_is_alone() {
        let entries = [
            entry("official", "https://official/"),
            entry("npmmirror", "https://mirror/"),
        ];
        let ids = |v: Vec<MirrorEntry>| v.into_iter().map(|e| e.id).collect::<Vec<_>>();
        assert_eq!(
            ids(registries_avoiding(&entries, Some("npmmirror"))),
            vec!["official"]
        );
        assert_eq!(
            ids(registries_avoiding(&entries, Some("official"))),
            vec!["npmmirror"]
        );
        assert_eq!(
            ids(registries_avoiding(&entries, Some("unknown"))),
            vec!["official", "npmmirror"]
        );
        assert_eq!(
            ids(registries_avoiding(&entries, None)),
            vec!["official", "npmmirror"]
        );
        assert_eq!(
            ids(registries_avoiding(&entries, Some("  "))),
            vec!["official", "npmmirror"]
        );
        // the only registry stays even when it just failed
        assert_eq!(
            ids(registries_avoiding(&entries[..1], Some("official"))),
            vec!["official"]
        );
        // combined with selection: the failed (faster) mirror loses to the remaining one
        let probes = [
            probe_result("https://official/", Some(900)),
            probe_result("https://mirror/", Some(50)),
        ];
        let chosen = select_mirror(&registries_avoiding(&entries, Some("npmmirror")), &probes)
            .expect("non-empty");
        assert_eq!(chosen.id, "official");
    }

    #[test]
    fn select_mirror_without_official_picks_fastest() {
        let entries = [entry("a", "https://a/"), entry("b", "https://b/")];
        let probes = [
            probe_result("https://a/", Some(300)),
            probe_result("https://b/", Some(100)),
        ];
        assert_eq!(
            select_mirror(&entries, &probes).map(|e| e.id),
            Some("b".to_owned())
        );
        assert!(select_mirror(&[], &probes).is_none());
    }

    #[test]
    fn choose_mirrors_falls_back_to_defaults_for_empty_config() {
        let mirrors = Mirrors {
            npm_registries: vec![],
            node_dist: vec![],
            probe_timeout_ms: 0,
        };
        assert_eq!(probe_timeout(&mirrors), FALLBACK_PROBE_TIMEOUT);
        assert_eq!(default_npm_registry().id, "official");
        assert!(default_node_dist().url.starts_with("https://"));
    }

    #[test]
    fn file_name_sanitisation() {
        assert_eq!(
            sanitize_file_name("cc-switch_1.2.3_x64-setup.exe").expect("valid"),
            "cc-switch_1.2.3_x64-setup.exe"
        );
        assert_eq!(
            sanitize_file_name("  node.msi ").expect("trimmed"),
            "node.msi"
        );
        for bad in [
            "", "   ", "../evil", "a/b", "a\\b", "..", ".", "x..y", "a:b", "a\0b", "a|b",
        ] {
            assert!(
                matches!(sanitize_file_name(bad), Err(AppError::InvalidInput(_))),
                "should reject {bad:?}"
            );
        }
        let long = "x".repeat(MAX_FILE_NAME_LEN + 1);
        assert!(sanitize_file_name(&long).is_err());
    }

    #[test]
    fn sha256_of_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hash_comparison_is_case_and_prefix_insensitive() {
        let a = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(hashes_match(&a.to_uppercase(), a));
        assert!(hashes_match(&format!(" sha256:{a}\n"), a));
        assert!(!hashes_match("", a));
        assert!(!hashes_match("deadbeef", a));
    }

    #[test]
    fn https_only() {
        assert!(is_https("https://example.com/x.exe"));
        assert!(!is_https("http://example.com/x.exe"));
        assert!(!is_https("ftp://example.com/x"));
        assert!(!is_https("not a url"));
    }

    #[test]
    fn progress_throttle_by_time_and_bytes() {
        let t0 = Instant::now();
        let mut th = ProgressThrottle::new(t0);
        assert!(!th.due(t0 + Duration::from_millis(10), 1024));
        assert!(th.due(t0 + Duration::from_millis(20), PROGRESS_BYTES));
        assert!(!th.due(t0 + Duration::from_millis(30), PROGRESS_BYTES + 10));
        assert!(th.due(
            t0 + Duration::from_millis(30) + PROGRESS_INTERVAL,
            PROGRESS_BYTES + 10
        ));
    }

    #[test]
    fn truncate_keeps_short_strings() {
        assert_eq!(truncate_chars("abc", 5), "abc");
        assert_eq!(truncate_chars("abcdef", 3), "abc…");
    }

    #[tokio::test]
    async fn download_client_refuses_plain_http_even_before_the_gate() {
        let client = download_client().expect("client");
        let err = client
            .get("http://127.0.0.1:9/installer.exe")
            .send()
            .await
            .expect_err("https_only client must refuse http");
        assert!(err.is_builder(), "{err}");
    }

    #[test]
    fn client_builders_succeed() {
        assert!(build_client().is_ok());
        assert!(client_with_timeout(0).is_ok());
        assert!(download_client().is_ok());
        assert!(gateway_client(Duration::from_secs(45)).is_ok());
    }

    /// A gateway probe waits for the *response headers* while the model thinks, and reqwest
    /// applies the read timeout to that wait — so the default read timeout must not be the
    /// effective bound of a probe that is allowed to take longer.
    #[test]
    fn gateway_timeout_outlives_the_default_read_timeout() {
        assert!(crate::verify::GATEWAY_TIMEOUT > DEFAULT_READ_TIMEOUT);
    }

    #[tokio::test]
    async fn download_rejects_plain_http_and_bad_names_before_any_io() {
        let client = build_client().expect("client");
        let dir = tempfile::tempdir().expect("tempdir");
        let req = DownloadRequest {
            job_id: "j".into(),
            url: "http://127.0.0.1:9/x.exe".into(),
            file_name: "x.exe".into(),
            expected_sha256: None,
        };
        let r = download(&client, &req, dir.path().to_path_buf(), |_, _| {}).await;
        assert!(matches!(r, Err(AppError::InvalidInput(_))), "{r:?}");

        let req = DownloadRequest {
            url: "https://127.0.0.1:9/x.exe".into(),
            file_name: "../x.exe".into(),
            ..req
        };
        let r = download(&client, &req, dir.path().to_path_buf(), |_, _| {}).await;
        assert!(matches!(r, Err(AppError::InvalidInput(_))), "{r:?}");
        assert!(std::fs::read_dir(dir.path()).expect("dir").next().is_none());
    }

    /// Minimal one-shot HTTP/1.1 server on the loopback interface (std threads — no tokio
    /// `net` feature needed). Returns the base URL; serves `body` with the given status.
    fn serve_once(status: &'static str, body: Vec<u8>) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            for chunk in body.chunks(64 * 1024) {
                let _ = stream.write_all(chunk);
            }
            let _ = stream.flush();
        });
        format!("http://{addr}")
    }

    fn payload() -> Vec<u8> {
        (0..700 * 1024)
            .map(|i| u8::try_from(i % 251).unwrap_or(0))
            .collect()
    }

    #[tokio::test]
    async fn download_streams_hashes_and_renames() {
        let body = payload();
        let expected = sha256_hex(&body);
        let base = serve_once("200 OK", body.clone());
        let client = build_client().expect("client");
        let dir = tempfile::tempdir().expect("tempdir");
        let req = DownloadRequest {
            job_id: "j".into(),
            url: format!("{base}/pkg.bin"),
            file_name: "pkg.bin".into(),
            expected_sha256: Some(expected.to_uppercase()),
        };
        let progress = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = progress.clone();
        let result = download_verified(&client, &req, dir.path().to_path_buf(), move |d, t| {
            if let Ok(mut p) = sink.lock() {
                p.push((d, t));
            }
        })
        .await
        .expect("download");

        assert_eq!(result.bytes, u64::try_from(body.len()).expect("len"));
        assert_eq!(result.sha256, expected);
        assert_eq!(result.verified, Some(true));
        assert_eq!(std::fs::read(&result.path).expect("file"), body);
        assert!(!dir.path().join("pkg.bin.part").exists());
        let progress = progress.lock().expect("lock");
        assert_eq!(progress.first(), Some(&(0, Some(result.bytes))));
        assert_eq!(progress.last(), Some(&(result.bytes, Some(result.bytes))));
    }

    #[tokio::test]
    async fn download_deletes_file_on_hash_mismatch() {
        let base = serve_once("200 OK", payload());
        let client = build_client().expect("client");
        let dir = tempfile::tempdir().expect("tempdir");
        let req = DownloadRequest {
            job_id: "j".into(),
            url: format!("{base}/pkg.bin"),
            file_name: "pkg.bin".into(),
            expected_sha256: Some("00".repeat(32)),
        };
        let r = download_verified(&client, &req, dir.path().to_path_buf(), |_, _| {}).await;
        assert!(matches!(r, Err(AppError::HashMismatch { .. })), "{r:?}");
        assert!(std::fs::read_dir(dir.path()).expect("dir").next().is_none());
    }

    #[tokio::test]
    async fn download_reports_http_errors() {
        let base = serve_once("404 Not Found", b"nope".to_vec());
        let client = build_client().expect("client");
        let dir = tempfile::tempdir().expect("tempdir");
        let req = DownloadRequest {
            job_id: "j".into(),
            url: format!("{base}/missing.bin"),
            file_name: "missing.bin".into(),
            expected_sha256: None,
        };
        let r = download_verified(&client, &req, dir.path().to_path_buf(), |_, _| {}).await;
        assert!(
            matches!(r, Err(AppError::Http { status: 404, .. })),
            "{r:?}"
        );
        assert!(std::fs::read_dir(dir.path()).expect("dir").next().is_none());
    }

    #[tokio::test]
    async fn probe_reports_reachable_for_any_http_status_and_unreachable_otherwise() {
        let base = serve_once("404 Not Found", Vec::new());
        let client = build_client().expect("client");
        let ok = probe(&client, "t", &base, Duration::from_secs(5)).await;
        assert!(ok.reachable, "{ok:?}");
        assert_eq!(ok.http_status, Some(404));
        assert!(ok.latency_ms.is_some());

        // Port 9 (discard) on loopback is almost certainly closed → connection refused.
        let bad = probe(&client, "t", "http://127.0.0.1:9/", Duration::from_secs(3)).await;
        assert!(!bad.reachable);
        assert!(
            bad.error
                .as_deref()
                .is_some_and(|e| !e.contains("127.0.0.1")),
            "{bad:?}"
        );
    }
}
