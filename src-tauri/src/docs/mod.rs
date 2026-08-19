//! M6 — help documentation, fetched from the designated docs site (PRD #18) with an on-disk
//! cache and a bundled fallback so the wizard is never without help text.
//!
//! # Remote contract (what the docs site must publish)
//!
//! ```text
//! {docs.baseUrl}/{lang}/{docs.indexPath}      e.g. https://docs.example.com/onboarding/zh-CN/index.json
//! {docs.baseUrl}/{lang}/{section.path}        e.g. https://docs.example.com/onboarding/zh-CN/verify.md
//! ```
//!
//! - `lang` is `zh-CN` or `en` ([`resolve_lang`] maps anything else to `en`).
//! - `index.json` = `{ "sections": [ { "id", "title", "path", "lang", "wizardStep"?, "children"? } ] }`
//!   where `path` is a Markdown file **relative to the language folder**. Section ids and paths
//!   must be plain relative names (`[A-Za-z0-9._-]`, `/` separators, no `..`, no absolute parts);
//!   offending sections are dropped when the index is loaded.
//! - Pages must be served as `text/*` or `application/octet-stream` (index: also
//!   `application/json`), UTF-8, at most 2 MiB (index 1 MiB).
//!
//! The same layout is shipped in `src-tauri/resources/docs/` (see its `README.md`) and embedded
//! into the binary by [`bundled`].
//!
//! # Resolution cascade (identical for index and pages)
//!
//! 1. cache `<app_cache_dir>/docs/<lang>/<path>` if younger than `docs.cacheTtlSeconds`
//!    → [`DocsSource::Cache`], no network;
//! 2. remote (10 s timeout, size / content-type enforced) → written to the cache
//!    → [`DocsSource::Remote`];
//! 3. stale cache → [`DocsSource::Cache`];
//! 4. bundled: `<resource_dir>/<lang>/<path>` (patchable post-build) or the embedded copy
//!    → [`DocsSource::Bundled`];
//! 5. otherwise the remote error is returned (`Http { 404, .. }`, `Network`, …).
//!
//! After a remote failure the site is not retried for [`REMOTE_BACKOFF`] so an unreachable
//! host does not add a timeout to every page the user opens.
//!
//! Every title / Markdown body that reaches the UI passes through
//! [`crate::redact::redact_secrets`] regardless of source.

pub mod bundled;
pub mod cache;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use futures_util::StreamExt;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::models::{DocPage, DocSection, DocsConfig, DocsIndex, DocsSource};
use crate::redact::redact_secrets;

/// Maximum accepted size of a Markdown page.
pub const PAGE_MAX_BYTES: usize = 2 * 1024 * 1024;
/// Maximum accepted size of `index.json`.
pub const INDEX_MAX_BYTES: usize = 1024 * 1024;
/// Per-request timeout for the docs site.
pub const REMOTE_TIMEOUT: Duration = Duration::from_secs(10);
/// How long the remote is skipped after a failure.
pub const REMOTE_BACKOFF: Duration = Duration::from_secs(60);
/// Languages with bundled content; the first entry is the default.
pub const SUPPORTED_LANGS: [&str; 2] = ["zh-CN", "en"];
const DEFAULT_INDEX_PATH: &str = "index.json";
const MAX_ID_LEN: usize = 64;
const MAX_PATH_LEN: usize = 256;

/// Everything the docs module needs from the app: config, shared HTTP client, cache root
/// (`<app_cache_dir>/docs`) and, when running from a bundle, the resource root
/// (`<resource_dir>/resources/docs`).
#[derive(Debug, Clone)]
pub struct DocsContext {
    pub config: DocsConfig,
    pub http: reqwest::Client,
    pub cache_dir: PathBuf,
    pub resource_dir: Option<PathBuf>,
}

/// Maps a UI language tag to a supported docs language: `zh*` → `zh-CN`, anything else → `en`.
pub fn resolve_lang(lang: &str) -> &'static str {
    let lower = lang.trim().to_ascii_lowercase();
    if lower == "zh" || lower.starts_with("zh-") || lower.starts_with("zh_") {
        "zh-CN"
    } else {
        "en"
    }
}

