//! M6 — help documentation, fetched from the designated docs site (PRD #18) with an on-disk
//! cache and a bundled fallback so the wizard is never without help text.
//!
//! Remote contract (to be confirmed with IT): `{docs.base_url}/{docs.index_path}` returns
//! `{ "sections": [ { "id", "title", "path", "lang", "wizardStep"?, "children"? } ] }`;
//! each `path` is a Markdown document relative to `base_url`.
//!
//! TODO(impl):
//! - `fetch_index(lang)`: remote → cache (`<app_cache_dir>/docs/<lang>/index.json`, TTL
//!   `cache_ttl_seconds`) → bundled (`resources/docs/<lang>/index.json`); set `source`
//! - `fetch_page(id, lang)`: same cascade for `<path>`; sanitise: reject non-Markdown content
//!   types, cap size at 2 MB
//! - bundled fallback lives in `src-tauri/resources/docs/{zh-CN,en}/` (add to tauri.conf
//!   `bundle.resources` when created)

use std::path::PathBuf;

use crate::error::{AppError, AppResult};
use crate::models::{DocPage, DocsConfig, DocsIndex};

#[derive(Debug, Clone)]
pub struct DocsContext {
    pub config: DocsConfig,
    pub http: reqwest::Client,
    pub cache_dir: PathBuf,
    pub resource_dir: Option<PathBuf>,
}

pub async fn fetch_index(ctx: &DocsContext, lang: &str) -> AppResult<DocsIndex> {
    let _ = (ctx, lang);
    Err(AppError::Other("docs::fetch_index not implemented".into()))
}

pub async fn fetch_page(ctx: &DocsContext, id: &str, lang: &str) -> AppResult<DocPage> {
    let _ = (ctx, id, lang);
    Err(AppError::Other("docs::fetch_page not implemented".into()))
}