/// Fetches the section index for `lang` through the cascade described in the module docs.
pub async fn fetch_index(ctx: &DocsContext, lang: &str) -> AppResult<DocsIndex> {
    let lang = resolve_lang(lang);
    let index_path = index_path(&ctx.config);
    let fetched = resolve(ctx, lang, index_path, DocKind::Index).await?;
    let sections = parse_index(&fetched.text)?;
    Ok(DocsIndex {
        sections,
        fetched_at: rfc3339(fetched.at),
        source: fetched.source,
    })
}

/// Fetches the Markdown page with section `id` for `lang`. The id is looked up in the resolved
/// index; unknown or unsafe ids are rejected with [`AppError::InvalidInput`].
pub async fn fetch_page(ctx: &DocsContext, id: &str, lang: &str) -> AppResult<DocPage> {
    if !is_safe_id(id) {
        return Err(AppError::InvalidInput("invalid doc id".into()));
    }
    let lang = resolve_lang(lang);
    let index = fetch_index(ctx, lang).await?;
    let section = find_section(&index.sections, id)
        .ok_or_else(|| AppError::InvalidInput(format!("unknown doc id `{id}`")))?;
    let fetched = resolve(ctx, lang, &section.path, DocKind::Page).await?;
    Ok(DocPage {
        id: section.id.clone(),
        title: section.title.clone(),
        lang: lang.to_owned(),
        markdown: fetched.text,
        source: fetched.source,
        fetched_at: rfc3339(fetched.at),
    })
}

// ---------------------------------------------------------------------------
// path safety
// ---------------------------------------------------------------------------

fn is_safe_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')
}

fn is_safe_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && !segment.contains("..")
        && segment.chars().all(is_safe_char)
}

/// A section id: one plain segment, at most 64 chars, no `..`.
pub fn is_safe_id(id: &str) -> bool {
    id.len() <= MAX_ID_LEN && is_safe_segment(id)
}

/// A relative document path: `/`-separated safe segments, no leading `/`, no `\`, no drive
/// letters, no `.`/`..` components, at most 256 chars.
pub fn is_safe_rel_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= MAX_PATH_LEN
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path.split('/').all(is_safe_segment)
}

// ---------------------------------------------------------------------------
// index handling
// ---------------------------------------------------------------------------

/// Shape of `index.json` on the site / in the bundle (no `fetchedAt` / `source`).
#[derive(Debug, Deserialize)]
struct IndexFile {
    sections: Vec<DocSection>,
}

/// Parses an index document, drops sections with unsafe ids/paths and redacts titles.
pub fn parse_index(json: &str) -> AppResult<Vec<DocSection>> {
    let file: IndexFile = serde_json::from_str(json)?;
    Ok(sanitize_sections(file.sections))
}

fn sanitize_sections(sections: Vec<DocSection>) -> Vec<DocSection> {
    sections
        .into_iter()
        .filter(|s| {
            let ok = is_safe_id(&s.id) && is_safe_rel_path(&s.path);
            if !ok {
                log::warn!(
                    "docs: dropping section with unsafe id/path: {}",
                    redact_secrets(&s.id)
                );
            }
            ok
        })
        .map(|mut s| {
            s.title = redact_secrets(&s.title);
            s.children = sanitize_sections(std::mem::take(&mut s.children));
            s
        })
        .collect()
}

/// Depth-first lookup of a section by id (children included).
pub fn find_section<'a>(sections: &'a [DocSection], id: &str) -> Option<&'a DocSection> {
    sections.iter().find_map(|s| {
        if s.id == id {
            Some(s)
        } else {
            find_section(&s.children, id)
        }
    })
}

fn index_path(cfg: &DocsConfig) -> &str {
    let p = cfg.index_path.trim().trim_start_matches('/');
    if is_safe_rel_path(p) {
        p
    } else {
        log::warn!("docs: invalid docs.indexPath in config, using `{DEFAULT_INDEX_PATH}`");
        DEFAULT_INDEX_PATH
    }
}

// ---------------------------------------------------------------------------
// cascade
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocKind {
    Index,
    Page,
}

impl DocKind {
    fn max_bytes(self) -> usize {
        match self {
            DocKind::Index => INDEX_MAX_BYTES,
            DocKind::Page => PAGE_MAX_BYTES,
        }
    }

    fn accept(self) -> &'static str {
        match self {
            DocKind::Index => "application/json, text/plain;q=0.9, */*;q=0.1",
            DocKind::Page => "text/markdown, text/plain;q=0.9, */*;q=0.1",
        }
    }

    /// Content types the site may use for this kind of document.
    fn content_type_ok(self, content_type: &str) -> bool {
        let mime = content_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let generic = mime.starts_with("text/") || mime == "application/octet-stream";
        match self {
            DocKind::Index => generic || mime == "application/json",
            DocKind::Page => generic,
        }
    }

    /// Rejects remote content that must never be cached (an index that does not parse).
    fn validate(self, text: &str) -> AppResult<()> {
        match self {
            DocKind::Index => parse_index(text).map(|_| ()),
            DocKind::Page => Ok(()),
        }
    }
}

#[derive(Debug)]
struct Resolved {
    text: String,
    source: DocsSource,
    at: DateTime<Utc>,
}

/// Runs the remote → cache → bundled cascade for one document.
async fn resolve(
    ctx: &DocsContext,
    lang: &str,
    rel_path: &str,
    kind: DocKind,
) -> AppResult<Resolved> {
    if !is_safe_rel_path(rel_path) {
        return Err(AppError::InvalidInput("invalid doc path".into()));
    }
    let cached = cache::read(&ctx.cache_dir, lang, rel_path).await;
    if let Some(c) = &cached {
        if cache::is_fresh(c.stored_at, cache::now_secs(), ctx.config.cache_ttl_seconds) {
            return Ok(from_cache(c));
        }
    }

    let remote_err = match fetch_remote(ctx, lang, rel_path, kind).await {
        Ok(text) => {
            if let Err(e) = cache::write(&ctx.cache_dir, lang, rel_path, &text).await {
                log::debug!("docs: cache write failed for {lang}/{rel_path}: {e}");
            }
            return Ok(Resolved {
                text: redact_secrets(&text),
                source: DocsSource::Remote,
                at: Utc::now(),
            });
        }
        Err(e) => e,
    };
    log::debug!("docs: remote unavailable for {lang}/{rel_path}: {remote_err}");

    if let Some(c) = &cached {
        return Ok(from_cache(c));
    }
    if let Some(text) = load_bundled(ctx.resource_dir.as_deref(), lang, rel_path, kind).await {
        return Ok(Resolved {
            text: redact_secrets(&text),
            source: DocsSource::Bundled,
            at: Utc::now(),
        });
    }
    Err(remote_err)
}

fn from_cache(c: &cache::CachedFile) -> Resolved {
    let at = if c.stored_at == 0 {
        Utc::now()
    } else {
        Utc.timestamp_opt(i64::try_from(c.stored_at).unwrap_or(0), 0)
            .single()
            .unwrap_or_else(Utc::now)
    };
    Resolved {
        text: redact_secrets(&c.text),
        source: DocsSource::Cache,
        at,
    }
}

/// Resource-dir copy first (IT may patch it post-build), then the embedded copy. The embedded
/// index is used for `DocKind::Index` whatever `docs.indexPath` is configured to.
async fn load_bundled(
    resource_dir: Option<&Path>,
    lang: &str,
    rel_path: &str,
    kind: DocKind,
) -> Option<String> {
    if let Some(dir) = resource_dir {
        let file = cache::file_path(dir, lang, rel_path);
        if let Ok(text) = tokio::fs::read_to_string(&file).await {
            return Some(text);
        }
    }
    let embedded = match kind {
        DocKind::Index => bundled::index(lang),
        DocKind::Page => bundled::page(lang, rel_path),
    };
    embedded.map(str::to_owned)
}

// ---------------------------------------------------------------------------
// remote
// ---------------------------------------------------------------------------

static REMOTE_FAILED_AT: Mutex<Option<Instant>> = Mutex::new(None);

fn remote_in_backoff() -> bool {
    REMOTE_FAILED_AT
        .lock()
        .ok()
        .and_then(|g| *g)
        .is_some_and(|t| t.elapsed() < REMOTE_BACKOFF)
}

fn note_remote_result(ok: bool) {
    if let Ok(mut g) = REMOTE_FAILED_AT.lock() {
        *g = if ok { None } else { Some(Instant::now()) };
    }
}

fn remote_url(cfg: &DocsConfig, lang: &str, rel_path: &str) -> Option<String> {
    let base = cfg.base_url.trim().trim_end_matches('/');
    if !(base.starts_with("https://") || base.starts_with("http://")) {
        return None;
    }
    Some(format!("{base}/{lang}/{rel_path}"))
}

async fn fetch_remote(
    ctx: &DocsContext,
    lang: &str,
    rel_path: &str,
    kind: DocKind,
) -> AppResult<String> {
    let url = remote_url(&ctx.config, lang, rel_path)
        .ok_or_else(|| AppError::Config("docs.baseUrl is not an http(s) URL".into()))?;
    if remote_in_backoff() {
        return Err(AppError::Network("docs_remote_backoff".into()));
    }
    let result = fetch_remote_url(&ctx.http, &url, kind).await;
    // Only transport-level failures trigger the backoff; a 404 for one page says nothing about
    // the site being down.
    match &result {
        Ok(_) => note_remote_result(true),
        Err(AppError::Network(_)) => note_remote_result(false),
        Err(_) => {}
    }
    result
}

async fn fetch_remote_url(http: &reqwest::Client, url: &str, kind: DocKind) -> AppResult<String> {
    let resp = http
        .get(url)
        .timeout(REMOTE_TIMEOUT)
        .header(ACCEPT, kind.accept())
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(AppError::Http {
            status: status.as_u16(),
            url: url.to_owned(),
        });
    }
    if let Some(ct) = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    {
        if !kind.content_type_ok(ct) {
            return Err(AppError::Other(format!(
                "docs: unexpected content-type `{}`",
                redact_secrets(ct)
            )));
        }
    }
    let max = kind.max_bytes();
    if resp
        .content_length()
        .is_some_and(|n| usize::try_from(n).ok().is_none_or(|n| n > max))
    {
        return Err(AppError::Other("docs: document exceeds size limit".into()));
    }
    let bytes = read_capped(resp, max).await?;
    let text = String::from_utf8(bytes)
        .map_err(|_| AppError::Other("docs: document is not valid UTF-8".into()))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned();
    kind.validate(&text)?;
    Ok(text)
}

async fn read_capped(resp: reqwest::Response, max: usize) -> AppResult<Vec<u8>> {
    let mut stream = resp.bytes_stream();
    let mut buf = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if buf.len() + chunk.len() > max {
            return Err(AppError::Other("docs: document exceeds size limit".into()));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

fn rfc3339(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::WizardStep;

    /// Short connect timeout so the closed-port remote fails fast on every platform.
    fn test_client() -> reqwest::Client {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_millis(300))
            .build()
            .expect("client")
    }

    /// A context whose remote is a closed local port: fails fast, never reachable.
    fn ctx(cache_dir: &Path, ttl: u64) -> DocsContext {
        DocsContext {
            config: DocsConfig {
                base_url: "http://127.0.0.1:1/docs/".into(),
                index_path: "index.json".into(),
                cache_ttl_seconds: ttl,
            },
            http: test_client(),
            cache_dir: cache_dir.to_path_buf(),
            resource_dir: None,
        }
    }

    #[test]
    fn language_resolution() {
        for (input, expected) in [
            ("zh-CN", "zh-CN"),
            ("zh", "zh-CN"),
            ("zh-TW", "zh-CN"),
            ("ZH_cn", "zh-CN"),
            ("en", "en"),
            ("en-US", "en"),
            ("fr", "en"),
            ("", "en"),
        ] {
            assert_eq!(resolve_lang(input), expected, "{input}");
        }
    }

    #[test]
    fn id_safety() {
        for ok in ["overview", "install-cli", "faq_v2", "a.b", "A1"] {
            assert!(is_safe_id(ok), "{ok}");
        }
        for bad in [
            "", ".", "..", "a..b", "a/b", "a\\b", "../x", "/abs", "a b", "a:b", "é",
        ] {
            assert!(!is_safe_id(bad), "{bad}");
        }
        assert!(!is_safe_id(&"x".repeat(MAX_ID_LEN + 1)));
    }

    #[test]
    fn path_safety() {
        for ok in ["overview.md", "guides/verify.md", "a/b/c.md", "x-y_z.1.md"] {
            assert!(is_safe_rel_path(ok), "{ok}");
        }
        for bad in [
            "",
            "/etc/passwd",
            "../index.json",
            "a/../b.md",
            "a/./b.md",
            "a//b.md",
            "C:/x.md",
            "a\\b.md",
            "trailing/",
            "sp ace.md",
            "a?.md",
        ] {
            assert!(!is_safe_rel_path(bad), "{bad}");
        }
        assert!(!is_safe_rel_path(&"x".repeat(MAX_PATH_LEN + 1)));
    }

    #[test]
    fn content_type_rules() {
        assert!(DocKind::Page.content_type_ok("text/markdown; charset=utf-8"));
        assert!(DocKind::Page.content_type_ok("TEXT/PLAIN"));
        assert!(DocKind::Page.content_type_ok("application/octet-stream"));
        assert!(!DocKind::Page.content_type_ok("application/json"));
        assert!(!DocKind::Page.content_type_ok("image/png"));
        assert!(DocKind::Index.content_type_ok("application/json; charset=utf-8"));
        assert!(DocKind::Index.content_type_ok("text/plain"));
        assert!(!DocKind::Index.content_type_ok("application/xml"));
        assert!(!DocKind::Index.content_type_ok("video/mp4"));
    }

    #[test]
    fn embedded_indexes_parse_with_expected_sections() {
        for lang in SUPPORTED_LANGS {
            let json = bundled::index(lang).expect("embedded index");
            let sections = parse_index(json).expect("index parses");
            let ids: Vec<&str> = sections.iter().map(|s| s.id.as_str()).collect();
            assert_eq!(
                ids,
                [
                    "overview",
                    "env-check",
                    "one-click",
                    "install-node",
                    "install-cli",
                    "install-cc-switch",
                    "configure-cc-switch",
                    "verify",
                    "troubleshooting",
                    "faq"
                ],
                "{lang}"
            );
            for s in &sections {
                assert_eq!(s.lang, lang, "{lang}/{}", s.id);
                assert!(!s.title.trim().is_empty(), "{lang}/{}", s.id);
            }
            let step = |id: &str| find_section(&sections, id).and_then(|s| s.wizard_step);
            assert_eq!(step("overview"), Some(WizardStep::Welcome));
            assert_eq!(step("env-check"), Some(WizardStep::EnvCheck));
            assert_eq!(step("install-cli"), Some(WizardStep::Install));
            assert_eq!(step("configure-cc-switch"), Some(WizardStep::Configure));
            assert_eq!(step("verify"), Some(WizardStep::Verify));
            assert_eq!(step("troubleshooting"), Some(WizardStep::Diagnose));
            assert_eq!(step("faq"), None);
        }
    }

    #[test]
    fn every_index_entry_is_embedded() {
        fn walk(lang: &str, sections: &[DocSection]) {
            for s in sections {
                let page = bundled::page(lang, &s.path);
                assert!(
                    page.is_some(),
                    "{lang}/{} ({}) is not embedded",
                    s.path,
                    s.id
                );
                assert!(
                    page.unwrap_or_default().starts_with("# "),
                    "{lang}/{} must start with an H1",
                    s.path
                );
                walk(lang, &s.children);
            }
        }
        for lang in SUPPORTED_LANGS {
            let sections = parse_index(bundled::index(lang).expect("index")).expect("parse");
            walk(lang, &sections);
        }
    }

    #[test]
    fn sanitize_drops_unsafe_sections_and_recurses() {
        let json = r#"{"sections":[
            {"id":"ok","title":"T","path":"ok.md","lang":"en","children":[
                {"id":"child","title":"C","path":"sub/child.md","lang":"en"},
                {"id":"../evil","title":"E","path":"x.md","lang":"en"}
            ]},
            {"id":"bad","title":"B","path":"/etc/passwd","lang":"en"},
            {"id":"bad2","title":"B","path":"..\\win.md","lang":"en"}
        ]}"#;
        let sections = parse_index(json).expect("parse");
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].children.len(), 1);
        assert!(find_section(&sections, "child").is_some());
        assert!(find_section(&sections, "bad").is_none());
    }

    #[test]
    fn index_path_falls_back_when_unsafe() {
        let mut cfg = DocsConfig {
            base_url: String::new(),
            index_path: "/index.json".into(),
            cache_ttl_seconds: 1,
        };
        assert_eq!(index_path(&cfg), "index.json");
        cfg.index_path = "../index.json".into();
        assert_eq!(index_path(&cfg), "index.json");
        cfg.index_path = "v2/index.json".into();
        assert_eq!(index_path(&cfg), "v2/index.json");
    }

    #[test]
    fn remote_url_requires_http_scheme() {
        let cfg = DocsConfig {
            base_url: "https://docs.example.com/onboarding/".into(),
            index_path: "index.json".into(),
            cache_ttl_seconds: 1,
        };
        assert_eq!(
            remote_url(&cfg, "en", "index.json").as_deref(),
            Some("https://docs.example.com/onboarding/en/index.json")
        );
        let bad = DocsConfig {
            base_url: "ftp://x".into(),
            ..cfg
        };
        assert!(remote_url(&bad, "en", "index.json").is_none());
    }

    #[tokio::test]
    async fn unreachable_remote_falls_back_to_bundled() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = ctx(dir.path(), 3600);
        let index = fetch_index(&ctx, "zh-CN").await.expect("index");
        assert_eq!(index.source, DocsSource::Bundled);
        assert_eq!(index.sections.len(), 10);

        let page = fetch_page(&ctx, "verify", "zh").await.expect("page");
        assert_eq!(page.source, DocsSource::Bundled);
        assert_eq!(page.lang, "zh-CN");
        assert_eq!(page.id, "verify");
        assert!(page.markdown.starts_with("# "));
        assert!(!page.title.is_empty());
    }

    #[tokio::test]
    async fn fresh_cache_is_served_without_network() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = ctx(dir.path(), 3600);
        let json = r#"{"sections":[{"id":"only","title":"Cached","path":"only.md","lang":"en"}]}"#;
        cache::write(dir.path(), "en", "index.json", json)
            .await
            .expect("write");
        cache::write(dir.path(), "en", "only.md", "# From cache")
            .await
            .expect("write");

        let index = fetch_index(&ctx, "en").await.expect("index");
        assert_eq!(index.source, DocsSource::Cache);
        assert_eq!(index.sections.len(), 1);

        let page = fetch_page(&ctx, "only", "en").await.expect("page");
        assert_eq!(page.source, DocsSource::Cache);
        assert_eq!(page.markdown, "# From cache");
        assert_eq!(page.title, "Cached");
    }

    #[tokio::test]
    async fn stale_cache_is_served_when_remote_fails() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = ctx(dir.path(), 0); // ttl 0 => cache is never fresh
        let json = r#"{"sections":[{"id":"stale","title":"Stale","path":"stale.md","lang":"en"}]}"#;
        cache::write(dir.path(), "en", "index.json", json)
            .await
            .expect("write");

        let index = fetch_index(&ctx, "en").await.expect("index");
        assert_eq!(index.source, DocsSource::Cache);
        assert_eq!(index.sections[0].id, "stale");
        // page is neither cached nor bundled -> the remote error surfaces
        assert!(fetch_page(&ctx, "stale", "en").await.is_err());
    }

    #[tokio::test]
    async fn resource_dir_copy_wins_over_embedded() {
        let cache = tempfile::tempdir().expect("tempdir");
        let res = tempfile::tempdir().expect("tempdir");
        let mut ctx = ctx(cache.path(), 3600);
        ctx.resource_dir = Some(res.path().to_path_buf());
        tokio::fs::create_dir_all(res.path().join("en"))
            .await
            .expect("mkdir");
        tokio::fs::write(res.path().join("en").join("faq.md"), "# Patched FAQ")
            .await
            .expect("write");

        let page = fetch_page(&ctx, "faq", "en").await.expect("page");
        assert_eq!(page.source, DocsSource::Bundled);
        assert_eq!(page.markdown, "# Patched FAQ");
        let other = fetch_page(&ctx, "overview", "en").await.expect("page");
        assert!(other.markdown.starts_with("# Welcome"));
    }

    #[tokio::test]
    async fn unknown_or_unsafe_ids_are_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = ctx(dir.path(), 3600);
        assert!(matches!(
            fetch_page(&ctx, "../secret", "en").await,
            Err(AppError::InvalidInput(_))
        ));
        assert!(matches!(
            fetch_page(&ctx, "does-not-exist", "en").await,
            Err(AppError::InvalidInput(_))
        ));
    }
}
